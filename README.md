# Game Boy Emulator
## Project Overview

- **CPU (LR35902)**: Executes Game Boy instructions, handles interrupts, manages registers.
- **Memory Bus**: Coordinates reads/writes across cartridge ROM/RAM, VRAM, WRAM, I/O registers, HRAM, etc.
- **Cartridge (MBC Support)**: Handles game ROMs and banking.
- **PPU**: Handles the pixel processing (background, window, and sprite rendering).
- **APU**: Emulates the audio channels (square waves, wave channel, noise).
- **Timer**: Manages DIV, TIMA, TMA, TAC, and requests timer interrupts.
- **DMA**: Handles OAM DMA for sprite data transfers.
- **Joypad**: Reads input from user and updates the joypad register.
- **Display**: Renders a 160×144 frame buffer to an on-screen window.

---

## Games

![Tennis_World](https://github.com/user-attachments/assets/75b1aa41-e49c-4b4c-84bf-63380e3dc411)

![Alleyway](https://github.com/user-attachments/assets/8af08853-9e49-41ad-a97e-625f3fa430e2)

![Dr_Mario](https://github.com/user-attachments/assets/18213b10-ca14-42be-a7bf-806109ecfb0f)

![Tetris](https://github.com/user-attachments/assets/3d170c48-e174-4542-ade5-5d6bc3fb4355)

## VRAM Viewer

![VRAM_Viewer](https://github.com/user-attachments/assets/59c05fb3-4eb4-4d8b-974b-be22438244cc)

## Passed tests:

### CPU tests:

| Test              | Status |
|-------------------|--------|
| `cpu_instrs.gb`   | ✅      |
| `halt_bug.gb`     | ✅      |
| `instr_timing.gb` | ✅      |
| `daa.gb`          | ✅      |

#### Timer tests:

| Test                      | Status |
|---------------------------|---------|
| `div_write.gb`            | ✅      |
| `tim00.gb`                | ✅      |
| `tim00_div_trigger.gb`    | ✅      |
| `tim01.gb`                | ✅      |
| `tim01_div_trigger.gb`    | ✅      |
| `tim10.gb`                | ✅      |
| `tim10_div_trigger.gb`    | ✅      |
| `tim11.gb`                | ✅      |
| `tim11_div_trigger.gb`    | ✅      |
| `tima_reload.gb`          | ✅      |
| `div_timing.gb`           | ✅      |

#### Bits tests:
| Test                      | Status |
|---------------------------|---------|
| `mem_oam.gb`              | ✅      |
| `reg_f.gb`                | ✅      |

#### Interrupts tests:
| Test                      | Status |
|---------------------------|---------|
| `ie_push.gb`              | ✅      |

#### OAM DMA tests:
| Test                      | Status |
|---------------------------|---------|
| `basic.gb`                | ✅      |
| `reg_read.gb`             | ✅      |

#### Misc tests:
| Test                      | Status |
|---------------------------|---------|
| `boot_regs-dmgABC.gb`     | ✅      |
| `if_ie_registers.gb`      | ✅      |

## Checklist

### **1. CPU (LR35902)**
- [X] **Opcode Coverage**  
  - [X] Main 8-bit ALU ops (ADD, ADC, SUB, SBC, AND, OR, XOR, CP, INC, DEC, etc.)  
  - [X] 16-bit ops (ADD HL, ADD SP, etc.)  
  - [X] Rotate/Shift ops (RLC, RRC, RL, RR, SLA, SRA, SRL, SWAP)  
  - [X] Bitwise ops (BIT, SET, RES)  
  - [X] Jumps/Calls/Returns (JP, JR, CALL, RET, RETI, etc.)  
  - [X] Special instructions (DAA, CPL, CCF, SCF, etc.)  
  - [X] CB-prefix table (extended instructions)  
- [X] **Instruction Timing** (correct cycle counts)  
- [X] **Flag Handling** (Z, N, H, C)  
- [X] **Interrupt Master Enable (IME) & Handling**  
  - [X] DI, EI instructions  
  - [X] Proper timing (IME set one instruction after EI)  
- [X] **Halt/Stop** states  
- [X] **PC, SP, Register Startup Values**  
- [ ] **Boot ROM Support** (Optional)

### **2. Memory / Bus**
- [ ] **Cartridge Integration** (MBC0, MBC1, MBC2 and more)  
- [X] **VRAM (0x8000–0x9FFF)**  
- [X] **WRAM (0xC000–0xDFFF) & Echo (0xE000–0xFDFF)**  
- [X] **OAM (0xFE00–0xFE9F)**  
- [X] **I/O Registers (0xFF00–0xFF7F)**  
- [X] **HRAM (0xFF80–0xFFFE)**  
- [X] **Interrupt Enable (0xFFFF)**  

### **3. Timers**
- [X] **DIV Register**
- [X] **TIMA/TMA/TAC**
- [X] **Timer frequencies** (4096, 262144, 65536, 16384 Hz)
- [X] **Exact timing edge cases**

### **4. Interrupts**
- [X] **IF (0xFF0F) & IE (0xFFFF)**  
- [X] **Priority** (VBlank, LCD STAT, Timer, Serial, Joypad)  
- [X] **Interrupt Vectors** (0x40, 0x48, 0x50, 0x58, 0x60)  
- [X] **Push PC on stack, jump to vector, IME disable**
- [X] **Interrupt delays / real hardware quirks** (e.g., EI sets IME after 1 instruction)

### **5. PPU (Graphics)**
- [X] **LCD Control (LCDC)**  
- [X] **LCD Status (STAT)** with modes (OAM scan, VRAM read, HBlank, VBlank)  
- [X] **Scanline Timing** (each line ~456 cycles, 144 visible lines, etc.)  
- [X] **Tile & Background Rendering**  
  - [X] BG tile map / tile data addressing  
  - [X] Window rendering  
  - [X] Sprite rendering (OAM) / Priority / Overlap rules  
  - [X] Palettes (BGP, OBP0, OBP1)  
- [X] **VBlank interrupt**  
- [X] **LCD STAT interrupt**

### **6. APU (Audio)**
- [ ] **Square Channel 1** (Sweep, Envelope, Length)  
- [ ] **Square Channel 2**  
- [ ] **Wave Channel 3** (Wave RAM)  
- [ ] **Noise Channel 4**  
- [ ] **Channel mixing** (Vin, L/R output select)  
- [ ] **Frame Sequencer** (512 Hz timer)  
- [ ] **Rodio Integration** (stream samples in real-time)

### **7. Joypad / Input**
- [X] **Button states** in `Joypad` struct  
- [X] **P1 Register** (0xFF00) bits for direction/buttons  
- [X] **Interrupt on button press** (bit 4 in IF)  
- [X] **Key Mappings** to D-pad, A/B, Start, Select

### **8. DMA**
- [X] **OAM DMA**
- [X] **Timing** 
- [ ] **HDMA (on CGB)** if implementing Game Boy Color features

### **9. Boot**
- [ ] **Optional Boot ROM** (DMG or CGB BIOS)  
- [ ] **Check Nintendo logo compare** at 0x0104..0x0133  

### **10. Serial / Link Cable** (Optional)
- [ ] **Serial Transfer** registers (0xFF01 / 0xFF02)  
- [ ] **External linking** (two emulator instances or TCP-based link)  
- [ ] **Interrupt** on serial completion

### **11. Extended Cartridge Types** (Optional)
- [ ] **MBC5, MBC3 (with RTC), MBC4**, etc.  
- [ ] **Battery-backed saves** (persist `cart_ram` to disk)  

### **12. Game Boy Color (CGB)** (Optional)
- [ ] **Double-speed mode**  
- [ ] **Extended Palettes** (BG/OBJ palettes)  
- [ ] **HDMA** (HBlank DMA) and **General-Purpose DMA**  
- [ ] **Extra WRAM bank**  
- [ ] **Increased VRAM size (2 banks)**

### **13. Performance & Compatibility**
- [ ] **Cycle-Accurate Timing** for CPU, PPU, APU if aiming for high accuracy  
- [ ] **Speed** (60 FPS target) with no audio cracks or frame drops
- [ ] **Save States** (serialize CPU/PPU/APU states)

---
