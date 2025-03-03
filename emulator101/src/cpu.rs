use crate::memory::MemoryBus;
use crate::interrupts::{InterruptController, InterruptType};

struct Flags {
    z: bool, // Zero flag
    n: bool, // Subtract flag
    h: bool, // Half-carry flag
    c: bool, // Carry flag
}

pub enum CpuFlag
{
    C = 0b00010000, // Carry flag (bit 4)
    H = 0b00100000, // Half-carry flag (bit 5)
    N = 0b01000000, // Subtract flag (bit 6)
    Z = 0b10000000, // Zero flag (bit 7)
}

impl Flags {
    fn new() -> Self {
        Self {
            z: false,
            n: false,
            h: false,
            c: false,
        }
    }

    fn to_byte(&self) -> u8 {
        let mut result: u8 = 0;
        if self.c { result |= CpuFlag::C as u8; }
        if self.h { result |= CpuFlag::H as u8; }
        if self.n { result |= CpuFlag::N as u8; }
        if self.z { result |= CpuFlag::Z as u8; }
        result
    }

    // Set from u8 value
    fn from_byte(&mut self, byte: u8) { 
        self.c = (byte & CpuFlag::C as u8) != 0;
        self.h = (byte & CpuFlag::H as u8) != 0;
        self.n = (byte & CpuFlag::N as u8) != 0;
        self.z = (byte & CpuFlag::Z as u8) != 0;
    }
}

pub struct Cpu {
    // Registers
    af: u16, // Accumulator and Flags
    bc: u16, // BC register pair
    de: u16, // DE register pair
    hl: u16, // HL register pair
    // Flags
    f: Flags,
    sp: u16, // Stack pointer
    pc: u16, // Program counter

    // CPU state
    halted: bool,
    ime: bool,     // interrupt master enable
    pending_ime: bool, // for EI's 1-instruction delay
    halt_bug: bool,    // for HALT bug tracking
    
    // Cycle counting
    pub cycle_count: u64,
}

impl Cpu {
    pub fn new() -> Self {
        // Post-boot ROM state
        Self {
            af: 0,
            bc: 0,
            de: 0,
            hl: 0,
            f: Flags::new(),
            sp: 0,
            pc: 0,
            halted: false,
            ime: false,
            pending_ime: false,
            halt_bug: false,
            cycle_count: 0,
        }
    }

    // Reset the CPU state
    pub fn reset(&mut self) {
        self.af = 0x01B0;
        self.bc = 0x0013;
        self.de = 0x00D8;
        self.hl = 0x014D;
        self.f = Flags {
            z: true,
            n: false,
            h: true,
            c: true,
        };
        self.sp = 0xFFFE;
        self.pc = 0x0100;
        self.halted = false;
        self.ime = false;
        self.pending_ime = false;
        self.halt_bug = false;
        self.cycle_count = 0;
    }

    // Get register BC as 16-bit
    fn get_bc(&self) -> u16 {
        self.bc
    }
    // Set register BC from 16-bit value
    fn set_bc(&mut self, value: u16) {
        self.bc = value;
    }
    // Get register DE as 16-bit
    fn get_de(&self) -> u16 {
        self.de
    }
    // Set register DE from 16-bit value
    fn set_de(&mut self, value: u16) {
        self.de = value;
    }
    // Get register HL as 16-bit
    fn get_hl(&self) -> u16 {
        self.hl
    }
    // Set register HL from 16-bit value
    fn set_hl(&mut self, value: u16) {
        self.hl = value;
    }
    // Get register AF as 16-bit
    fn get_af(&self) -> u16 {
        self.af
    }
    // Set register AF from 16-bit value
    fn set_af(&mut self, value: u16) {
        // Extract F register value (lower 8 bits) and ensure lower 4 bits are always 0
        let f = (value & 0x00FF) as u8 & 0xF0;
        
        // Update the flags struct with the new value
        self.f.from_byte(f);
        
        // Update the full AF register
        self.af = value & 0xFFF0; // Ensure lower 4 bits are always 0
    }
    // Get register A as 8-bit
    fn get_a(&self) -> u8 {
        (self.af >> 8) as u8
    }
    // Set register A from 8-bit value
    fn set_a(&mut self, value: u8) {
        self.af = (self.af & 0x00FF) | ((value as u16) << 8);
    }
    // Set a flag in the F register
    fn flag(&mut self, flags: CpuFlag, set: bool) {
        let mask = flags as u8;
        let mut f_value = self.f.to_byte();
        
        if set {
            f_value |= mask;
        } else {
            f_value &= !mask;
        }
        
        // Update the Flags struct
        self.f.from_byte(f_value);
        
        // Update the F register in the af register pair
        self.af = (self.af & 0xFF00) | (f_value as u16);
    }
    // Get register B as 8-bit
    fn get_b(&self) -> u8 {
        (self.bc >> 8) as u8
    }
    // Set register B from 8-bit value
    fn set_b(&mut self, value: u8) {
        self.bc = (self.bc & 0x00FF) | ((value as u16) << 8);
    }
    // Get register C as 8-bit
    fn get_c(&self) -> u8 {
        self.bc as u8
    }
    // Set register C from 8-bit value
    fn set_c(&mut self, value: u8) {
        self.bc = (self.bc & 0xFF00) | value as u16;
    }
    // Get register D as 8-bit
    fn get_d(&self) -> u8 {
        (self.de >> 8) as u8
    }
    // Set register D from 8-bit value
    fn set_d(&mut self, value: u8) {
        self.de = (self.de & 0x00FF) | ((value as u16) << 8);
    }
    // Get register E as 8-bit
    fn get_e(&self) -> u8 {
        self.de as u8
    }
    // Set register E from 8-bit value
    fn set_e(&mut self, value: u8) {
        self.de = (self.de & 0xFF00) | value as u16;
    }
    // Get register H as 8-bit
    fn get_h(&self) -> u8 {
        (self.hl >> 8) as u8
    }
    // Set register H from 8-bit value
    fn set_h(&mut self, value: u8) {
        self.hl = (self.hl & 0x00FF) | ((value as u16) << 8);
    }
    // Get register L as 8-bit
    fn get_l(&self) -> u8 {
        self.hl as u8
    }
    // Set register L from 8-bit value
    fn set_l(&mut self, value: u8) {
        self.hl = (self.hl & 0xFF00) | value as u16;
    }
    
    // Fetch the next byte from memory and increment PC
    fn fetch_byte<'a>(&mut self, memory: &'a MemoryBus) -> u8 {
        let byte = memory.read_byte(self.pc);
        self.pc = self.pc.wrapping_add(1);
        byte
    }
    
    // Fetch the next 16-bit word from memory and increment PC
    fn fetch_word<'a>(&mut self, memory: &'a MemoryBus) -> u16 {
        let lo = self.fetch_byte(memory) as u16;
        let hi = self.fetch_byte(memory) as u16;
        (hi << 8) | lo
    }

    // Write word to memory
    fn write_word<'a>(&mut self, memory: &mut MemoryBus<'a>, addr: u16, value: u16) {
        memory.write_byte(addr, (value & 0xFF) as u8);
        memory.write_byte(addr + 1, (value >> 8) as u8);
    }
    
    // Push a 16-bit value onto the stack
    fn push_word<'a>(&mut self, memory: &mut MemoryBus<'a>, value: u16) {
        self.sp = self.sp.wrapping_sub(1);
        memory.write_byte(self.sp, (value >> 8) as u8);
        self.sp = self.sp.wrapping_sub(1);
        memory.write_byte(self.sp, value as u8);
    }
    
    // Pop a 16-bit value from the stack
    fn pop_word<'a>(&mut self, memory: &'a MemoryBus) -> u16 {
        let lo = memory.read_byte(self.sp) as u16;
        self.sp = self.sp.wrapping_add(1);
        let hi = memory.read_byte(self.sp) as u16;
        self.sp = self.sp.wrapping_add(1);
        (hi << 8) | lo
    }

    #[allow(dead_code)]
    fn debugging(&self, memory: &MemoryBus, opcode: u8) {
        println!("Opcode: {:#04X}", opcode);
        println!("AF: {:#06X}", self.af);
        println!("BC: {:#06X}", self.bc);
        println!("DE: {:#06X}", self.de);
        println!("HL: {:#06X}", self.hl);
        println!("SP: {:#06X}", self.sp);
        println!("PC: {:#06X}", self.pc);
        println!("Z: {}", self.f.z);
        println!("N: {}", self.f.n);
        println!("H: {}", self.f.h);
        println!("C: {}", self.f.c);
        println!("ie: {}", memory.get_ie());
        println!("if: {}", memory.get_if());
        println!("ime: {}", self.ime);
        println!("pending_ime: {}", self.pending_ime);
        println!("halted: {}", self.halted);
    }

    // Execute a single instruction
    pub fn step<'a>(&mut self, memory: &mut MemoryBus<'a>) -> u8 {
        // If halted, check if we should wake up
        if self.halted {
            if InterruptController::has_pending_interrupts(memory) {
                self.halted = false;
            } else {
                // Stay halted for 4 T-cycles
                return 4;
            }
        }
        
        // Handle HALT bug - don't increment PC for the next instruction
        let opcode = self.fetch_byte(memory);

        if self.halt_bug {
            //self.debugging(memory, opcode);
            self.pc = self.pc.wrapping_sub(1);
            self.halt_bug = false;
        }
        
        let cycles = self.execute_instruction(opcode, memory);

        // Debugging
        //self.debugging(memory, opcode);
        
        // Handle EI's delayed effect
        if self.pending_ime {
            self.ime = true;
            self.pending_ime = false;
        }
        
        // Count cycles
        self.cycle_count += cycles as u64;
        cycles
    }

    // Process pending interrupts
    pub fn handle_interrupts<'a>(&mut self, memory: &mut MemoryBus<'a>) -> u8 {
        if !self.ime {
            return 0;
        }
        
        if let Some(interrupt) = InterruptController::get_highest_priority_interrupt(memory) {
            // Disable IME
            self.ime = false;

            let interrupt: InterruptType = interrupt;
            
            // Clear the interrupt flag
            memory.clear_interrupt(interrupt);
            
            // Push PC onto stack
            self.push_word(memory, self.pc);
            
            // Jump to interrupt vector
            self.pc = InterruptController::get_interrupt_vector(interrupt);
            
            // Return the number of cycles (interrupts take 20 T-cycles or 5 M-cycles)
            return 20;
        }
        
        0 // No interrupt handled
    }

    // Execute a single instruction
    fn execute_instruction<'a>(&mut self, opcode: u8, memory: &mut MemoryBus<'a>) -> u8 {
        match opcode {
            0x00 => 4, // NOP
            0x01 => {
                let value = self.fetch_word(memory);
                self.set_bc(value);
                12
            },
            0x02 => {
                let addr = self.get_bc();
                memory.write_byte(addr, self.get_a());
                8
            },
            0x03 => {
                let value = self.get_bc().wrapping_add(1);
                self.set_bc(value);
                8
            },
            0x04 => {
                let result = self.inc_r8(self.get_b());
                self.set_b(result);
                4
            },
            0x05 => {
                let result = self.dec_r8(self.get_b());
                self.set_b(result);
                4
            },
            0x06 => {
                let value = self.fetch_byte(memory);
                self.set_b(value);
                8
            },
            0x07 => {
                let r = self.rlc_r8(self.get_a());
                self.set_a(r);
                self.flag(CpuFlag::Z, false);
                4
            },
            0x08 => {
                let addr = self.fetch_word(memory);
                self.write_word(memory, addr, self.sp);
                20
            },
            0x09 => {
                self.add16(self.get_bc());
                8
            }
            0x0A => {
                let addr = self.get_bc();
                let value = memory.read_byte(addr);
                self.set_a(value);
                8
            },
            0x0B => {
                let value = self.get_bc().wrapping_sub(1);
                self.set_bc(value);
                8
            },
            0x0C => {
                let result = self.inc_r8(self.get_c());
                self.set_c(result);
                4
            },
            0x0D => {
                let result = self.dec_r8(self.get_c());
                self.set_c(result);
                4
            },
            0x0E => {
                let value = self.fetch_byte(memory);
                self.set_c(value);
                8
            },
            0x0F => {
                let r = self.rrc_r8(self.get_a());
                self.set_a(r);
                self.flag(CpuFlag::Z, false);
                4
            },
            0x10 => 4, // STOP
            0x11 => {
                let value = self.fetch_word(memory);
                self.set_de(value);
                12
            },
            0x12 => {
                let addr = self.get_de();
                memory.write_byte(addr, self.get_a());
                8
            },
            0x13 => {
                let value = self.get_de().wrapping_add(1);
                self.set_de(value);
                8
            },
            0x14 => {
                let result = self.inc_r8(self.get_d());
                self.set_d(result);
                4
            },
            0x15 => {
                let result = self.dec_r8(self.get_d());
                self.set_d(result);
                4
            },
            0x16 => {
                let value = self.fetch_byte(memory);
                self.set_d(value);
                8
            },
            0x17 => {
                let r = self.rl_r8(self.get_a());
                self.set_a(r);
                self.flag(CpuFlag::Z, false);
                4
            },
            0x18 => {
                self.cpu_jr(memory, true)
            },
            0x19 => {
                self.add16(self.get_de());
                8
            },
            0x1A => {
                let addr = self.get_de();
                let value = memory.read_byte(addr);
                self.set_a(value);
                8
            },
            0x1B => {
                let value = self.get_de().wrapping_sub(1);
                self.set_de(value);
                8
            },
            0x1C => {
                let result = self.inc_r8(self.get_e());
                self.set_e(result);
                4
            },
            0x1D => {
                let result = self.dec_r8(self.get_e());
                self.set_e(result);
                4
            },
            0x1E => {
                let value = self.fetch_byte(memory);
                self.set_e(value);
                8
            },
            0x1F => {
                let r = self.rr_r8(self.get_a());
                self.set_a(r);
                self.flag(CpuFlag::Z, false);
                4
            },
            0x20 => {
                self.cpu_jr(memory, !self.f.z)
            },
            0x21 => {
                let value = self.fetch_word(memory);
                self.set_hl(value);
                12
            },
            0x22 => {
                let addr = self.get_hl();
                memory.write_byte(addr, self.get_a());
                self.set_hl(addr.wrapping_add(1));
                8
            },
            0x23 => {
                let value = self.get_hl().wrapping_add(1);
                self.set_hl(value);
                8
            },
            0x24 => {
                let result = self.inc_r8(self.get_h());
                self.set_h(result);
                4
            },
            0x25 => {
                let result = self.dec_r8(self.get_h());
                self.set_h(result);
                4
            },
            0x26 => {
                let value = self.fetch_byte(memory);
                self.set_h(value);
                8
            },
            0x27 => {
                self.daa();
                4
            },
            0x28 => {
                self.cpu_jr(memory, self.f.z)
            },
            0x29 => {
                self.add16(self.get_hl());
                8
            },
            0x2A => {
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                self.set_hl(addr.wrapping_add(1));
                self.set_a(value);
                8
            },
            0x2B => {
                let value = self.get_hl().wrapping_sub(1);
                self.set_hl(value);
                8
            },
            0x2C => {
                let result = self.inc_r8(self.get_l());
                self.set_l(result);
                4
            },
            0x2D => {
                let result = self.dec_r8(self.get_l());
                self.set_l(result);
                4
            },
            0x2E => {
                let value = self.fetch_byte(memory);
                self.set_l(value);
                8
            },
            0x2F => {
                let a = self.get_a();
                self.set_a(!a);
                self.flag(CpuFlag::H, true);
                self.flag(CpuFlag::N, true);
                4
            },
            0x30 => {
                self.cpu_jr(memory, !self.f.c)
            },
            0x31 => {
                let value = self.fetch_word(memory);
                self.sp = value;
                12
            },
            0x32 => {
                let addr = self.get_hl();
                memory.write_byte(addr, self.get_a());
                self.set_hl(addr.wrapping_sub(1));
                8
            },
            0x33 => {
                let value = self.sp.wrapping_add(1);
                self.sp = value;
                8
            },
            0x34 => {
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                let result = self.inc_r8(value);
                memory.write_byte(addr, result);
                12
            },
            0x35 => {
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                let result = self.dec_r8(value);
                memory.write_byte(addr, result);
                12
            },
            0x36 => {
                let value = self.fetch_byte(memory);
                let addr = self.get_hl();
                memory.write_byte(addr, value);
                12
            },
            0x37 => {
                self.flag(CpuFlag::C, true);
                self.flag(CpuFlag::H, false);
                self.flag(CpuFlag::N, false);
                4
            },
            0x38 => {
                self.cpu_jr(memory, self.f.c)
            },
            0x39 => {
                self.add16(self.sp);
                8
            },
            0x3A => {
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                self.set_hl(addr.wrapping_sub(1));
                self.set_a(value);
                8
            },
            0x3B => {
                let value = self.sp.wrapping_sub(1);
                self.sp = value;
                8
            },
            0x3C => {
                let result = self.inc_r8(self.get_a());
                self.set_a(result);
                4
            },
            0x3D => {
                let result = self.dec_r8(self.get_a());
                self.set_a(result);
                4
            },
            0x3E => {
                let value = self.fetch_byte(memory);
                self.set_a(value);
                8
            },
            0x3F => {
                self.flag(CpuFlag::C, !self.f.c);
                self.flag(CpuFlag::H, false);
                self.flag(CpuFlag::N, false);
                4
            },
            0x40 => 4,
            0x41 => {
                let c = self.get_c();
                self.set_b(c);
                4
            },
            0x42 => {
                let d = self.get_d();
                self.set_b(d);
                4
            },
            0x43 => {
                let e = self.get_e();
                self.set_b(e);
                4
            },
            0x44 => {
                let h = self.get_h();
                self.set_b(h);
                4
            },
            0x45 => {
                let l = self.get_l();
                self.set_b(l);
                4
            },
            0x46 => {
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                self.set_b(value);
                8
            },
            0x47 => {
                let a = self.get_a();
                self.set_b(a);
                4
            },
            0x48 => {
                let b = self.get_b();
                self.set_c(b);
                4
            },
            0x49 => 4,
            0x4A => {
                let d = self.get_d();
                self.set_c(d);
                4
            },
            0x4B => {
                let e = self.get_e();
                self.set_c(e);
                4
            },
            0x4C => {
                let h = self.get_h();
                self.set_c(h);
                4
            },
            0x4D => {
                let l = self.get_l();
                self.set_c(l);
                4
            },
            0x4E => {
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                self.set_c(value);
                8
            },
            0x4F => {
                let a = self.get_a();
                self.set_c(a);
                4
            },
            0x50 => {
                let b = self.get_b();
                self.set_d(b);
                4
            },
            0x51 => {
                let c = self.get_c();
                self.set_d(c);
                4
            },
            0x52 => 4,
            0x53 => {
                let e = self.get_e();
                self.set_d(e);
                4
            },
            0x54 => {
                let h = self.get_h();
                self.set_d(h);
                4
            },
            0x55 => {
                let l = self.get_l();
                self.set_d(l);
                4
            },
            0x56 => {
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                self.set_d(value);
                8
            },
            0x57 => {
                let a = self.get_a();
                self.set_d(a);
                4
            },
            0x58 => {
                let b = self.get_b();
                self.set_e(b);
                4
            },
            0x59 => {
                let c = self.get_c();
                self.set_e(c);
                4
            },
            0x5A => {
                let d = self.get_d();
                self.set_e(d);
                4
            },
            0x5B => 4,
            0x5C => {
                let h = self.get_h();
                self.set_e(h);
                4
            },
            0x5D => {
                let l = self.get_l();
                self.set_e(l);
                4
            },
            0x5E => {
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                self.set_e(value);
                8
            },
            0x5F => {
                let a = self.get_a();
                self.set_e(a);
                4
            },
            0x60 => {
                let b = self.get_b();
                self.set_h(b);
                4
            },
            0x61 => {
                let c = self.get_c();
                self.set_h(c);
                4
            },
            0x62 => {
                let d = self.get_d();
                self.set_h(d);
                4
            },
            0x63 => {
                let e = self.get_e();
                self.set_h(e);
                4
            },
            0x64 => 4,
            0x65 => {
                let l = self.get_l();
                self.set_h(l);
                4
            },
            0x66 => {
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                self.set_h(value);
                8
            },
            0x67 => {
                let a = self.get_a();
                self.set_h(a);
                4
            },
            0x68 => {
                let b = self.get_b();
                self.set_l(b);
                4
            },
            0x69 => {
                let c = self.get_c();
                self.set_l(c);
                4
            },
            0x6A => {
                let d = self.get_d();
                self.set_l(d);
                4
            },
            0x6B => {
                let e = self.get_e();
                self.set_l(e);
                4
            },
            0x6C => {
                let h = self.get_h();
                self.set_l(h);
                4
            },
            0x6D => 4,
            0x6E => {
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                self.set_l(value);
                8
            },
            0x6F => {
                let a = self.get_a();
                self.set_l(a);
                4
            },
            0x70 => {
                let b = self.get_b();
                let addr = self.get_hl();
                memory.write_byte(addr, b);
                8
            },
            0x71 => {
                let c = self.get_c();
                let addr = self.get_hl();
                memory.write_byte(addr, c);
                8
            },
            0x72 => {
                let d = self.get_d();
                let addr = self.get_hl();
                memory.write_byte(addr, d);
                8
            },
            0x73 => {
                let e = self.get_e();
                let addr = self.get_hl();
                memory.write_byte(addr, e);
                8
            },
            0x74 => {
                let h = self.get_h();
                let addr = self.get_hl();
                memory.write_byte(addr, h);
                8
            },
            0x75 => {
                let l = self.get_l();
                let addr = self.get_hl();
                memory.write_byte(addr, l);
                8
            },
            0x76 => {
                // Check for HALT bug condition
                if !self.ime && InterruptController::has_pending_interrupts(memory) {
                    // HALT bug triggered
                    self.halt_bug = true;
                    // In this case, HALT ends immediately
                } else {
                    // Normal HALT behavior
                    self.halted = true;
                }
                4
            },
            0x77 => {
                let a = self.get_a();
                let addr = self.get_hl();
                memory.write_byte(addr, a);
                8
            },
            0x78 => {
                let b = self.get_b();
                self.set_a(b);
                4
            },
            0x79 => {
                let c = self.get_c();
                self.set_a(c);
                4
            },
            0x7A => {
                let d = self.get_d();
                self.set_a(d);
                4
            },
            0x7B => {
                let e = self.get_e();
                self.set_a(e);
                4
            },
            0x7C => {
                let h = self.get_h();
                self.set_a(h);
                4
            },
            0x7D => {
                let l = self.get_l();
                self.set_a(l);
                4
            },
            0x7E => {
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                self.set_a(value);
                8
            },
            0x7F => 4,
            0x80 => {
                self.add_r8(self.get_b(), false);
                4
            },
            0x81 => {
                self.add_r8(self.get_c(), false);
                4
            },
            0x82 => {
                self.add_r8(self.get_d(), false);
                4
            },
            0x83 => {
                self.add_r8(self.get_e(), false);
                4
            },
            0x84 => {
                self.add_r8(self.get_h(), false);
                4
            },
            0x85 => {
                self.add_r8(self.get_l(), false);
                4
            },
            0x86 => {
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                self.add_r8(value, false);
                8
            },
            0x87 => {
                self.add_r8(self.get_a(), false);
                4
            },
            0x88 => {
                self.add_r8(self.get_b(), true);
                4
            },
            0x89 => {
                self.add_r8(self.get_c(), true);
                4
            },
            0x8A => {
                self.add_r8(self.get_d(), true);
                4
            },
            0x8B => {
                self.add_r8(self.get_e(), true);
                4
            },
            0x8C => {
                self.add_r8(self.get_h(), true);
                4
            },
            0x8D => {
                self.add_r8(self.get_l(), true);
                4
            },
            0x8E => {
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                self.add_r8(value, true);
                8
            },
            0x8F => {
                self.add_r8(self.get_a(), true);
                4
            },
            0x90 => {
                self.sub_r8(self.get_b(), false);
                4
            },
            0x91 => {
                self.sub_r8(self.get_c(), false);
                4
            },
            0x92 => {
                self.sub_r8(self.get_d(), false);
                4
            },
            0x93 => {
                self.sub_r8(self.get_e(), false);
                4
            },
            0x94 => {
                self.sub_r8(self.get_h(), false);
                4
            },
            0x95 => {
                self.sub_r8(self.get_l(), false);
                4
            },
            0x96 => {
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                self.sub_r8(value, false);
                8
            },
            0x97 => {
                self.sub_r8(self.get_a(), false);
                4
            },
            0x98 => {
                self.sub_r8(self.get_b(), true);
                4
            },
            0x99 => {
                self.sub_r8(self.get_c(), true);
                4
            },
            0x9A => {
                self.sub_r8(self.get_d(), true);
                4
            },
            0x9B => {
                self.sub_r8(self.get_e(), true);
                4
            },
            0x9C => {
                self.sub_r8(self.get_h(), true);
                4
            },
            0x9D => {
                self.sub_r8(self.get_l(), true);
                4
            },
            0x9E => {
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                self.sub_r8(value, true);
                8
            },
            0x9F => {
                self.sub_r8(self.get_a(), true);
                4
            },
            0xA0 => {
                self.and_r8(self.get_b());
                4
            },
            0xA1 => {
                self.and_r8(self.get_c());
                4
            },
            0xA2 => {
                self.and_r8(self.get_d());
                4
            },
            0xA3 => {
                self.and_r8(self.get_e());
                4
            },
            0xA4 => {
                self.and_r8(self.get_h());
                4
            },
            0xA5 => {
                self.and_r8(self.get_l());
                4
            },
            0xA6 => {
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                self.and_r8(value);
                8
            },
            0xA7 => {
                self.and_r8(self.get_a());
                4
            },
            0xA8 => {
                self.xor_r8(self.get_b());
                4
            },
            0xA9 => {
                self.xor_r8(self.get_c());
                4
            },
            0xAA => {
                self.xor_r8(self.get_d());
                4
            },
            0xAB => {
                self.xor_r8(self.get_e());
                4
            },
            0xAC => {
                self.xor_r8(self.get_h());
                4
            },
            0xAD => {
                self.xor_r8(self.get_l());
                4
            },
            0xAE => {
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                self.xor_r8(value);
                8
            },
            0xAF => {
                self.xor_r8(self.get_a());
                4
            },
            0xB0 => {
                self.or_r8(self.get_b());
                4
            },
            0xB1 => {
                self.or_r8(self.get_c());
                4
            },
            0xB2 => {
                self.or_r8(self.get_d());
                4
            },
            0xB3 => {
                self.or_r8(self.get_e());
                4
            },
            0xB4 => {
                self.or_r8(self.get_h());
                4
            },
            0xB5 => {
                self.or_r8(self.get_l());
                4
            },
            0xB6 => {
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                self.or_r8(value);
                8
            },
            0xB7 => {
                self.or_r8(self.get_a());
                4
            },
            0xB8 => {
                self.cp_r8(self.get_b());
                4
            },
            0xB9 => {
                self.cp_r8(self.get_c());
                4
            },
            0xBA => {
                self.cp_r8(self.get_d());
                4
            },
            0xBB => {
                self.cp_r8(self.get_e());
                4
            },
            0xBC => {
                self.cp_r8(self.get_h());
                4
            },
            0xBD => {
                self.cp_r8(self.get_l());
                4
            },
            0xBE => {
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                self.cp_r8(value);
                8
            },
            0xBF => {
                self.cp_r8(self.get_a());
                4
            },
            0xC0 => {
                self.ret_cc(memory, !self.f.z)
            },
            0xC1 => {
                let value = self.pop_word(memory);
                self.set_bc(value);
                12
            },
            0xC2 => {
                self.cpu_jp(memory, !self.f.z)
            },
            0xC3 => {
                self.cpu_jp(memory, true)
            },
            0xC4 => {
                self.call_cc(memory, !self.f.z)
            },
            0xC5 => {
                self.push_word(memory, self.get_bc());
                16
            },
            0xC6 => {
                let value = self.fetch_byte(memory);
                self.add_r8(value, false);
                8
            },
            0xC7 => {
                self.push_word(memory, self.pc);
                self.pc = 0x00;
                16
            },
            0xC8 => {
                self.ret_cc(memory, self.f.z)
            },
            0xC9 => {
                self.pc = self.pop_word(memory);
                16
            },
            0xCA => {
                self.cpu_jp(memory, self.f.z)
            },
            0xCB => {
                self.call_cb(memory)
            },
            0xCC => {
                self.call_cc(memory, self.f.z)
            },
            0xCD => {
                self.call(memory)
            },
            0xCE => {
                let value = self.fetch_byte(memory);
                self.add_r8(value, true);
                8
            },
            0xCF => {
                self.push_word(memory, self.pc);
                self.pc = 0x08;
                16
            },
            0xD0 => {
                self.ret_cc(memory, !self.f.c)
            },
            0xD1 => {
                let value = self.pop_word(memory);
                self.set_de(value);
                12
            },
            0xD2 => {
                self.cpu_jp(memory, !self.f.c)
            },
            0xD4 => {
                self.call_cc(memory, !self.f.c)
            },
            0xD5 => {
                self.push_word(memory, self.get_de());
                16
            },
            0xD6 => {
                let value = self.fetch_byte(memory);
                self.sub_r8(value, false);
                8
            },
            0xD7 => {
                self.push_word(memory, self.pc);
                self.pc = 0x10;
                16
            },
            0xD8 => {
                self.ret_cc(memory, self.f.c)
            },
            0xD9 => {
                self.pc = self.pop_word(memory);
                self.ime = true;  // Enable interrupts immediately after RETI
                16
            },
            0xDA => {
                self.cpu_jp(memory, self.f.c)
            },
            0xDC => {
                self.call_cc(memory, self.f.c)
            },
            0xDE => {
                let value = self.fetch_byte(memory);
                self.sub_r8(value, true);
                8
            },
            0xDF => {
                self.push_word(memory, self.pc);
                self.pc = 0x18;
                16
            },
            0xE0 => {
                let addr = 0xFF00 | self.fetch_byte(memory) as u16;
                memory.write_byte(addr, self.get_a());
                12
            },
            0xE1 => {
                let value = self.pop_word(memory);
                self.set_hl(value);
                12
            },
            0xE2 => {
                let addr = 0xFF00 | self.get_c() as u16;
                memory.write_byte(addr, self.get_a());
                8
            },
            0xE5 => {
                self.push_word(memory, self.get_hl());
                16
            },
            0xE6 => {
                let value = self.fetch_byte(memory);
                self.and_r8(value);
                8
            },
            0xE7 => {
                self.push_word(memory, self.pc);
                self.pc = 0x20;
                16
            },
            0xE8 => {
                let value = self.add16_imm(memory, self.sp);
                self.sp = value;
                16
            },
            0xE9 => {
                self.pc = self.get_hl();
                4
            },
            0xEA => {
                let addr = self.fetch_word(memory);
                memory.write_byte(addr, self.get_a());
                16
            },
            0xEE => {
                let value = self.fetch_byte(memory);
                self.xor_r8(value);
                8
            },
            0xEF => {
                self.push_word(memory, self.pc);
                self.pc = 0x28;
                16
            },
            0xF0 => {
                let addr = 0xFF00 | self.fetch_byte(memory) as u16;
                let value = memory.read_byte(addr);
                self.set_a(value);
                12
            },
            0xF1 => {
                let value = self.pop_word(memory);
                self.set_af(value);
                12
            },
            0xF2 => {
                let addr = 0xFF00 | self.get_c() as u16;
                let value = memory.read_byte(addr);
                self.set_a(value);
                8
            },
            0xF3 => {
                self.ime = false;
                4
            },
            0xF5 => {
                self.push_word(memory, self.get_af());
                16
            },
            0xF6 => {
                let value = self.fetch_byte(memory);
                self.or_r8(value);
                8
            },
            0xF7 => {
                self.push_word(memory, self.pc);
                self.pc = 0x30;
                16
            },
            0xF8 => {
                let value = self.add16_imm(memory, self.sp);
                self.set_hl(value);
                12
            },
            0xF9 => {
                self.sp = self.get_hl();
                8
            },
            0xFA => {
                let addr = self.fetch_word(memory);
                let value = memory.read_byte(addr);
                self.set_a(value);
                16
            },
            0xFB => {
                self.pending_ime = true;
                4
            },
            0xFE => {
                let value = self.fetch_byte(memory);
                self.cp_r8(value);
                8
            },
            0xFF => {
                self.push_word(memory, self.pc);
                self.pc = 0x38;
                16
            },
            _ => {
                println!("Unimplemented opcode: 0x{:02X}", opcode);
                4
            }
        }
    }

    fn call_cb<'a>(&mut self, memory: &mut MemoryBus<'a>) -> u8 {
        let opcode = self.fetch_byte(memory);
        match opcode {
            0x00 => {
                let b = self.get_b();
                let r = self.rlc_r8(b);
                self.set_b(r);
                8
            },
            0x01 => {
                let c = self.get_c();
                let r = self.rlc_r8(c);
                self.set_c(r);
                8
            },
            0x02 => {
                let d = self.get_d();
                let r = self.rlc_r8(d);
                self.set_d(r);
                8
            },
            0x03 => {
                let e = self.get_e();
                let r = self.rlc_r8(e);
                self.set_e(r);
                8
            },
            0x04 => {
                let h = self.get_h();
                let r = self.rlc_r8(h);
                self.set_h(r);
                8
            },
            0x05 => {
                let l = self.get_l();
                let r = self.rlc_r8(l);
                self.set_l(r);
                8
            },
            0x06 => {
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                let r = self.rlc_r8(value);
                memory.write_byte(addr, r);
                16
            },
            0x07 => {
                let a = self.get_a();
                let r = self.rlc_r8(a);
                self.set_a(r);
                8
            },
            0x08 => {
                let b = self.get_b();
                let r = self.rrc_r8(b);
                self.set_b(r);
                8
            },
            0x09 => {
                let c = self.get_c();
                let r = self.rrc_r8(c);
                self.set_c(r);
                8
            },
            0x0A => {
                let d = self.get_d();
                let r = self.rrc_r8(d);
                self.set_d(r);
                8
            },
            0x0B => {
                let e = self.get_e();
                let r = self.rrc_r8(e);
                self.set_e(r);
                8
            },
            0x0C => {
                let h = self.get_h();
                let r = self.rrc_r8(h);
                self.set_h(r);
                8
            },
            0x0D => {
                let l = self.get_l();
                let r = self.rrc_r8(l);
                self.set_l(r);
                8
            },
            0x0E => {
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                let r = self.rrc_r8(value);
                memory.write_byte(addr, r);
                16
            },
            0x0F => {
                let a = self.get_a();
                let r = self.rrc_r8(a);
                self.set_a(r);
                8
            },
            0x10 => {
                let b = self.get_b();
                let r = self.rl_r8(b);
                self.set_b(r);
                8
            },
            0x11 => {
                let c = self.get_c();
                let r = self.rl_r8(c);
                self.set_c(r);
                8
            },
            0x12 => {
                let d = self.get_d();
                let r = self.rl_r8(d);
                self.set_d(r);
                8
            },
            0x13 => {
                let e = self.get_e();
                let r = self.rl_r8(e);
                self.set_e(r);
                8
            },
            0x14 => {
                let h = self.get_h();
                let r = self.rl_r8(h);
                self.set_h(r);
                8
            },
            0x15 => {
                let l = self.get_l();
                let r = self.rl_r8(l);
                self.set_l(r);
                8
            },
            0x16 => {
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                let r = self.rl_r8(value);
                memory.write_byte(addr, r);
                16
            },
            0x17 => {
                let a = self.get_a();
                let r = self.rl_r8(a);
                self.set_a(r);
                8
            },
            0x18 => {
                let b = self.get_b();
                let r = self.rr_r8(b);
                self.set_b(r);
                8
            },
            0x19 => {
                let c = self.get_c();
                let r = self.rr_r8(c);
                self.set_c(r);
                8
            },
            0x1A => {
                let d = self.get_d();
                let r = self.rr_r8(d);
                self.set_d(r);
                8
            },
            0x1B => {
                let e = self.get_e();
                let r = self.rr_r8(e);
                self.set_e(r);
                8
            },
            0x1C => {
                let h = self.get_h();
                let r = self.rr_r8(h);
                self.set_h(r);
                8
            },
            0x1D => {
                let l = self.get_l();
                let r = self.rr_r8(l);
                self.set_l(r);
                8
            },
            0x1E => {
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                let r = self.rr_r8(value);
                memory.write_byte(addr, r);
                16
            },
            0x1F => {
                let a = self.get_a();
                let r = self.rr_r8(a);
                self.set_a(r);
                8
            },
            0x20 => {
                let b = self.get_b();
                let r = self.sla_r8(b);
                self.set_b(r);
                8
            },
            0x21 => {
                let c = self.get_c();
                let r = self.sla_r8(c);
                self.set_c(r);
                8
            },
            0x22 => {
                let d = self.get_d();
                let r = self.sla_r8(d);
                self.set_d(r);
                8
            },
            0x23 => {
                let e = self.get_e();
                let r = self.sla_r8(e);
                self.set_e(r);
                8
            },
            0x24 => {
                let h = self.get_h();
                let r = self.sla_r8(h);
                self.set_h(r);
                8
            },
            0x25 => {
                let l = self.get_l();
                let r = self.sla_r8(l);
                self.set_l(r);
                8
            },
            0x26 => {
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                let r = self.sla_r8(value);
                memory.write_byte(addr, r);
                16
            },
            0x27 => {
                let a = self.get_a();
                let r = self.sla_r8(a);
                self.set_a(r);
                8
            },
            0x28 => {
                let b = self.get_b();
                let r = self.sra_r8(b);
                self.set_b(r);
                8
            },
            0x29 => {
                let c = self.get_c();
                let r = self.sra_r8(c);
                self.set_c(r);
                8
            },
            0x2A => {
                let d = self.get_d();
                let r = self.sra_r8(d);
                self.set_d(r);
                8
            },
            0x2B => {
                let e = self.get_e();
                let r = self.sra_r8(e);
                self.set_e(r);
                8
            },
            0x2C => {
                let h = self.get_h();
                let r = self.sra_r8(h);
                self.set_h(r);
                8
            },
            0x2D => {
                let l = self.get_l();
                let r = self.sra_r8(l);
                self.set_l(r);
                8
            },
            0x2E => {
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                let r = self.sra_r8(value);
                memory.write_byte(addr, r);
                16
            },
            0x2F => {
                let a = self.get_a();
                let r = self.sra_r8(a);
                self.set_a(r);
                8
            },
            0x30 => {
                let b = self.get_b();
                let r = self.swap_r8(b);
                self.set_b(r);
                8
            },
            0x31 => {
                let c = self.get_c();
                let r = self.swap_r8(c);
                self.set_c(r);
                8
            },
            0x32 => {
                let d = self.get_d();
                let r = self.swap_r8(d);
                self.set_d(r);
                8
            },
            0x33 => {
                let e = self.get_e();
                let r = self.swap_r8(e);
                self.set_e(r);
                8
            },
            0x34 => {
                let h = self.get_h();
                let r = self.swap_r8(h);
                self.set_h(r);
                8
            },
            0x35 => {
                let l = self.get_l();
                let r = self.swap_r8(l);
                self.set_l(r);
                8
            },
            0x36 => {
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                let r = self.swap_r8(value);
                memory.write_byte(addr, r);
                16
            },
            0x37 => {
                let a = self.get_a();
                let r = self.swap_r8(a);
                self.set_a(r);
                8
            },
            0x38 => {
                let b = self.get_b();
                let r = self.srl_r8(b);
                self.set_b(r);
                8
            },
            0x39 => {
                let c = self.get_c();
                let r = self.srl_r8(c);
                self.set_c(r);
                8
            },
            0x3A => {
                let d = self.get_d();
                let r = self.srl_r8(d);
                self.set_d(r);
                8
            },
            0x3B => {
                let e = self.get_e();
                let r = self.srl_r8(e);
                self.set_e(r);
                8
            },
            0x3C => {
                let h = self.get_h();
                let r = self.srl_r8(h);
                self.set_h(r);
                8
            },
            0x3D => {
                let l = self.get_l();
                let r = self.srl_r8(l);
                self.set_l(r);
                8
            },
            0x3E => {
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                let r = self.srl_r8(value);
                memory.write_byte(addr, r);
                16
            },
            0x3F => {
                let a = self.get_a();
                let r = self.srl_r8(a);
                self.set_a(r);
                8
            },
            0x40 => { 
                self.bit_r8(self.get_b(), 0);
                8
            },
            0x41 => { 
                self.bit_r8(self.get_c(), 0);
                8
            },
            0x42 => { 
                self.bit_r8(self.get_d(), 0);
                8
            },
            0x43 => { 
                self.bit_r8(self.get_e(), 0);
                8
            },
            0x44 => { 
                self.bit_r8(self.get_h(), 0);
                8
            },
            0x45 => { 
                self.bit_r8(self.get_l(), 0);
                8
            },
            0x46 => { 
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                self.bit_r8(value, 0);
                12
            },
            0x47 => { 
                self.bit_r8(self.get_a(), 0);
                8
            },
            0x48 => { 
                self.bit_r8(self.get_b(), 1);
                8
            },
            0x49 => { 
                self.bit_r8(self.get_c(), 1);
                8
            },
            0x4A => { 
                self.bit_r8(self.get_d(), 1);
                8
            },
            0x4B => { 
                self.bit_r8(self.get_e(), 1);
                8
            },
            0x4C => { 
                self.bit_r8(self.get_h(), 1);
                8
            },
            0x4D => { 
                self.bit_r8(self.get_l(), 1);
                8
            },
            0x4E => { 
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                self.bit_r8(value, 1);
                12
            },
            0x4F => { 
                self.bit_r8(self.get_a(), 1);
                8
            },
            0x50 => { 
                self.bit_r8(self.get_b(), 2);
                8
            },
            0x51 => { 
                self.bit_r8(self.get_c(), 2);
                8
            },
            0x52 => { 
                self.bit_r8(self.get_d(), 2);
                8
            },
            0x53 => { 
                self.bit_r8(self.get_e(), 2);
                8
            },
            0x54 => { 
                self.bit_r8(self.get_h(), 2);
                8
            },
            0x55 => { 
                self.bit_r8(self.get_l(), 2);
                8
            },
            0x56 => { 
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                self.bit_r8(value, 2);
                12
            },
            0x57 => { 
                self.bit_r8(self.get_a(), 2);
                8
            },
            0x58 => { 
                self.bit_r8(self.get_b(), 3);
                8
            },
            0x59 => { 
                self.bit_r8(self.get_c(), 3);
                8
            },
            0x5A => { 
                self.bit_r8(self.get_d(), 3);
                8
            },
            0x5B => { 
                self.bit_r8(self.get_e(), 3);
                8
            },
            0x5C => { 
                self.bit_r8(self.get_h(), 3);
                8
            },
            0x5D => { 
                self.bit_r8(self.get_l(), 3);
                8
            },
            0x5E => { 
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                self.bit_r8(value, 3);
                12
            },
            0x5F => { 
                self.bit_r8(self.get_a(), 3);
                8
            },
            0x60 => { 
                self.bit_r8(self.get_b(), 4);
                8
            },
            0x61 => { 
                self.bit_r8(self.get_c(), 4);
                8
            },
            0x62 => { 
                self.bit_r8(self.get_d(), 4);
                8
            },
            0x63 => { 
                self.bit_r8(self.get_e(), 4);
                8
            },
            0x64 => { 
                self.bit_r8(self.get_h(), 4);
                8
            },
            0x65 => { 
                self.bit_r8(self.get_l(), 4);
                8
            },
            0x66 => { 
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                self.bit_r8(value, 4);
                12
            },
            0x67 => { 
                self.bit_r8(self.get_a(), 4);
                8
            },
            0x68 => { 
                self.bit_r8(self.get_b(), 5);
                8
            },
            0x69 => { 
                self.bit_r8(self.get_c(), 5);
                8
            },
            0x6A => { 
                self.bit_r8(self.get_d(), 5);
                8
            },
            0x6B => { 
                self.bit_r8(self.get_e(), 5);
                8
            },
            0x6C => { 
                self.bit_r8(self.get_h(), 5);
                8
            },
            0x6D => { 
                self.bit_r8(self.get_l(), 5);
                8
            },
            0x6E => { 
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                self.bit_r8(value, 5);
                12
            },
            0x6F => { 
                self.bit_r8(self.get_a(), 5);
                8
            },
            0x70 => { 
                self.bit_r8(self.get_b(), 6);
                8
            },
            0x71 => { 
                self.bit_r8(self.get_c(), 6);
                8
            },
            0x72 => { 
                self.bit_r8(self.get_d(), 6);
                8
            },
            0x73 => { 
                self.bit_r8(self.get_e(), 6);
                8
            },
            0x74 => { 
                self.bit_r8(self.get_h(), 6);
                8
            },
            0x75 => { 
                self.bit_r8(self.get_l(), 6);
                8
            },
            0x76 => { 
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                self.bit_r8(value, 6);
                12
            },
            0x77 => { 
                self.bit_r8(self.get_a(), 6);
                8
            },
            0x78 => { 
                self.bit_r8(self.get_b(), 7);
                8
            },
            0x79 => { 
                self.bit_r8(self.get_c(), 7);
                8
            },
            0x7A => { 
                self.bit_r8(self.get_d(), 7);
                8
            },
            0x7B => { 
                self.bit_r8(self.get_e(), 7);
                8
            },
            0x7C => { 
                self.bit_r8(self.get_h(), 7);
                8
            },
            0x7D => { 
                self.bit_r8(self.get_l(), 7);
                8
            },
            0x7E => { 
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                self.bit_r8(value, 7);
                12
            },
            0x7F => { 
                self.bit_r8(self.get_a(), 7);
                8
            },
            0x80 => { 
                let r = self.get_b() & !(1 << 0);
                self.set_b(r);
                8
            },
            0x81 => { 
                let r = self.get_c() & !(1 << 0);
                self.set_c(r);
                8
            },
            0x82 => { 
                let r = self.get_d() & !(1 << 0);
                self.set_d(r);
                8
            },
            0x83 => { 
                let r = self.get_e() & !(1 << 0);
                self.set_e(r);
                8
            },
            0x84 => { 
                let r = self.get_h() & !(1 << 0);
                self.set_h(r);
                8
            },
            0x85 => { 
                let r = self.get_l() & !(1 << 0);
                self.set_l(r);
                8
            },
            0x86 => { 
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                let r = value & !(1 << 0);
                memory.write_byte(addr, r);
                16
            },
            0x87 => { 
                let r = self.get_a() & !(1 << 0);
                self.set_a(r);
                8
            },
            0x88 => { 
                let r = self.get_b() & !(1 << 1);
                self.set_b(r);
                8
            },
            0x89 => { 
                let r = self.get_c() & !(1 << 1);
                self.set_c(r);
                8
            },
            0x8A => { 
                let r = self.get_d() & !(1 << 1);
                self.set_d(r);
                8
            },
            0x8B => { 
                let r = self.get_e() & !(1 << 1);
                self.set_e(r);
                8
            },
            0x8C => { 
                let r = self.get_h() & !(1 << 1);
                self.set_h(r);
                8
            },
            0x8D => { 
                let r = self.get_l() & !(1 << 1);
                self.set_l(r);
                8
            },
            0x8E => { 
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                let r = value & !(1 << 1);
                memory.write_byte(addr, r);
                16
            },
            0x8F => { 
                let r = self.get_a() & !(1 << 1);
                self.set_a(r);
                8
            },
            0x90 => { 
                let r = self.get_b() & !(1 << 2);
                self.set_b(r);
                8
            },
            0x91 => { 
                let r = self.get_c() & !(1 << 2);
                self.set_c(r);
                8
            },
            0x92 => { 
                let r = self.get_d() & !(1 << 2);
                self.set_d(r);
                8
            },
            0x93 => { 
                let r = self.get_e() & !(1 << 2);
                self.set_e(r);
                8
            },
            0x94 => { 
                let r = self.get_h() & !(1 << 2);
                self.set_h(r);
                8
            },
            0x95 => { 
                let r = self.get_l() & !(1 << 2);
                self.set_l(r);
                8
            },
            0x96 => { 
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                let r = value & !(1 << 2);
                memory.write_byte(addr, r);
                16
            },
            0x97 => { 
                let r = self.get_a() & !(1 << 2);
                self.set_a(r);
                8
            },
            0x98 => { 
                let r = self.get_b() & !(1 << 3);
                self.set_b(r);
                8
            },
            0x99 => { 
                let r = self.get_c() & !(1 << 3);
                self.set_c(r);
                8
            },
            0x9A => { 
                let r = self.get_d() & !(1 << 3);
                self.set_d(r);
                8
            },
            0x9B => { 
                let r = self.get_e() & !(1 << 3);
                self.set_e(r);
                8
            },
            0x9C => { 
                let r = self.get_h() & !(1 << 3);
                self.set_h(r);
                8
            },
            0x9D => { 
                let r = self.get_l() & !(1 << 3);
                self.set_l(r);
                8
            },
            0x9E => { 
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                let r = value & !(1 << 3);
                memory.write_byte(addr, r);
                16
            },
            0x9F => { 
                let r = self.get_a() & !(1 << 3);
                self.set_a(r);
                8
            },
            0xA0 => { 
                let r = self.get_b() & !(1 << 4);
                self.set_b(r);
                8
            },
            0xA1 => { 
                let r = self.get_c() & !(1 << 4);
                self.set_c(r);
                8
            },
            0xA2 => { 
                let r = self.get_d() & !(1 << 4);
                self.set_d(r);
                8
            },
            0xA3 => { 
                let r = self.get_e() & !(1 << 4);
                self.set_e(r);
                8
            },
            0xA4 => { 
                let r = self.get_h() & !(1 << 4);
                self.set_h(r);
                8
            },
            0xA5 => { 
                let r = self.get_l() & !(1 << 4);
                self.set_l(r);
                8
            },
            0xA6 => { 
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                let r = value & !(1 << 4);
                memory.write_byte(addr, r);
                16
            },
            0xA7 => { 
                let r = self.get_a() & !(1 << 4);
                self.set_a(r);
                8
            },
            0xA8 => { 
                let r = self.get_b() & !(1 << 5);
                self.set_b(r);
                8
            },
            0xA9 => { 
                let r = self.get_c() & !(1 << 5);
                self.set_c(r);
                8
            },
            0xAA => { 
                let r = self.get_d() & !(1 << 5);
                self.set_d(r);
                8
            },
            0xAB => { 
                let r = self.get_e() & !(1 << 5);
                self.set_e(r);
                8
            },
            0xAC => { 
                let r = self.get_h() & !(1 << 5);
                self.set_h(r);
                8
            },
            0xAD => { 
                let r = self.get_l() & !(1 << 5);
                self.set_l(r);
                8
            },
            0xAE => { 
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                let r = value & !(1 << 5);
                memory.write_byte(addr, r);
                16
            },
            0xAF => { 
                let r = self.get_a() & !(1 << 5);
                self.set_a(r);
                8
            },
            0xB0 => { 
                let r = self.get_b() & !(1 << 6);
                self.set_b(r);
                8
            },
            0xB1 => { 
                let r = self.get_c() & !(1 << 6);
                self.set_c(r);
                8
            },
            0xB2 => { 
                let r = self.get_d() & !(1 << 6);
                self.set_d(r);
                8
            },
            0xB3 => { 
                let r = self.get_e() & !(1 << 6);
                self.set_e(r);
                8
            },
            0xB4 => { 
                let r = self.get_h() & !(1 << 6);
                self.set_h(r);
                8
            },
            0xB5 => { 
                let r = self.get_l() & !(1 << 6);
                self.set_l(r);
                8
            },
            0xB6 => { 
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                let r = value & !(1 << 6);
                memory.write_byte(addr, r);
                16
            },
            0xB7 => { 
                let r = self.get_a() & !(1 << 6);
                self.set_a(r);
                8
            },
            0xB8 => { 
                let r = self.get_b() & !(1 << 7);
                self.set_b(r);
                8
            },
            0xB9 => { 
                let r = self.get_c() & !(1 << 7);
                self.set_c(r);
                8
            },
            0xBA => { 
                let r = self.get_d() & !(1 << 7);
                self.set_d(r);
                8
            },
            0xBB => { 
                let r = self.get_e() & !(1 << 7);
                self.set_e(r);
                8
            },
            0xBC => { 
                let r = self.get_h() & !(1 << 7);
                self.set_h(r);
                8
            },
            0xBD => { 
                let r = self.get_l() & !(1 << 7);
                self.set_l(r);
                8
            },
            0xBE => { 
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                let r = value & !(1 << 7);
                memory.write_byte(addr, r);
                16
            },
            0xBF => { 
                let r = self.get_a() & !(1 << 7);
                self.set_a(r);
                8
            },
            0xC0 => { 
                let r = self.get_b() | (1 << 0);
                self.set_b(r);
                8
            },
            0xC1 => { 
                let r = self.get_c() | (1 << 0);
                self.set_c(r);
                8
            },
            0xC2 => { 
                let r = self.get_d() | (1 << 0);
                self.set_d(r);
                8
            },
            0xC3 => { 
                let r = self.get_e() | (1 << 0);
                self.set_e(r);
                8
            },
            0xC4 => { 
                let r = self.get_h() | (1 << 0);
                self.set_h(r);
                8
            },
            0xC5 => { 
                let r = self.get_l() | (1 << 0);
                self.set_l(r);
                8
            },
            0xC6 => { 
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                let r = value | (1 << 0);
                memory.write_byte(addr, r);
                16
            },
            0xC7 => { 
                let r = self.get_a() | (1 << 0);
                self.set_a(r);
                8
            },
            0xC8 => { 
                let r = self.get_b() | (1 << 1);
                self.set_b(r);
                8
            },
            0xC9 => { 
                let r = self.get_c() | (1 << 1);
                self.set_c(r);
                8
            },
            0xCA => { 
                let r = self.get_d() | (1 << 1);
                self.set_d(r);
                8
            },
            0xCB => { 
                let r = self.get_e() | (1 << 1);
                self.set_e(r);
                8
            },
            0xCC => { 
                let r = self.get_h() | (1 << 1);
                self.set_h(r);
                8
            },
            0xCD => { 
                let r = self.get_l() | (1 << 1);
                self.set_l(r);
                8
            },
            0xCE => { 
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                let r = value | (1 << 1);
                memory.write_byte(addr, r);
                16
            },
            0xCF => { 
                let r = self.get_a() | (1 << 1);
                self.set_a(r);
                8
            },
            0xD0 => { 
                let r = self.get_b() | (1 << 2);
                self.set_b(r);
                8
            },
            0xD1 => { 
                let r = self.get_c() | (1 << 2);
                self.set_c(r);
                8
            },
            0xD2 => { 
                let r = self.get_d() | (1 << 2);
                self.set_d(r);
                8
            },
            0xD3 => { 
                let r = self.get_e() | (1 << 2);
                self.set_e(r);
                8
            },
            0xD4 => { 
                let r = self.get_h() | (1 << 2);
                self.set_h(r);
                8
            },
            0xD5 => { 
                let r = self.get_l() | (1 << 2);
                self.set_l(r);
                8
            },
            0xD6 => { 
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                let r = value | (1 << 2);
                memory.write_byte(addr, r);
                16
            },
            0xD7 => { 
                let r = self.get_a() | (1 << 2);
                self.set_a(r);
                8
            },
            0xD8 => { 
                let r = self.get_b() | (1 << 3);
                self.set_b(r);
                8
            },
            0xD9 => { 
                let r = self.get_c() | (1 << 3);
                self.set_c(r);
                8
            },
            0xDA => { 
                let r = self.get_d() | (1 << 3);
                self.set_d(r);
                8
            },
            0xDB => { 
                let r = self.get_e() | (1 << 3);
                self.set_e(r);
                8
            },
            0xDC => { 
                let r = self.get_h() | (1 << 3);
                self.set_h(r);
                8
            },
            0xDD => { 
                let r = self.get_l() | (1 << 3);
                self.set_l(r);
                8
            },
            0xDE => { 
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                let r = value | (1 << 3);
                memory.write_byte(addr, r);
                16
            },
            0xDF => { 
                let r = self.get_a() | (1 << 3);
                self.set_a(r);
                8
            },
            0xE0 => { 
                let r = self.get_b() | (1 << 4);
                self.set_b(r);
                8
            },
            0xE1 => { 
                let r = self.get_c() | (1 << 4);
                self.set_c(r);
                8
            },
            0xE2 => { 
                let r = self.get_d() | (1 << 4);
                self.set_d(r);
                8
            },
            0xE3 => { 
                let r = self.get_e() | (1 << 4);
                self.set_e(r);
                8
            },
            0xE4 => { 
                let r = self.get_h() | (1 << 4);
                self.set_h(r);
                8
            },
            0xE5 => { 
                let r = self.get_l() | (1 << 4);
                self.set_l(r);
                8
            },
            0xE6 => { 
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                let r = value | (1 << 4);
                memory.write_byte(addr, r);
                16
            },
            0xE7 => { 
                let r = self.get_a() | (1 << 4);
                self.set_a(r);
                8
            },
            0xE8 => { 
                let r = self.get_b() | (1 << 5);
                self.set_b(r);
                8
            },
            0xE9 => { 
                let r = self.get_c() | (1 << 5);
                self.set_c(r);
                8
            },
            0xEA => { 
                let r = self.get_d() | (1 << 5);
                self.set_d(r);
                8
            },
            0xEB => { 
                let r = self.get_e() | (1 << 5);
                self.set_e(r);
                8
            },
            0xEC => { 
                let r = self.get_h() | (1 << 5);
                self.set_h(r);
                8
            },
            0xED => { 
                let r = self.get_l() | (1 << 5);
                self.set_l(r);
                8
            },
            0xEE => { 
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                let r = value | (1 << 5);
                memory.write_byte(addr, r);
                16
            },
            0xEF => { 
                let r = self.get_a() | (1 << 5);
                self.set_a(r);
                8
            },
            0xF0 => { 
                let r = self.get_b() | (1 << 6);
                self.set_b(r);
                8
            },
            0xF1 => { 
                let r = self.get_c() | (1 << 6);
                self.set_c(r);
                8
            },
            0xF2 => { 
                let r = self.get_d() | (1 << 6);
                self.set_d(r);
                8
            },
            0xF3 => { 
                let r = self.get_e() | (1 << 6);
                self.set_e(r);
                8
            },
            0xF4 => { 
                let r = self.get_h() | (1 << 6);
                self.set_h(r);
                8
            },
            0xF5 => { 
                let r = self.get_l() | (1 << 6);
                self.set_l(r);
                8
            },
            0xF6 => { 
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                let r = value | (1 << 6);
                memory.write_byte(addr, r);
                16
            },
            0xF7 => { 
                let r = self.get_a() | (1 << 6);
                self.set_a(r);
                8
            },
            0xF8 => { 
                let r = self.get_b() | (1 << 7);
                self.set_b(r);
                8
            },
            0xF9 => { 
                let r = self.get_c() | (1 << 7);
                self.set_c(r);
                8
            },
            0xFA => { 
                let r = self.get_d() | (1 << 7);
                self.set_d(r);
                8
            },
            0xFB => { 
                let r = self.get_e() | (1 << 7);
                self.set_e(r);
                8
            },
            0xFC => { 
                let r = self.get_h() | (1 << 7);
                self.set_h(r);
                8
            },
            0xFD => { 
                let r = self.get_l() | (1 << 7);
                self.set_l(r);
                8
            },
            0xFE => { 
                let addr = self.get_hl();
                let value = memory.read_byte(addr);
                let r = value | (1 << 7);
                memory.write_byte(addr, r);
                16
            },
            0xFF => { 
                let r = self.get_a() | (1 << 7);
                self.set_a(r);
                8
            },
        }
    }

    fn call<'a>(&mut self, memory: &mut MemoryBus<'a>) -> u8 {
        self.push_word(memory, self.pc + 2);
        let addr = self.fetch_word(memory);
        self.pc = addr;
        24
    }

    fn call_cc<'a>(&mut self, memory: &mut MemoryBus<'a>, condition: bool) -> u8 {
        if condition {
            self.push_word(memory, self.pc + 2);
            let addr = self.fetch_word(memory);
            self.pc = addr;
            24
        } else {
            self.pc = self.pc.wrapping_add(2);
            12
        }
    }

    fn cpu_jp<'a>(&mut self, memory: &mut MemoryBus<'a>, condition: bool) -> u8 {
        if condition {
            self.pc = self.fetch_word(memory);
            16
        } else {
            self.pc = self.pc.wrapping_add(2);
            12
        }
    }

    fn ret_cc<'a>(&mut self, memory: &mut MemoryBus<'a>, condition: bool) -> u8 {
        if condition {
            self.pc = self.pop_word(memory);
            20
        } else {
            8
        }
    }

    fn inc_r8(&mut self, value: u8) -> u8 {
        let result = value.wrapping_add(1);
        // Set or reset flags using the flag() method
        self.flag(CpuFlag::Z, result == 0);
        self.flag(CpuFlag::H, (value & 0x0F) + 1 > 0x0F);
        self.flag(CpuFlag::N, false);
        result
    }

    fn dec_r8(&mut self, value: u8) -> u8 {
        let result = value.wrapping_sub(1);
        // Set or reset flags using the flag() method
        self.flag(CpuFlag::Z, result == 0);
        self.flag(CpuFlag::H, (value & 0x0F) == 0);
        self.flag(CpuFlag::N, true);
        result
    }

    fn add16(&mut self, value: u16) {
        let hl = self.get_hl();
        let result = hl.wrapping_add(value);
        self.flag(CpuFlag::C, hl > 0xFFFF - value);
        self.flag(CpuFlag::H, (hl & 0x0FFF) + (value & 0x0FFF) > 0x0FFF);
        self.flag(CpuFlag::N, false);
        self.set_hl(result);
    }

    fn add16_imm(&mut self, memory: &mut MemoryBus, value: u16) -> u16 {
        let b = self.fetch_byte(memory) as i8 as i16 as u16;
        self.flag(CpuFlag::C, (value & 0x00FF) + (b & 0x00FF) > 0x00FF);
        self.flag(CpuFlag::H, (value & 0x000F) + (b & 0x000F) > 0x000F);
        self.flag(CpuFlag::N, false);
        self.flag(CpuFlag::Z, false);

        value.wrapping_add(b)
    }

    fn srflagupdate(&mut self, value: u8, c: bool) {
        self.flag(CpuFlag::C, c);
        self.flag(CpuFlag::H, false);
        self.flag(CpuFlag::N, false);
        self.flag(CpuFlag::Z, value == 0);
    }

    fn swap_r8(&mut self, value: u8) -> u8 {
        self.flag(CpuFlag::C, false);
        self.flag(CpuFlag::H, false);
        self.flag(CpuFlag::N, false);
        self.flag(CpuFlag::Z, value == 0);
        (value >> 4) | (value << 4)
    }

    fn rlc_r8(&mut self, value: u8) -> u8 {
        let c = value & 0x80 == 0x80;
        let result = (value << 1) | if c { 0x01 } else { 0x00 };
        self.srflagupdate(result, c);
        result
    }

    fn rl_r8(&mut self, value: u8) -> u8 {
        let c = value & 0x80 == 0x80;
        let result = (value << 1) | if self.f.c { 0x01 } else { 0x00 };
        self.srflagupdate(result, c);
        result
    }

    fn rrc_r8(&mut self, value: u8) -> u8 {
        let c = value & 0x01 == 0x01;
        let result = (value >> 1) | if c { 0x80 } else { 0x00 };
        self.srflagupdate(result, c);
        result
    }

    fn rr_r8(&mut self, value: u8) -> u8 {
        let c = value & 0x01 == 0x01;
        let result = (value >> 1) | if self.f.c { 0x80 } else { 0x00 };
        self.srflagupdate(result, c);
        result
    }

    fn sla_r8(&mut self, value: u8) -> u8 {
        let c = value & 0x80 == 0x80;
        let result = value << 1;
        self.srflagupdate(result, c);
        result
    }

    fn sra_r8(&mut self, value: u8) -> u8 {
        let c = value & 0x01 == 0x01;
        let result = (value >> 1) | (value & 0x80);
        self.srflagupdate(result, c);
        result
    }

    fn srl_r8(&mut self, value: u8) -> u8 {
        let c = value & 0x01 == 0x01;
        let result = value >> 1;
        self.srflagupdate(result, c);
        result
    }

    fn bit_r8(&mut self, value: u8, bit: u8) {
        let result = value & (1 << (bit as u32)) == 0;
        self.flag(CpuFlag::H, true);
        self.flag(CpuFlag::N, false);
        self.flag(CpuFlag::Z, result);
    }

    fn daa(&mut self) {
        let mut a = self.get_a();
        let mut adjust = if self.f.c { 0x60 } else { 0x00 };
        if self.f.h { adjust |= 0x06; };
        if !self.f.n {
            if a & 0x0F > 0x09 { adjust |= 0x06; };
            if a > 0x99 { adjust |= 0x60; };
            a = a.wrapping_add(adjust);
        } else {
            a = a.wrapping_sub(adjust);
        }

        self.flag(CpuFlag::C, adjust >= 0x60);
        self.flag(CpuFlag::H, false);
        self.flag(CpuFlag::Z, a == 0);
        self.set_a(a);
    }

    fn cpu_jr<'a>(&mut self, memory: &'a MemoryBus, condition: bool) -> u8 {
        if condition {
            let n = self.fetch_byte(memory) as i8;
            self.pc = ((self.pc as u32 as i32) + (n as i32)) as u16;
            12
        } else {
            self.pc = self.pc.wrapping_add(1);
            8
        }
    }

    fn add_r8(&mut self, value: u8, usec: bool) {
        let c = if usec && self.f.c { 1 } else { 0 };
        let a = self.get_a();
        let r = a.wrapping_add(value).wrapping_add(c);
        self.flag(CpuFlag::Z, r == 0);
        self.flag(CpuFlag::H, (a & 0xF) + ((value & 0xF) + c) > 0xF);
        self.flag(CpuFlag::N, false);
        self.flag(CpuFlag::C, (a as u16) + (value as u16) + (c as u16) > 0xFF);
        self.set_a(r);
    }

    fn sub_r8(&mut self, value: u8, usec: bool) {
        let c = if usec && self.f.c { 1 } else { 0 };
        let a = self.get_a();
        let r = a.wrapping_sub(value).wrapping_sub(c);
        self.flag(CpuFlag::Z, r == 0);
        self.flag(CpuFlag::H, (a & 0x0F) < ((value & 0x0F) + c));
        self.flag(CpuFlag::N, true);
        self.flag(CpuFlag::C, (a as u16) < (value as u16) + (c as u16));
        self.set_a(r);
    }

    fn and_r8(&mut self, value: u8) {
        let r = self.get_a() & value;
        self.flag(CpuFlag::Z, r == 0);
        self.flag(CpuFlag::H, true);
        self.flag(CpuFlag::C, false);
        self.flag(CpuFlag::N, false);
        self.set_a(r);
    }

    fn or_r8(&mut self, value: u8) {
        let r = self.get_a() | value;
        self.flag(CpuFlag::Z, r == 0);
        self.flag(CpuFlag::C, false);
        self.flag(CpuFlag::H, false);
        self.flag(CpuFlag::N, false);
        self.set_a(r);
    }

    fn xor_r8(&mut self, value: u8) {
        let r = self.get_a() ^ value;
        self.flag(CpuFlag::Z, r == 0);
        self.flag(CpuFlag::C, false);
        self.flag(CpuFlag::H, false);
        self.flag(CpuFlag::N, false);
        self.set_a(r);
    }

    fn cp_r8(&mut self, value: u8) {
        let a = self.get_a();
        self.sub_r8(value, false);
        self.set_a(a);
    }
}