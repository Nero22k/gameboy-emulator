use crate::memory::MemoryBus;

#[derive(Debug, Clone, Copy)]
pub enum InterruptType {
    VBlank = 0,  // Bit 0 of IF/IE
    LcdStat = 1, // Bit 1
    Timer = 2,   // Bit 2
    Serial = 3,  // Bit 3
    Joypad = 4,  // Bit 4
}

pub struct InterruptController;

impl InterruptController {
    pub fn new() -> Self {
        InterruptController
    }
    
    /// Requests an interrupt by setting the appropriate bit in the interrupt flag register (`if_reg`).
    pub fn request_interrupt(&self, if_reg: &mut u8, interrupt: InterruptType) {
        *if_reg |= 1 << interrupt as u8;
    }
    
    /// Clears an interrupt by resetting the appropriate bit in the interrupt flag register (`if_reg`).
    pub fn clear_interrupt(&self, if_reg: &mut u8, interrupt: InterruptType) {
        *if_reg &= !(1 << interrupt as u8);
    }
    
    /// Checks if there are any pending interrupts (enabled and requested).
    pub fn has_pending_interrupts(memory: &MemoryBus) -> bool {
        let ie = memory.get_ie();
        let if_reg = memory.get_if();
        (ie & if_reg & 0x1F) != 0
    }
    
    // Get the highest priority interrupt that is enabled and requested by the IF and IE registers
    pub fn get_highest_priority_interrupt(memory: &MemoryBus) -> Option<InterruptType> {
        let ie = memory.get_ie();
        let if_reg = memory.get_if();
        let pending = ie & if_reg & 0x1F;
        
        if pending == 0x0 {
            return None;
        }
        
        // Check in priority order (VBlank is highest)
        if pending & 0x01 != 0x0 {
            Some(InterruptType::VBlank)
        } else if pending & 0x02 != 0 {
            Some(InterruptType::LcdStat)
        } else if pending & 0x04 != 0 {
            Some(InterruptType::Timer)
        } else if pending & 0x08 != 0 {
            Some(InterruptType::Serial)
        } else {
            Some(InterruptType::Joypad)
        }
    }
    
    // Get the interrupt vector address for the given interrupt type by multiplying the interrupt type by 0x08 and adding 0x40
    pub fn get_interrupt_vector(interrupt: InterruptType) -> u16 {
        0x0040 + ((interrupt as u16) * 0x08)
    }
}