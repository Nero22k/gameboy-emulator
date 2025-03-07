/// Clock module
/// 
/// This module provides a clock to track the number of M-cycles (machine cycles)
/// that have occurred since the start of the program.
/// 
/// M-cycles called machine cycles are used to determine when events should occur.

pub struct Clock {
    pub m_cycles: u64,
}

impl Clock {
    pub fn new() -> Self {
        Self { m_cycles: 0, }
    }

    pub fn tick(&mut self, m_cycles: u8) {
        self.m_cycles += m_cycles as u64;
    }

    pub fn m_to_t_cycles(m_cycles: u8) -> u8 {
        m_cycles * 4
    }

    pub fn t_to_m_cycles(t_cycles: u8) -> u8 {
        t_cycles / 4
    }
}