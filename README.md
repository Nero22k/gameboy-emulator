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

## Checklist

### **1. CPU (LR35902)**
- [ ] **Opcode Coverage**  
  - [ ] Main 8-bit ALU ops (ADD, ADC, SUB, SBC, AND, OR, XOR, CP, INC, DEC, etc.)  
  - [ ] 16-bit ops (ADD HL, ADD SP, etc.)  
  - [ ] Rotate/Shift ops (RLC, RRC, RL, RR, SLA, SRA, SRL, SWAP)  
  - [ ] Bitwise ops (BIT, SET, RES)  
  - [ ] Jumps/Calls/Returns (JP, JR, CALL, RET, RETI, etc.)  
  - [ ] Special instructions (DAA, CPL, CCF, SCF, etc.)  
  - [ ] CB-prefix table (extended instructions)  
- [ ] **Instruction Timing** (correct cycle counts)  
- [ ] **Flag Handling** (Z, N, H, C)  
- [ ] **Interrupt Master Enable (IME) & Handling**  
  - [ ] DI, EI instructions  
  - [ ] Proper timing (IME set one instruction after EI)  
- [ ] **Halt/Stop** states  
- [ ] **PC, SP, Register Startup Values**  
- [ ] **Boot ROM Support** (Optional)

### **2. Memory / Bus**
- [ ] **Cartridge Integration** (MBC0, MBC1, MBC2 and more)  
- [ ] **VRAM (0x8000–0x9FFF)**  
- [ ] **WRAM (0xC000–0xDFFF) & Echo (0xE000–0xFDFF)**  
- [ ] **OAM (0xFE00–0xFE9F)**  
- [ ] **I/O Registers (0xFF00–0xFF7F)**  
- [ ] **HRAM (0xFF80–0xFFFE)**  
- [ ] **Interrupt Enable (0xFFFF)**  

### **3. Timers**
- [ ] **DIV Register**
- [ ] **TIMA/TMA/TAC**
- [ ] **Timer frequencies** (4096, 262144, 65536, 16384 Hz)
- [ ] **Exact timing edge cases**

### **4. Interrupts**
- [ ] **IF (0xFF0F) & IE (0xFFFF)**  
- [ ] **Priority** (VBlank, LCD STAT, Timer, Serial, Joypad)  
- [ ] **Interrupt Vectors** (0x40, 0x48, 0x50, 0x58, 0x60)  
- [ ] **Push PC on stack, jump to vector, IME disable**
- [ ] **Interrupt delays / real hardware quirks** (e.g., EI sets IME after 1 instruction)

### **5. PPU (Graphics)**
- [ ] **LCD Control (LCDC)**  
- [ ] **LCD Status (STAT)** with modes (OAM scan, VRAM read, HBlank, VBlank)  
- [ ] **Scanline Timing** (each line ~456 cycles, 144 visible lines, etc.)  
- [ ] **Tile & Background Rendering**  
  - [ ] BG tile map / tile data addressing  
  - [ ] Window rendering  
  - [ ] Sprite rendering (OAM) / Priority / Overlap rules  
  - [ ] Palettes (BGP, OBP0, OBP1)  
- [ ] **VBlank interrupt**  
- [ ] **LCD STAT interrupt**

### **6. APU (Audio)**
- [ ] **Square Channel 1** (Sweep, Envelope, Length)  
- [ ] **Square Channel 2**  
- [ ] **Wave Channel 3** (Wave RAM)  
- [ ] **Noise Channel 4**  
- [ ] **Channel mixing** (Vin, L/R output select)  
- [ ] **Frame Sequencer** (512 Hz timer)  
- [ ] **Rodio Integration** (stream samples in real-time)

### **7. Joypad / Input**
- [ ] **Button states** in `Joypad` struct  
- [ ] **P1 Register** (0xFF00) bits for direction/buttons  
- [ ] **Interrupt on button press** (bit 4 in IF)  
- [ ] **Minifb Key Mapping** to D-pad, A/B, Start, Select

### **8. DMA**
- [ ] **OAM DMA**
- [ ] **Timing** 
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
- [ ] **Pass basic test ROMs** (e.g., Blargg’s CPU tests, Mooneye-GB tests)  
- [ ] **Save States** (serialize CPU/PPU/APU states)

---
