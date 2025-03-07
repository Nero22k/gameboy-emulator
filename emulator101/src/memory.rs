use crate::interrupts::{InterruptController, InterruptType};
use crate::timer::Timer;
use crate::clock::Clock;
use crate::ppu::Ppu;
use sdl2::keyboard::Keycode;

// Joypad button enum
#[derive(Debug, Clone, Copy)]
pub enum JoypadButton {
    // D-pad
    Right,
    Left,
    Up,
    Down,
    
    // Buttons
    A,
    B,
    Select,
    Start,
}

pub struct MemoryBus<'a> {
    // Basic memory regions
    wram: [u8; 0x2000],       // 8KB Working RAM (0xC000-0xDFFF)
    hram: [u8; 0x7F],         // High RAM (0xFF80-0xFFFE)
    io_registers: [u8; 0x80],  // I/O registers (0xFF00-0xFF7F)
    ie_register: u8,           // Interrupt Enable register (0xFFFF)
    
    // ROM and external RAM - these would be in the cartridge
    rom: &'a [u8],            // ROM data reference
    eram: Vec<u8>,            // External RAM
    
    // Interrupt controller
    int_ctrl: InterruptController,

    // Timer component
    timer: Timer,

    // PPU component
    pub ppu: Ppu,

    // Joypad state
    joypad_select: u8,  // Joypad selection (buttons or d-pad)
    joypad_buttons: u8, // State of buttons (A, B, Select, Start)
    joypad_dpad: u8,    // State of D-pad (Right, Left, Up, Down)
    last_joypad_state: u8,
    joypad_debounce_counter: u8,
    joypad_debounce_delay: u8,
    
    // Serial output for tests
    serial_data: u8,           // SB register (0xFF01)
    serial_control: u8,        // SC register (0xFF02)
    serial_transfer_active: bool,
    serial_bit_counter: u8,
    serial_clock_counter: u16,

    clock: Clock,
    memory_access_in_progress: bool,
    memory_access_cycles_remaining: u8,
    memory_access_type: MemoryAccessType,
    memory_access_address: u16,
    memory_access_value: Option<u8>,
}

#[derive(PartialEq)]
enum MemoryAccessType {
    Read,
    Write,
    DMA,
    None,
}

// Lifetime 'a is used to ensure that the ROM data reference is valid for the lifetime of the MemoryBus instance.
// This is necessary because the ROM data is stored in the cartridge and is not owned by the MemoryBus.
impl<'a> MemoryBus<'a> {
    pub fn new(rom: &'a [u8]) -> Self {
        let mut mmu = Self {
            wram: [0; 0x2000],
            hram: [0; 0x7F],
            io_registers: [0; 0x80],
            ie_register: 0,
            rom,
            eram: vec![0; 0x2000], // 8KB external RAM
            int_ctrl: InterruptController::new(),
            timer: Timer::new(),
            ppu: Ppu::new(),
            joypad_select: 0xCF, // Both button and direction selected (P14 and P15 high)
            joypad_buttons: 0x0F, // All buttons released
            joypad_dpad: 0x0F,    // All d-pad released
            last_joypad_state: 0xCF,
            joypad_debounce_counter: 0,
            joypad_debounce_delay: 1,
            serial_data: 0,
            serial_control: 0x7E,
            serial_transfer_active: false,
            serial_bit_counter: 0,
            serial_clock_counter: 0,
            clock: Clock::new(),
            memory_access_in_progress: false,
            memory_access_cycles_remaining: 0,
            memory_access_type: MemoryAccessType::None,
            memory_access_address: 0,
            memory_access_value: None,
        };
        mmu.io_registers[0x0F] = 0xE1; // Set if register to post boot value
        mmu
    }

    // Update timer for a single cycle
    pub fn update_timer_cycle(&mut self) -> bool {
        self.timer.update_cycle()
    }
    
    // Update PPU for a single cycle
    pub fn update_ppu_cycle(&mut self) -> Option<InterruptType> {
        self.ppu.update_cycle()
    }
    
    // Update serial for a single cycle
    pub fn update_serial_cycle(&mut self) -> bool {
        // Skip if transfer not active
        if !self.serial_transfer_active {
            return false;
        }
        
        // Only handle internal clock (bit 0 of SC set)
        if self.serial_control & 0x01 != 0 {
            // Update clock counter
            self.serial_clock_counter = self.serial_clock_counter.wrapping_add(1);
            
            // Each bit takes 512 T-cycles at normal speed
            if self.serial_clock_counter == 512 {
                self.serial_clock_counter -= 512;
                
                // Shift out a bit
                self.serial_bit_counter += 1;
                self.serial_data = (self.serial_data << 1) | 1; // Shift in 1s (no cable connected)
                
                // After 8 bits, transfer is complete
                if self.serial_bit_counter == 8 {
                    // Reset transfer
                    self.serial_transfer_active = false;
                    self.serial_bit_counter = 0;
                    
                    // Clear transfer bit (7) in SC
                    self.serial_control &= 0x7F;
                    
                    // Request serial interrupt
                    return true;
                }
            }
        }
        
        false
    }
    
    // Update joypad_cycle to only check for interrupts
    pub fn update_joypad_cycle(&mut self) -> bool {
        // Joypad is usually edge-triggered, so we only need to check for changes
        // This is a simplified implementation
        if self.joypad_debounce_counter > 0 {
            self.joypad_debounce_counter -= 1;
        }
        
        // In a real implementation, you'd check for changes in button state here
        // For now, just return false (no interrupt)
        false
    }
    
    // Process one DMA cycle
    pub fn process_dma_cycle(&mut self) {
        if !self.ppu.oam_dma_active {
            return;
        }
        
        // Get current byte position
        let byte_pos = self.ppu.oam_dma_byte;
        let addr = self.ppu.oam_dma_source + (byte_pos as u16);
        
        // Read the byte from memory (bypass memory access timing for DMA)
        let value = match addr {
            // Direct memory access implementation without setting memory_access_in_progress
            // (Copy your read_byte logic here but without the timing code)
            // ROM bank 0 (0x0000-0x3FFF)
            0x0000..=0x3FFF => {
                if addr as usize >= self.rom.len() {
                    0xFF
                } else {
                    self.rom[addr as usize]
                }
            },
            // ROM bank 1-N (0x4000-0x7FFF)
            0x4000..=0x7FFF => {
                // The correct calculation depends on your MBC implementation
                // For simple cases with no banking, it would be:
                let rom_addr = addr as usize;
                if rom_addr >= self.rom.len() {
                    0xFF
                } else {
                    self.rom[rom_addr]
                }
                // For MBC implementations, you'd calculate the correct bank
            },
            // VRAM (0x8000-0x9FFF)
            0x8000..=0x9FFF => self.ppu.read_vram(addr),
            // External RAM (0xC000-0xDFFF)
            0xA000..=0xBFFF => {
                let addr = (addr - 0xA000) as u16;
                if (addr as u16) < self.eram.len() as u16 {
                    self.eram[addr as usize]
                } else {
                    0xFF
                }
            },
            // Working RAM (0xC000-0xDFFF)
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize],
            
            // Echo RAM (0xE000-0xFDFF)
            0xE000..=0xFDFF => self.wram[(addr - 0xE000) as usize],

            // OAM (0xFE00-0xFE9F)
            0xFE00..=0xFE9F => self.ppu.read_oam(addr),
            
            // I/O Registers (0xFF00-0xFF7F)
            0xFF00..=0xFF7F => self.read_io(addr),
            
            // High RAM (0xFF80-0xFFFE)
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize],
            
            // Interrupt Enable
            0xFFFF => self.get_ie(),
            
            // Unused memory regions
            _ => 0xFF,
        };
        
        self.ppu.write_oam_internal(byte_pos as u16, value);
        
        // Increment byte position
        self.ppu.oam_dma_byte += 1;
        
        // Check if DMA is complete
        if self.ppu.oam_dma_byte >= 0xA0 {
            self.ppu.oam_dma_active = false;
            self.ppu.oam_dma_byte = 0;
            // DMA is complete, but memory is still blocked for a few more cycles
        }
    }

    pub fn begin_oam_dma(&mut self, value: u8) {        
        // Start DMA in PPU
        self.ppu.begin_oam_dma(value);
        
        // Block CPU access to most memory during DMA
        self.memory_access_in_progress = true;
        self.memory_access_type = MemoryAccessType::DMA; // Add this enum variant
        self.memory_access_cycles_remaining = 160; // 160 m-cycles
    }

    pub fn tick(&mut self) -> Option<InterruptType> {
        let mut interrupt = None;
        
        // Process memory access if in progress
        if self.memory_access_in_progress {
            self.memory_access_cycles_remaining -= 1;
            
            if self.memory_access_cycles_remaining == 0 {
                self.memory_access_in_progress = false;
                self.memory_access_type = MemoryAccessType::None;
            }
        }

        // Process DMA transfers (one byte per m-cycle)
        if self.ppu.oam_dma_active {
            self.process_dma_cycle();
        }
        
        // (4 t-cycles = 1 m-cycle)
        for _ in 0..4 {
            if self.update_timer_cycle() {
                self.request_interrupt(InterruptType::Timer);
            }
        }
        
        for _ in 0..4 {
            if let Some(ppu_interrupt) = self.update_ppu_cycle() {
                interrupt = Some(ppu_interrupt);
                self.request_interrupt(ppu_interrupt);
            }
        }
        
        for _ in 0..4 {
            if self.update_serial_cycle() {
                self.request_interrupt(InterruptType::Serial);
            }
        }
        
        for _ in 0..4 {
            if self.update_joypad_cycle() {
                self.request_interrupt(InterruptType::Joypad);
            }
        }
        
        // Advance the clock
        self.clock.tick(1);
        
        interrupt
    }

    pub fn read_byte(&mut self, addr: u16) -> u8 {
        // During DMA, CPU can only access HRAM
        if self.memory_access_type == MemoryAccessType::DMA && !(addr >= 0xFF80 && addr <= 0xFFFE) {
            // Return 0xFF for inaccessible memory during DMA
            return 0xFF;
        }
        // Start memory access timing
        self.memory_access_in_progress = true;
        self.memory_access_type = MemoryAccessType::Read;
        self.memory_access_address = addr;
        
        // Set access cycles based on memory region
        self.memory_access_cycles_remaining = match addr {
            0x0000..=0x7FFF => 1, // ROM: 1 m-cycle
            0x8000..=0x9FFF => 1, // VRAM: 1 m-cycle
            0xA000..=0xBFFF => 1, // External RAM: 1 m-cycle
            0xC000..=0xDFFF => 1, // WRAM: 1 m-cycle
            0xE000..=0xFDFF => 1, // Echo RAM: 1 m-cycle
            0xFE00..=0xFE9F => 1, // OAM: 1 m-cycle
            0xFF00..=0xFF7F => 1, // I/O: 1 m-cycle
            0xFF80..=0xFFFE => 1, // HRAM: 1 m-cycle
            0xFFFF => 1,          // IE: 1 m-cycle
            _ => 1,
        };

        match addr {
            // ROM bank 0 (0x0000-0x3FFF)
            0x0000..=0x3FFF => {
                if addr as usize >= self.rom.len() {
                    0xFF
                } else {
                    self.rom[addr as usize]
                }
            },
            // ROM bank 1-N (0x4000-0x7FFF)
            0x4000..=0x7FFF => {
                // The correct calculation depends on your MBC implementation
                // For simple cases with no banking, it would be:
                let rom_addr = addr as usize;
                if rom_addr >= self.rom.len() {
                    0xFF
                } else {
                    self.rom[rom_addr]
                }
                // For MBC implementations, you'd calculate the correct bank
            },
            // VRAM (0x8000-0x9FFF)
            0x8000..=0x9FFF => self.ppu.read_vram(addr),
            // External RAM (0xC000-0xDFFF)
            0xA000..=0xBFFF => {
                let addr = (addr - 0xA000) as u16;
                if (addr as u16) < self.eram.len() as u16 {
                    self.eram[addr as usize]
                } else {
                    0xFF
                }
            },
            // Working RAM (0xC000-0xDFFF)
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize],
            
            // Echo RAM (0xE000-0xFDFF)
            0xE000..=0xFDFF => self.wram[(addr - 0xE000) as usize],

            // OAM (0xFE00-0xFE9F)
            0xFE00..=0xFE9F => self.ppu.read_oam(addr),
            
            // I/O Registers (0xFF00-0xFF7F)
            0xFF00..=0xFF7F => self.read_io(addr),
            
            // High RAM (0xFF80-0xFFFE)
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize],
            
            // Interrupt Enable
            0xFFFF => self.get_ie(),
            
            // Unused memory regions
            _ => 0xFF,
        }
    }

    pub fn write_byte(&mut self, addr: u16, value: u8) {
        // During DMA, CPU can only access HRAM
        if self.memory_access_type == MemoryAccessType::DMA && !(addr >= 0xFF80 && addr <= 0xFFFE) {
            // Return 0xFF for inaccessible memory during DMA
            return;
        }
        // Start memory access timing
        self.memory_access_in_progress = true;
        self.memory_access_type = MemoryAccessType::Write;
        self.memory_access_address = addr;
        self.memory_access_value = Some(value);
        
        // Set access cycles based on memory region
        self.memory_access_cycles_remaining = match addr {
            0x0000..=0x7FFF => 1, // ROM: 1 m-cycle
            0x8000..=0x9FFF => 1, // VRAM: 1 m-cycle
            0xA000..=0xBFFF => 1, // External RAM: 1 m-cycle
            0xC000..=0xDFFF => 1, // WRAM: 1 m-cycle
            0xE000..=0xFDFF => 1, // Echo RAM: 1 m-cycle
            0xFE00..=0xFE9F => 1, // OAM: 1 m-cycle
            0xFF00..=0xFF7F => 1, // I/O: 1 m-cycle
            0xFF80..=0xFFFE => 1, // HRAM: 1 m-cycle
            0xFFFF => 1,          // IE: 1 m-cycle
            _ => 1,
        };

        match addr {
            // VRAM (0x8000-0x9FFF)
            0x8000..=0x9FFF => self.ppu.write_vram(addr, value),

            // External RAM
            0xA000..=0xBFFF => {
                let addr = (addr - 0xA000) as u16;
                if (addr as u16) < self.eram.len() as u16 {
                    self.eram[addr as usize] = value;
                }
            },
            
            // Working RAM
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize] = value,
            
            // Echo RAM
            0xE000..=0xFDFF => self.wram[(addr - 0xE000) as usize] = value,

            // OAM (0xFE00-0xFE9F)
            0xFE00..=0xFE9F => self.ppu.write_oam(addr, value),
            
            // I/O Registers
            0xFF00..=0xFF7F => self.write_io(addr, value),
            
            // High RAM
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize] = value,
            
            // Interrupt Enable
            0xFFFF => self.set_ie(value),
            
            // Unused memory regions
            _ => {},
        }
    }

    fn read_io(&self, addr: u16) -> u8 {
        match addr {
            // Joypad
            0xFF00 => {
                if self.joypad_select & 0x20 == 0 {
                    // If action buttons are selected (P15 = 0)
                    0xC0 | (self.joypad_select & 0x30) | self.joypad_buttons
                } else if self.joypad_select & 0x10 == 0 {
                    // If direction buttons are selected (P14 = 0)
                    0xC0 | (self.joypad_select & 0x30) | self.joypad_dpad
                } else {
                    0xCF
                }
            },
            // Serial Transfer Data
            0xFF01 => self.serial_data,
            
            // Serial Transfer Control
            0xFF02 => self.serial_control,

            // Timer registers
            0xFF04 => self.timer.get_div(),
            0xFF05 => self.timer.get_tima(),
            0xFF06 => self.timer.get_tma(),
            0xFF07 => self.timer.get_tac(),

            // Audio
            0xFF24 => 0x77, // Sound control register
            0xFF25 => 0xF3, // Sound output terminal selection
            0xFF26 => 0xF1, // Sound on/off
            
            // Interrupt Flag (0xFF0F)
            0xFF0F => self.get_if(),

            // PPU registers
            0xFF40..=0xFF4B => self.ppu.read_register(addr),
            
            // Other I/O registers
            _ => self.io_registers[(addr - 0xFF00) as usize],
        }
    }

    fn write_io(&mut self, addr: u16, value: u8) {
        match addr {
            // Joypad
            0xFF00 => {
                // Only bits 4-5 are writable (selection bits)
                self.joypad_select = 0xC0 | (value & 0x30) | (self.joypad_select & 0xF); // bit 7 and 6 unused and always 1
            },
            // Serial Transfer Data
            0xFF01 => {
                self.serial_data = value;
            },
            
            // Serial Transfer Control
            0xFF02 => {
                self.serial_control = value & 0x83; // Only bits 0, 1, and 7 are writable
                
                // Check if transfer start requested (bit 7 changed from 0 to 1)
                if self.serial_control & 0x80 != 0 {
                    // Start a new transfer
                    self.serial_transfer_active = true;
                    self.serial_bit_counter = 0;
                    self.serial_clock_counter = 0;
                }
            },

            // Timer registers
            0xFF04 => self.timer.set_div(value),
            0xFF05 => self.timer.set_tima(value),
            0xFF06 => self.timer.set_tma(value),
            0xFF07 => self.timer.set_tac(value),
            
            // Interrupt Flag (0xFF0F)
            0xFF0F => self.set_if(value), // Only bits 0-4 are used

            // PPU registers
            0xFF40..=0xFF4B => {
                if addr == 0xFF46 {
                    self.begin_oam_dma(value);
                } else {
                    self.ppu.write_register(addr, value);
                }
            },
            
            // Other I/O registers
            _ => self.io_registers[(addr - 0xFF00) as usize] = value,
        }
    }

    pub fn is_memory_access_in_progress(&self) -> bool {
        self.memory_access_in_progress
    }

    // Methods for interrupt handling
    pub fn request_interrupt(&mut self, interrupt: InterruptType) {
        self.int_ctrl.request_interrupt(&mut self.io_registers[0x0F], interrupt);
    }

    pub fn clear_interrupt(&mut self, interrupt: InterruptType) {
        self.int_ctrl.clear_interrupt(&mut self.io_registers[0x0F], interrupt);
    }

    /*
    The key insight is that on the original Game Boy hardware,
    the unused bits (5-7) of the IE register at address 0xFFFF always read as "1".
    */

    pub fn set_if(&mut self, value: u8) {
        self.io_registers[0x0F] = (value & 0x1F) | 0xE0; // Only bits 0-4 are writable, bits 5-7 always 1
    }

    pub fn set_ie(&mut self, value: u8) {
        self.ie_register = (value & 0x1F) | 0xE0; // Only bits 0-4 are writable, bits 5-7 always 1
    }

    pub fn get_ie(&self) -> u8 {
        self.ie_register | 0xE0  // Ensure bits 5-7 always read as 1
    }
    
    pub fn get_if(&self) -> u8 {
        self.io_registers[0x0F]
    }

    pub fn handle_key_event(&mut self, key: Keycode, pressed: bool) {
        // Skip rapid repeat inputs via debouncing for press events (not release)
        if pressed && self.joypad_debounce_counter > 0 {
            return;
        }
        
        match key {
            // D-pad
            Keycode::Right => {
                if pressed {
                    self.press_button(JoypadButton::Right);
                    self.joypad_debounce_counter = self.joypad_debounce_delay;
                } else {
                    self.release_button(JoypadButton::Right);
                }
            },
            Keycode::Left => {
                if pressed {
                    self.press_button(JoypadButton::Left);
                    self.joypad_debounce_counter = self.joypad_debounce_delay;
                } else {
                    self.release_button(JoypadButton::Left);
                }
            },
            Keycode::Up => {
                if pressed {
                    self.press_button(JoypadButton::Up);
                    self.joypad_debounce_counter = self.joypad_debounce_delay;
                } else {
                    self.release_button(JoypadButton::Up);
                }
            },
            Keycode::Down => {
                if pressed {
                    self.press_button(JoypadButton::Down);
                    self.joypad_debounce_counter = self.joypad_debounce_delay;
                } else {
                    self.release_button(JoypadButton::Down);
                }
            },
            
            // Buttons - Z for A, X for B, Space for Select, Return for Start
            Keycode::Z => {
                if pressed {
                    self.press_button(JoypadButton::A);
                    self.joypad_debounce_counter = self.joypad_debounce_delay;
                } else {
                    self.release_button(JoypadButton::A);
                }
            },
            Keycode::X => {
                if pressed {
                    self.press_button(JoypadButton::B);
                    self.joypad_debounce_counter = self.joypad_debounce_delay;
                } else {
                    self.release_button(JoypadButton::B);
                }
            },
            Keycode::Space => {
                if pressed {
                    self.press_button(JoypadButton::Select);
                    self.joypad_debounce_counter = self.joypad_debounce_delay;
                } else {
                    self.release_button(JoypadButton::Select);
                }
            },
            Keycode::Return => {
                if pressed {
                    self.press_button(JoypadButton::Start);
                    self.joypad_debounce_counter = self.joypad_debounce_delay;
                } else {
                    self.release_button(JoypadButton::Start);
                }
            },
            
            _ => {} // Ignore other keys
        }
    }

    // Press a button (set bit to 0)
    fn press_button(&mut self, button: JoypadButton) {
        let old_buttons = (self.joypad_buttons & 0x0F) | (self.joypad_dpad & 0x0F);
        
        match button {
            // D-pad
            JoypadButton::Right => self.joypad_dpad &= !0x01,
            JoypadButton::Left => self.joypad_dpad &= !0x02,
            JoypadButton::Up => self.joypad_dpad &= !0x04,
            JoypadButton::Down => self.joypad_dpad &= !0x08,
            
            // Buttons
            JoypadButton::A => self.joypad_buttons &= !0x01,
            JoypadButton::B => self.joypad_buttons &= !0x02,
            JoypadButton::Select => self.joypad_buttons &= !0x04,
            JoypadButton::Start => self.joypad_buttons &= !0x08,
        }
        
        let new_buttons = (self.joypad_buttons & 0x0F) | (self.joypad_dpad & 0x0F);
        
        // Only request interrupt if a button is newly pressed
        // (changed from released to pressed)
        if (old_buttons & new_buttons) != old_buttons {
            // Request joypad interrupt
            self.request_interrupt(InterruptType::Joypad);
        }
        
        // Store the current state for debouncing
        self.last_joypad_state = new_buttons;
    }
    
    // Release a button (set bit to 1)
    fn release_button(&mut self, button: JoypadButton) {
        match button {
            // D-pad
            JoypadButton::Right => self.joypad_dpad |= 0x01,
            JoypadButton::Left => self.joypad_dpad |= 0x02,
            JoypadButton::Up => self.joypad_dpad |= 0x04,
            JoypadButton::Down => self.joypad_dpad |= 0x08,
            
            // Buttons
            JoypadButton::A => self.joypad_buttons |= 0x01,
            JoypadButton::B => self.joypad_buttons |= 0x02,
            JoypadButton::Select => self.joypad_buttons |= 0x04,
            JoypadButton::Start => self.joypad_buttons |= 0x08,
        }
    }
}