pub struct Timer {
    // The internal 16-bit DIV counter
    div_counter: u16,
    
    // Timer registers
    tima: u8,   // Timer counter (0xFF05)
    tma: u8,    // Timer modulo (0xFF06)
    tac: u8,    // Timer control (0xFF07)
    
    // State for edge detection
    previous_and_result: bool,
    
    // State for TIMA overflow handling
    tima_overflow: bool,
    tima_overflow_cycles: u8,
    // Queued write during overflow state
    queued_tima_write: Option<u8>,
}

impl Timer {
    pub fn new() -> Self {
        Self {
            div_counter: 0xABCC,
            tima: 0,
            tma: 0,
            tac: 0xF8,
            previous_and_result: false,
            tima_overflow: false,
            tima_overflow_cycles: 0,
            queued_tima_write: None,
        }
    }

    pub fn update_cycle(&mut self) -> bool {
        let mut interrupt_requested = false;
        
        // Increment the 16-bit DIV counter
        self.div_counter = self.div_counter.wrapping_add(1);
        
        // Get the bit position to check based on TAC clock select
        let bit_position: u8 = match self.tac & 0x03 {
            0 => 9, // 4096HZ (check bit 9)
            1 => 3, // 262144HZ (check bit 3)
            2 => 5, // 65536HZ (check bit 5)
            3 => 7, // 16384HZ (check bit 7)
            _ => unreachable!(),
        };
        
        // Extract the bit from DIV counter at the specified position
        let bit_value = (self.div_counter & (1 << bit_position)) != 0;
        
        // Check if timer is enabled
        let timer_enabled = (self.tac & 0x04) != 0;
        
        // Calculate current AND result
        let current_and_result = bit_value && timer_enabled;
        
        // Check for falling edge (1->0)
        if self.previous_and_result && !current_and_result {
            // Increment TIMA on falling edge
            if !self.tima_overflow {
                let current_tima = self.tima;
                self.tima = self.tima.wrapping_add(1);
                
                // Check for overflow (when TIMA goes from 0xFF to 0x00)
                if current_tima == 0xFF {
                    self.tima_overflow = true;
                    self.tima_overflow_cycles = 0;
                    // Keep TIMA at zero during the delay period
                    self.tima = 0;
                }
            }
        }
        
        // Update the previous AND result for next cycle
        self.previous_and_result = current_and_result;
        
        // Handle TIMA overflow (if active)
        if self.tima_overflow {
            self.tima_overflow_cycles += 1;
            
            // TIMA reads as 0 during overflow
            self.tima = 0;
            
            if self.tima_overflow_cycles == 4 {
                // First reload from TMA
                self.tima = self.tma;
                
                // THEN apply any queued write
                if let Some(value) = self.queued_tima_write {
                    self.tima = value;
                    self.queued_tima_write = None;
                }
                
                // Reset overflow state and trigger interrupt
                self.tima_overflow = false;
                self.tima_overflow_cycles = 0;
                interrupt_requested = true;
            }
        }
        
        interrupt_requested
    }
    
    // Getters and setters for timer registers
    
    pub fn get_div(&self) -> u8 {
        // DIV register is the upper 8 bits of the 16-bit counter
        (self.div_counter >> 8) as u8
    }
    
    pub fn set_div(&mut self, _value: u8) {
        // Save the old DIV value to check for falling edge
        let old_div_counter = self.div_counter;
        
        // Writing to DIV resets the entire 16-bit counter to 0
        self.div_counter = 0;
        
        // This can trigger a TIMA increment if it causes a falling edge!
        let bit_position: u8 = match self.tac & 0x03 {
            0 => 9, // 4096HZ (check bit 9)
            1 => 3, // 262144HZ (check bit 3)
            2 => 5, // 65536HZ (check bit 5)
            3 => 7, // 16384HZ (check bit 7)
            _ => unreachable!(),
        };
        
        // Check if the relevant bit was high in the old counter
        let old_bit_value = (old_div_counter & (1 << bit_position)) != 0;
        let timer_enabled = (self.tac & 0x04) != 0;
        let old_and_result = old_bit_value && timer_enabled;
        
        // After reset, all bits of DIV are 0
        let new_bit_value = false;
        let new_and_result = new_bit_value && timer_enabled;
        
        // Check for falling edge caused by DIV reset
        if old_and_result && !new_and_result {
            // Increment TIMA if it's not already in overflow state
            if !self.tima_overflow {
                let (new_tima, overflow) = self.tima.overflowing_add(1);
                self.tima = new_tima;
                
                if overflow {
                    self.tima_overflow = true;
                    self.tima_overflow_cycles = 0;
                }
            }
        }
        
        self.previous_and_result = new_and_result;
    }
    
    pub fn get_tima(&self) -> u8 {
        self.tima
    }
    
    pub fn set_tima(&mut self, value: u8) {
        // If TIMA is in overflow state, queue the write for after the overflow completes
        if self.tima_overflow {
            self.queued_tima_write = Some(value);
        } else {
            self.tima = value;
        }
    }
    
    pub fn get_tma(&self) -> u8 {
        self.tma
    }
    
    pub fn set_tma(&mut self, value: u8) {
        self.tma = value;
    }
    
    pub fn get_tac(&self) -> u8 {
        self.tac
    }
    
    pub fn set_tac(&mut self, value: u8) {
        // Only bits 0-2 are used
        let old_tac = self.tac;
        self.tac = value & 0x07;
        
        // Changing TAC can trigger a TIMA increment if it causes a falling edge!
        if old_tac != self.tac {
            let bit_position: u8 = match self.tac & 0x03 {
                0 => 9, // 4096HZ (check bit 9)
                1 => 3, // 262144HZ (check bit 3)
                2 => 5, // 65536HZ (check bit 5)
                3 => 7, // 16384HZ (check bit 7)
                _ => unreachable!(),
            };
            
            let bit_value = (self.div_counter & (1 << bit_position)) != 0;
            let timer_enabled = (self.tac & 0x04) != 0;
            let current_and_result = bit_value && timer_enabled;
            
            // Check for falling edge caused by TAC change
            if self.previous_and_result && !current_and_result {
                // Increment TIMA if it's not already in overflow state
                if !self.tima_overflow {
                    let (new_tima, overflow) = self.tima.overflowing_add(1);
                    self.tima = new_tima;
                    
                    if overflow {
                        self.tima_overflow = true;
                        self.tima_overflow_cycles = 0;
                    }
                }
            }
            
            self.previous_and_result = current_and_result;
        }
    }
}