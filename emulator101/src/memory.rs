use crate::interrupts::{InterruptController, InterruptType};
use crate::timer::Timer;
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
    serial_data: u8,
    serial_control: u8,
    pub serial_output: String,
}

// Lifetime 'a is used to ensure that the ROM data reference is valid for the lifetime of the MemoryBus instance.
// This is necessary because the ROM data is stored in the cartridge and is not owned by the MemoryBus.
impl<'a> MemoryBus<'a> {
    pub fn new(rom: &'a [u8]) -> Self {
        Self {
            wram: [0; 0x2000],
            hram: [0; 0x7F],
            io_registers: [0; 0x80],
            ie_register: 0,
            rom,
            eram: vec![0; 0x2000], // 8KB external RAM
            int_ctrl: InterruptController::new(),
            timer: Timer::new(),
            ppu: Ppu::new(),
            joypad_select: 0x30, // Both button and direction selected (P14 and P15 high)
            joypad_buttons: 0x0F, // All buttons released
            joypad_dpad: 0x0F,    // All d-pad released
            last_joypad_state: 0xFF,
            joypad_debounce_counter: 0,
            joypad_debounce_delay: 2,
            serial_data: 0,
            serial_control: 0x7E,
            serial_output: String::new(),
        }
    }

    pub fn update_joypad(&mut self) {
        // Implement debouncing by only processing inputs
        // after a certain number of frames
        if self.joypad_debounce_counter > 0 {
            self.joypad_debounce_counter -= 1;
        }
    }

    // Update timer with the number of cycles that have passed
    pub fn update_timer(&mut self, cycles: u8) {
        if self.timer.update(cycles) {
            // Request timer interrupt if timer overflowed
            self.request_interrupt(InterruptType::Timer);
        }
    }

    // Update PPU with the number of cycles that have passed
    pub fn update_ppu(&mut self, cycles: u8) {
        // First, update the PPU state and get any triggered interrupts
        if let Some(interrupt) = self.ppu.update(cycles) {
            // Request the appropriate interrupt
            self.request_interrupt(interrupt);
        }
        
        // Process DMA transfers - one byte per CPU cycle
        for _ in 0..cycles {
            // Get DMA source without borrowing self.ppu mutably
            let dma_source = self.ppu.get_dma_source();
            
            // If DMA is active (source != 0)
            if dma_source != 0 {
                // Get current byte position
                let byte_pos = self.ppu.get_dma_byte();
                
                // Calculate actual memory address
                let addr = dma_source + (byte_pos as u16);
                
                // Read the byte from memory
                let value = self.read_byte(addr);
                
                // Process the DMA byte (write to OAM)
                self.ppu.process_dma_byte(value);
            }
        }
    }

    pub fn read_byte(&self, addr: u16) -> u8 {
        match addr {
            // ROM (0x0000-0x7FFF)
            0x0000..=0x7FFF => {
                if addr as u16 >= self.rom.len() as u16 {
                    0xFF
                } else {
                    self.rom[addr as usize]
                }
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

    /*
    Had silly issue where having serial registers would break my emulator with games like Tetris and Tennis.
    */

    fn read_io(&self, addr: u16) -> u8 {
        match addr {
            // Joypad
            0xFF00 => {
                // Default state: All inputs inactive (1), both button groups selected (0)
                let mut result = self.joypad_select;
    
                // If action buttons are selected (P15 = 0)
                if self.joypad_select & 0x20 == 0 {
                    result &= 0xF0 | self.joypad_buttons;
                }
                
                // If direction buttons are selected (P14 = 0)
                if self.joypad_select & 0x10 == 0 {
                    result &= 0xF0 | self.joypad_dpad;
                }
                
                result
            },
            // Serial Transfer Data
            //0xFF01 => self.serial_data,
            
            // Serial Transfer Control
            //0xFF02 => self.serial_control,

            // Timer registers
            0xFF04 => self.timer.get_div(),
            0xFF05 => self.timer.get_tima(),
            0xFF06 => self.timer.get_tma(),
            0xFF07 => self.timer.get_tac(),
            
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
                self.joypad_select = (value & 0x30) | 0x0F;
            },
            // Serial Transfer Data
            /*0xFF01 => {
                self.serial_data = value;
            },
            
            // Serial Transfer Control
            0xFF02 => {
                self.serial_control = value;
                
                // Handle serial transfer (for Blargg's test ROMs)
                // If bit 7 is set, the serial transfer is in progress
                if value & 0x80 != 0 {
                    let data = self.serial_data;
                    
                    // Only add printable characters to the output string
                    if data >= 0x20 && data <= 0x7E {
                        let c = data as char;
                        self.serial_output.push(c);
                    } else if data == 0x0A { // Handle newline
                        self.serial_output.push('\n');
                    }
                    
                    // Auto-acknowledge the transfer by clearing bit 7
                    self.serial_control &= 0x7F;
                    
                    // Request Serial interrupt
                    self.request_interrupt(InterruptType::Serial);
                }
            },*/

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
                    self.ppu.begin_oam_dma(value);
                } else {
                    self.ppu.write_register(addr, value);
                }
            },
            
            // Other I/O registers
            _ => self.io_registers[(addr - 0xFF00) as usize] = value,
        }
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
    
    pub fn get_if(&self) -> u8 {
        self.io_registers[0x0F] | 0xE0 // Always read bits 5-7 as 1
    }
    
    pub fn set_if(&mut self, value: u8) {
        self.io_registers[0x0F] = (value & 0x1F) | 0xE0; // Only bits 0-4 are writable, bits 5-7 always 1
    }
    
    pub fn get_ie(&self) -> u8 {
        self.ie_register | 0xE0 // Always read bits 5-7 as 1
    }

    pub fn set_ie(&mut self, value: u8) {
        self.ie_register = (value & 0x1F) | 0xE0; // Only bits 0-4 are writable, bits 5-7 always 1
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