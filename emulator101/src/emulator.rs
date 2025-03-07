use crate::cpu::Cpu;
use crate::memory::MemoryBus;

pub struct Emulator<'a> {
    pub cpu: Cpu,
    pub bus: MemoryBus<'a>,
}

impl<'a> Emulator<'a> {
    pub fn new(rom: &'a [u8]) -> Self {
        let mut cpu = Cpu::new();
        cpu.reset();
        
        Self {
            cpu,
            bus: MemoryBus::new(rom),
        }
    }
    
    fn step(&mut self) -> u8 {
        // If memory access is in progress, just tick the bus
        if self.bus.is_memory_access_in_progress() {
            self.bus.tick();
            return 1;
        }
        
        // Handle interrupts
        if self.cpu.handle_interrupts(&mut self.bus) {
            // Interrupts take 5 m-cycles
            for _ in 0..5 {
                self.bus.tick();
            }
            return 5;
        }
        
        // Execute an instruction
        let m_cycles = self.cpu.tick(&mut self.bus);
        
        // Update components for each m-cycle
        for _ in 0..m_cycles {
            self.bus.tick();
        }
        
        m_cycles
    }
    
    pub fn run_until_frame(&mut self) -> u32 {
        let mut cycles_this_frame = 0;
        
        while !self.bus.ppu.is_frame_ready() {
            let m_cycles = self.step();
            cycles_this_frame += m_cycles as u32;
        }
        
        cycles_this_frame
    }
}
