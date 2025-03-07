// Pixel Processing Unit (PPU) module
// The PPU is responsible for rendering the graphics of the Game
use crate::interrupts::InterruptType;

pub const SCREEN_WIDTH: usize = 160;
pub const SCREEN_HEIGHT: usize = 144;

// LCD Registers
const LCDC: u16 = 0xFF40; // LCD Control
const STAT: u16 = 0xFF41; // LCDC Status
const SCY: u16 = 0xFF42;  // Scroll Y
const SCX: u16 = 0xFF43;  // Scroll X
const LY: u16 = 0xFF44;   // LCD Y-Coordinate
const LYC: u16 = 0xFF45;  // LY Compare
const DMA: u16 = 0xFF46;  // DMA Transfer (Using OAM RAM)
const BGP: u16 = 0xFF47;  // BG Palette Data
const OBP0: u16 = 0xFF48; // Object Palette 0 Data
const OBP1: u16 = 0xFF49; // Object Palette 1 Data
const WY: u16 = 0xFF4A;   // Window Y Position
const WX: u16 = 0xFF4B;   // Window X Position

// PPU Mode timing constants
const MODE2_CYCLES: u32 = 80;    // OAM scan (Mode 2) - 80 cycles
const MODE3_CYCLES: u32 = 172;   // Drawing (Mode 3) - 172 cycles (can vary based on sprites)
const LINE_CYCLES: u32 = 456;    // Total cycles per scanline

// LCD Mode
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LcdMode {
    HBlank = 0,     // Horizontal blanking (mode 0)
    VBlank = 1,     // Vertical blanking (mode 1)
    OamScan = 2,    // OAM RAM access (mode 2)
    Drawing = 3,    // Pixel transfer (mode 3)
}

#[derive(Clone, Copy, Debug)]
enum Palette {
    BGP,
    OBP0,
    OBP1,
}

// OAM Entry (Sprite Attributes)
#[derive(Clone, Copy, Debug)]
pub struct OamEntry {
    pub y_pos: u8,      // Y position on screen (minus 16)
    pub x_pos: u8,      // X position on screen (minus 8)
    pub tile_idx: u8,   // Tile index from 0x8000
    pub attributes: u8,  // Sprite attributes (flip, priority, palette)
}

impl OamEntry {
    fn new() -> Self {
        Self {
            y_pos: 0,
            x_pos: 0,
            tile_idx: 0,
            attributes: 0,
        }
    }

    fn from_bytes(bytes: &[u8; 4]) -> Self {
        Self {
            y_pos: bytes[0],
            x_pos: bytes[1],
            tile_idx: bytes[2],
            attributes: bytes[3],
        }
    }

    // Is sprite on current scanline?
    fn is_on_scanline(&self, ly: u8, sprite_size: u8) -> bool {
        let sprite_y = self.y_pos.wrapping_sub(16);
        ly >= sprite_y && ly < sprite_y.wrapping_add(sprite_size)
    }

    // Get priority flag (0 = Above BG, 1 = Behind non-zero BG)
    fn has_priority(&self) -> bool {
        self.attributes & 0x80 != 0
    }

    // Get Y-flip flag
    fn is_y_flipped(&self) -> bool {
        self.attributes & 0x40 != 0
    }

    // Get X-flip flag
    fn is_x_flipped(&self) -> bool {
        self.attributes & 0x20 != 0
    }

    // Get palette (0 = OBP0, 1 = OBP1)
    fn palette(&self) -> u8 {
        if self.attributes & 0x10 != 0 { 1 } else { 0 }
    }
}

// Background-to-OAM Priority
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BgOamPrio {
    OAMPrio,  // OAM has priority over background
    BGPrio,   // Background has priority over OAM
}

// Pixel color with priority information
#[derive(Clone, Copy, Debug)]
pub struct PixelData {
    pub color_idx: u8,             // Original color index (0-3)
    pub color: u8,                 // Final color after palette mapping
    pub priority: BgOamPrio,       // BG-to-OAM priority
}

pub struct Ppu {
    frame_buffer: [u8; SCREEN_WIDTH * SCREEN_HEIGHT * 4], // RGBA
    pub ui_frame_buffer: [u8; SCREEN_WIDTH * SCREEN_HEIGHT * 4],
    // VRAM
    vram: [u8; 0x2000],
    // OAM
    oam: [u8; 0xA0],
    // Parsed OAM entries for quick access
    pub oam_entries: [OamEntry; 40],
    // Current scanline sprites (max 10 per line)
    scanline_sprites: Vec<(usize, OamEntry)>, // (index, entry) pairs
    // Scanline data
    scanline_data: Vec<PixelData>,
    
    // LCD Registers
    pub lcdc: u8,
    pub stat: u8,
    pub scy: u8,
    pub scx: u8,
    latched_scx: u8,
    latched_scy: u8,
    pub ly: u8,
    pub lyc: u8,
    pub dma: u8,
    pub bgp: u8,
    pub obp0: u8,
    pub obp1: u8,
    pub wy: u8,
    pub wx: u8,

    // Window internal position counter
    window_line: u8,        // Current line in the window, separate from LY
    window_triggered: bool, // Window was triggered this frame
    window_active: bool,    // Window rendering is active

    // PPU Mode
    mode: LcdMode,
    cycle_count: u32,     // Cycles within the current scanline
    
    // STAT interrupt edge detection
    prev_stat_signal: bool,

    // Access control flags
    vram_accessible: bool,
    oam_accessible: bool,

    // For tracking OAM DMA
    pub oam_dma_active: bool,
    pub oam_dma_byte: u8,
    pub oam_dma_source: u16,
    frame_swapped: bool,
}

impl Ppu {
    pub fn new() -> Self {
        let mut ppu = Self {
            frame_buffer: [0xFF; SCREEN_WIDTH * SCREEN_HEIGHT * 4], // Initialize with white
            ui_frame_buffer: [0xFF; SCREEN_WIDTH * SCREEN_HEIGHT * 4],
            vram: [0; 0x2000],
            oam: [0; 0xA0],
            oam_entries: [OamEntry::new(); 40],
            scanline_sprites: Vec::with_capacity(10),
            scanline_data: Vec::with_capacity(256),
            
            lcdc: 0x91, // LCD & PPU are on by default
            stat: 0x85,
            scy: 0,
            scx: 0,
            latched_scx: 0,
            latched_scy: 0,
            ly: 0,
            lyc: 0,
            dma: 0xFF,
            bgp: 0xFC,
            obp0: 0xFF,
            obp1: 0xFF,
            wy: 0,
            wx: 0,
            
            window_line: 0,
            window_triggered: false,
            window_active: false,
            
            mode: LcdMode::VBlank,
            cycle_count: 0,
            
            prev_stat_signal: false,
            
            vram_accessible: true,
            oam_accessible: true,
            
            oam_dma_active: false,
            oam_dma_byte: 0,
            oam_dma_source: 0,
            frame_swapped: false,
        };
        
        // Initialize OAM entries from initial OAM data
        ppu.update_oam_entries();
        ppu
    }

    // Update OAM entries from raw OAM data
    fn update_oam_entries(&mut self) {
        for i in 0..40 {
            let start = i * 4;
            let mut bytes = [0u8; 4];
            bytes.copy_from_slice(&self.oam[start..start + 4]);
            self.oam_entries[i] = OamEntry::from_bytes(&bytes);
        }
    }

    // Read from VRAM
    pub fn read_vram(&self, addr: u16) -> u8 {
        if !self.vram_accessible && self.is_lcd_on() {
            // VRAM is inaccessible during certain modes when LCD is on
            return 0xFF;
        }
        self.vram[(addr - 0x8000) as usize]
    }

    // Write to VRAM
    pub fn write_vram(&mut self, addr: u16, value: u8) {
        if !self.vram_accessible && self.is_lcd_on() {
            // VRAM is inaccessible during certain modes when LCD is on
            return;
        }
        self.vram[(addr - 0x8000) as usize] = value;
    }

    // Read from OAM
    pub fn read_oam(&self, addr: u16) -> u8 {
        let oam_addr = (addr - 0xFE00) as usize;
        if oam_addr >= 0xA0 {
            return 0xFF; // Out of bounds
        }
        
        // Check if OAM is accessible based on the current mode
        if !self.oam_accessible && self.is_lcd_on() {
            // OAM is inaccessible during certain modes when LCD is on
            return 0xFF;
        }
        
        // Simulate OAM corruption during DMA
        if self.oam_dma_active {
            // Return 0xFF during DMA (simplified, actual behavior is more complex)
            return 0xFF;
        }
        
        self.oam[oam_addr]
    }
    
    // Write to OAM
    pub fn write_oam(&mut self, addr: u16, value: u8) {
        let oam_addr = (addr - 0xFE00) as usize;
        if oam_addr >= 0xA0 {
            return; // Out of bounds
        }
        
        // Check if OAM is accessible based on the current mode
        if !self.oam_accessible && self.is_lcd_on() {
            // OAM is inaccessible during certain modes when LCD is on
            return;
        }
        
        // OAM is inaccessible during DMA
        if self.oam_dma_active {
            return;
        }
        
        self.write_oam_internal(addr - 0xFE00, value);
    }
    
    // Internal method to write to OAM without access checks
    pub fn write_oam_internal(&mut self, oam_addr: u16, value: u8) {
        self.oam[oam_addr as usize] = value;
        
        // Update the corresponding OAM entry
        let entry_idx = (oam_addr / 4) as usize;
        if entry_idx < 40 {
            let byte_idx = oam_addr % 4;
            
            match byte_idx {
                0 => self.oam_entries[entry_idx].y_pos = value,
                1 => self.oam_entries[entry_idx].x_pos = value,
                2 => self.oam_entries[entry_idx].tile_idx = value,
                3 => self.oam_entries[entry_idx].attributes = value,
                _ => unreachable!(),
            }
        }
    }
    
    // Begin DMA transfer
    pub fn begin_oam_dma(&mut self, value: u8) {
        self.dma = value;
        self.oam_dma_active = true;
        self.oam_dma_byte = 0;
        self.oam_dma_source = (value as u16) << 8; // Source address is value * 0x100
    }

    // Read from a PPU register
    pub fn read_register(&self, addr: u16) -> u8 {
        match addr {
            LCDC => self.lcdc,
            STAT => {
                // Combine STAT register with current mode
                // Bits 0-1: Mode, Bit 2: LY=LYC flag, Bits 3-6: Interrupt sources, Bit 7: Always 1
                let mode_bits = self.mode as u8;
                let lyc_flag = if self.ly == self.lyc { 0x04 } else { 0x00 };
                0x80 | (self.stat & 0x78) | lyc_flag | mode_bits
            },
            SCY => self.scy,
            SCX => self.scx,
            LY => self.ly,
            LYC => self.lyc,
            DMA => self.dma,
            BGP => self.bgp,
            OBP0 => self.obp0,
            OBP1 => self.obp1,
            WY => self.wy,
            WX => self.wx,
            _ => 0xFF, // Invalid address
        }
    }
    
    // Write to a PPU register
    pub fn write_register(&mut self, addr: u16, value: u8) {
        match addr {
            LCDC => {
                let old_lcd_enabled = self.is_lcd_on();
                self.lcdc = value;
                
                // Handle LCD being turned off
                if old_lcd_enabled && !self.is_lcd_on() {
                    self.turn_lcd_off();
                }
                // When LCD is turned on, reset PPU state
                else if !old_lcd_enabled && self.is_lcd_on() {
                    self.cycle_count = 0;
                    self.mode = LcdMode::OamScan;
                    
                    // Update STAT mode and LY=LYC comparison
                    self.stat &= !0x03;  // Clear mode bits
                    self.stat |= LcdMode::OamScan as u8; // Set mode 2
                    
                    // Check LY=LYC
                    if self.ly == self.lyc {
                        self.stat |= 0x04; // Set coincidence flag
                    } else {
                        self.stat &= !0x04; // Clear coincidence flag
                    }
                }
            },
            STAT => {
                // Only bits 3-6 are writable, bit 7 always reads as 1
                self.stat = (value & 0x78) | (self.stat & 0x07) | 0x80;
                
                // Check if this causes a STAT interrupt
                self.check_stat_interrupt();
            },
            SCY => self.scy = value,
            SCX => self.scx = value,
            LY => {}, // LY is read-only
            LYC => {
                self.lyc = value;
                
                // Update LY=LYC flag immediately if LCD is on
                if self.is_lcd_on() {
                    if self.ly == self.lyc {
                        self.stat |= 0x04; // Set coincidence flag
                    } else {
                        self.stat &= !0x04; // Clear coincidence flag
                    }
                }
                
                // Check if this causes a STAT interrupt
                self.check_stat_interrupt();
            },
            DMA => self.begin_oam_dma(value),
            BGP => self.bgp = value,
            OBP0 => self.obp0 = value,
            OBP1 => self.obp1 = value,
            WY => self.wy = value,
            WX => self.wx = value,
            _ => {}, // Invalid address
        }
    }

    // Update the PPU state for one T-cycle
    pub fn update_cycle(&mut self) -> Option<InterruptType> {
        // Skip all processing if LCD is off
        if !self.is_lcd_on() {
            return None;
        }

        // Latch scroll registers at the beginning of each scanline
    if self.cycle_count == 0 {
        self.latched_scx = self.scx;
        self.latched_scy = self.scy;
    }
        
        let mut interrupt = None;
        
        // Process the current PPU state based on mode
        match self.mode {
            LcdMode::OamScan => {
                // OAM not accessible during OAM scan
                self.oam_accessible = false;
                self.vram_accessible = true;
                
                // End of OAM scan - prepare sprites and transition to Drawing mode
                if self.cycle_count == MODE2_CYCLES {
                    // Find sprites on the current scanline
                    if self.scanline_sprites.is_empty() {
                        self.prepare_sprites_for_scanline();
                    }
                    
                    // Transition to Drawing mode
                    self.change_mode(LcdMode::Drawing);
                    self.vram_accessible = false; // VRAM not accessible during Drawing
                }
            },
            
            LcdMode::Drawing => {
                // Both OAM and VRAM inaccessible during Drawing
                self.oam_accessible = false;
                self.vram_accessible = false;
                
                // Calculate Mode 3 duration (it can vary based on sprite count)
                let sprite_penalty = (self.scanline_sprites.len() as u32 * 6).min(60);
                let mode3_duration = MODE3_CYCLES + sprite_penalty;
                
                // End of Drawing - render scanline and transition to HBlank
                if self.cycle_count == MODE2_CYCLES + mode3_duration {
                    // Render the current scanline
                    if self.scanline_data.is_empty() {
                        self.render_scanline();
                    }
                    
                    // Check for window activation during this line
                    if self.is_window_enabled() && self.ly >= self.wy && self.wx <= 166 {
                        self.window_triggered = true;
                        if self.window_active {
                            self.window_line += 1;
                        } else {
                            self.window_active = true;
                        }
                    }
                    
                    // Transition to HBlank mode
                    self.change_mode(LcdMode::HBlank);
                    self.vram_accessible = true;  // VRAM accessible during HBlank
                    self.oam_accessible = true;   // OAM accessible during HBlank
                }
            },
            
            LcdMode::HBlank => {
                // Both OAM and VRAM accessible during HBlank
                self.oam_accessible = true;
                self.vram_accessible = true;
                
                // End of scanline
                if self.cycle_count == LINE_CYCLES {
                    // Reset cycle count and process the next line
                    self.cycle_count = 0;
                    self.ly += 1;
                    
                    // Check if we're at the VBlank boundary
                    if self.ly == 144 {
                        // Enter VBlank mode
                        self.change_mode(LcdMode::VBlank);
                        std::mem::swap(&mut self.frame_buffer, &mut self.ui_frame_buffer);
                        self.frame_swapped = true;
                        interrupt = Some(InterruptType::VBlank);
                    } else {
                        // Start the next scanline
                        self.change_mode(LcdMode::OamScan);
                        self.oam_accessible = false; // OAM scan begins immediately
                    }
                    
                    // Clear scanline buffers for the next line
                    self.scanline_sprites.clear();
                    self.scanline_data.clear();
                    
                    // Update LY=LYC flag for the new LY value
                    if self.ly == self.lyc {
                        self.stat |= 0x04; // Set coincidence flag
                    } else {
                        self.stat &= !0x04; // Clear coincidence flag
                    }
                    
                    // Check if this causes a STAT interrupt
                    if self.check_stat_interrupt() && interrupt.is_none() {
                        interrupt = Some(InterruptType::LcdStat);
                    }
                    
                    return interrupt; // Return early to avoid incrementing cycle_count again
                }
            },
            
            LcdMode::VBlank => {
                // Both OAM and VRAM accessible during VBlank
                self.oam_accessible = true;
                self.vram_accessible = true;
                
                // End of scanline during VBlank
                if self.cycle_count == LINE_CYCLES {
                    // Reset cycle count
                    self.cycle_count = 0;
                    self.ly += 1;
                    
                    // Check if we've reached the end of VBlank
                    if self.ly == 154 {
                        // Start a new frame
                        self.ly = 0;
                        self.window_line = 0;
                        self.window_active = false;
                        self.window_triggered = false;
                        
                        // Transition to OAM scan for the first line
                        self.change_mode(LcdMode::OamScan);
                        self.oam_accessible = false; // OAM scan begins immediately
                    }
                    
                    // Update LY=LYC flag for the new LY value
                    if self.ly == self.lyc {
                        self.stat |= 0x04; // Set coincidence flag
                    } else {
                        self.stat &= !0x04; // Clear coincidence flag
                    }
                    
                    // Check if this causes a STAT interrupt
                    if self.check_stat_interrupt() {
                        interrupt = Some(InterruptType::LcdStat);
                    }
                    
                    return interrupt; // Return early to avoid incrementing cycle_count again
                }
            },
        }
        
        // Increment the cycle count
        self.cycle_count += 1;
        
        interrupt
    }

    // Check if a STAT interrupt should be triggered (rising edge detection)
    fn check_stat_interrupt(&mut self) -> bool {
        // Calculate STAT interrupt signal based on current state
        let lyc_int = (self.stat & 0x40) != 0 && (self.stat & 0x04) != 0;
        let mode_0_int = (self.stat & 0x08) != 0 && self.mode == LcdMode::HBlank;
        let mode_1_int = (self.stat & 0x10) != 0 && self.mode == LcdMode::VBlank;
        let mode_2_int = (self.stat & 0x20) != 0 && self.mode == LcdMode::OamScan;
        
        let stat_signal = lyc_int || mode_0_int || mode_1_int || mode_2_int;
        
        // Detect rising edge (0 -> 1 transition)
        let triggered = !self.prev_stat_signal && stat_signal;
        self.prev_stat_signal = stat_signal;
        
        triggered
    }

    // Change the PPU mode and update STAT register
    fn change_mode(&mut self, new_mode: LcdMode) {
        if self.mode != new_mode {
            // Update the mode bits in STAT register
            self.stat &= !0x03; // Clear mode bits
            self.stat |= new_mode as u8; // Set new mode
            
            // Update internal mode
            self.mode = new_mode;
        }
    }

    // Turn the LCD off (LCDC bit 7 = 0)
    fn turn_lcd_off(&mut self) {
        // Reset PPU state when LCD is turned off
        self.ly = 0;
        self.window_line = 0;
        self.window_active = false;
        self.window_triggered = false;
        
        self.cycle_count = 0;
        self.mode = LcdMode::HBlank;
        
        // Set mode to 0 in STAT register
        self.stat &= !0x03;
        
        // Make VRAM and OAM accessible
        self.vram_accessible = true;
        self.oam_accessible = true;
    }

    // Prepare sprites for the current scanline
    fn prepare_sprites_for_scanline(&mut self) {
        self.scanline_sprites.clear();
        
        // Skip if sprites are disabled
        if !self.is_obj_enabled() {
            return;
        }
        
        // Determine sprite size (8x8 or 8x16)
        let sprite_size = if self.is_sprite_8x16() { 16 } else { 8 };
        
        // Collect all sprites that are visible on this scanline
        for (idx, sprite) in self.oam_entries.iter().enumerate() {
            if sprite.is_on_scanline(self.ly, sprite_size) {
                self.scanline_sprites.push((idx, *sprite));
            }
        }
        
        // Sort sprites by X-coordinate (lower X has priority)
        // If X coordinates are equal, lower OAM index has priority
        self.scanline_sprites.sort_by(|(idx_a, sprite_a), (idx_b, sprite_b)| {
            sprite_a.x_pos.cmp(&sprite_b.x_pos)
                .then_with(|| idx_a.cmp(idx_b))
        });
        
        // Limit to 10 sprites per scanline (hardware limitation)
        if self.scanline_sprites.len() > 10 {
            self.scanline_sprites.truncate(10);
        }
        
        // Reverse to process from highest to lowest priority
        self.scanline_sprites.reverse();
    }

    // Render the current scanline
    fn render_scanline(&mut self) {
        // Clear the scanline buffer
        self.scanline_data.clear();
        self.scanline_data.resize(256, PixelData {
            color_idx: 0,
            color: self.get_dmg_color(0, Palette::BGP),
            priority: BgOamPrio::BGPrio,
        });
        
        // Render background if enabled
        if self.is_bg_enabled() {
            self.render_background();
        }
        
        // Render window if enabled and visible
        if self.is_window_enabled() && self.window_active && self.is_window_visible() {
            self.render_window();
        }
        
        // Render sprites if enabled
        if self.is_obj_enabled() {
            self.render_sprites();
        }
        
        // Transfer the scanline to the frame buffer
        self.draw_scanline_to_frame_buffer();
    }

    // Render the background layer
    fn render_background(&mut self) {
        // The correct tile map address based on LCDC bit 3
        // LCDC bit 3 (value 0x08) determines which background tile map to use
        let bg_tile_map = if (self.lcdc & 0x08) == 0 { 0x9800 } else { 0x9C00 };
        
        // Use the latched scroll values for the entire scanline
        let y = self.latched_scy.wrapping_add(self.ly);
        let tile_row = (y / 8) as u16; // Which row of tiles
        let tile_y = y % 8;           // Which line within the tile
        
        // Determine the tile data addressing mode from LCDC bit 4
        let unsigned_addressing = (self.lcdc & 0x10) != 0;
        
        // For each pixel in the scanline
        for x in 0..256 {
            // Calculate X position within background map with correct wrapping
            let bg_x = (self.latched_scx as u16 + x as u16) & 0xFF;
            let tile_col = (bg_x / 8) as u16;
            let tile_x = bg_x % 8;
            
            // Get the right tile from the map
            // Background maps are 32x32 tiles, so wrap at 32
            let map_idx = ((tile_row & 0x1F) * 32 + (tile_col & 0x1F)) as usize;
            let tile_map_addr = (bg_tile_map - 0x8000) + map_idx;
            
            // Safety check to prevent out-of-bounds access
            if tile_map_addr >= self.vram.len() {
                continue;
            }
            
            // Get the tile index from the background map
            let tile_idx = self.vram[tile_map_addr];
            
            // Get tile data address based on addressing mode
            let tile_data_addr = if unsigned_addressing {
                // 8000 addressing mode (unsigned)
                0x8000 + (tile_idx as u16 * 16)
            } else {
                // 8800 addressing mode (signed)
                if tile_idx < 128 {
                    0x9000 + (tile_idx as u16 * 16)
                } else {
                    0x8800 + ((tile_idx - 128) as u16 * 16)
                }
            };
            
            // Calculate the specific row address within the tile
            let row_addr = tile_data_addr + (tile_y as u16 * 2);
            
            // Read the two bytes that define this row of the tile
            let low_byte_addr = (row_addr - 0x8000) as usize;
            let high_byte_addr = low_byte_addr + 1;
            
            // Safety check again
            if high_byte_addr >= self.vram.len() {
                continue;
            }
            
            let low_byte = self.vram[low_byte_addr];
            let high_byte = self.vram[high_byte_addr];
            
            // Get the bit for this specific pixel
            let bit_pos = 7 - (tile_x as u8);
            let low_bit = (low_byte >> bit_pos) & 1;
            let high_bit = (high_byte >> bit_pos) & 1;
            let color_idx = (high_bit << 1) | low_bit;
            
            // Set the pixel in the scanline buffer (if within bounds)
            if x < self.scanline_data.len() {
                self.scanline_data[x] = PixelData {
                    color_idx,
                    color: self.get_dmg_color(color_idx, Palette::BGP),
                    priority: BgOamPrio::BGPrio,
                };
            }
        }
    }

    // Render the window layer
    fn render_window(&mut self) {
        // Only render if window is visible at current scanline
        if self.wx > 166 {
            return;
        }
        
        // Get window map area
        let win_tile_map = if self.lcdc & 0x40 == 0 { 0x9800 } else { 0x9C00 };
        
        // Calculate Y position within window
        let win_y = self.window_line;
        let tile_row = (win_y / 8) as u16;
        let tile_y = win_y % 8;
        
        // Calculate window X position (WX-7 is actually the left edge of the window)
        let win_x_start = if self.wx < 7 { 0 } else { (self.wx as i16 - 7) as usize };
        
        // Process window pixels
        for x in win_x_start..SCREEN_WIDTH {
            // Calculate position within window
            let win_x = (x - win_x_start) as u8;
            let tile_col = (win_x / 8) as u16;
            let tile_x = win_x % 8;
            
            // Get tile index from the window map
            let tile_map_addr = win_tile_map - 0x8000 + ((tile_row as usize) * 32) + (tile_col as usize);
            if tile_map_addr >= 0x2000 {
                continue; // Out of bounds
            }
            
            let tile_idx = self.vram[tile_map_addr];
            
            // Get tile data address based on LCDC bit 4
            let tile_data_addr = self.get_tile_data_address(tile_idx);
            
            // Get the specific tile row we need
            let tile_addr = tile_data_addr + (tile_y as u16 * 2);
            
            // Read the two bytes of tile data for this row
            let low_byte = self.vram[(tile_addr - 0x8000) as usize];
            let high_byte = self.vram[(tile_addr - 0x8000 + 1) as usize];
            
            // Get the color bit for this pixel
            let bit_pos = 7 - tile_x;
            let low_bit = (low_byte >> bit_pos) & 1;
            let high_bit = (high_byte >> bit_pos) & 1;
            let color_idx = (high_bit << 1) | low_bit;
            
            // Only draw if within visible area (0-159)
            if x < SCREEN_WIDTH {
                self.scanline_data[x] = PixelData {
                    color_idx,
                    color: self.get_dmg_color(color_idx, Palette::BGP),
                    priority: BgOamPrio::BGPrio,
                };
            }
        }
    }

    // Render sprites for current scanline
    fn render_sprites(&mut self) {
        // Get sprite size from LCDC
        let sprite_size = if self.is_sprite_8x16() { 16 } else { 8 };
        
        // Process sprites from highest to lowest priority (already sorted)
        for &(_, sprite) in &self.scanline_sprites {
            let sprite_y = sprite.y_pos.wrapping_sub(16);
            let sprite_x = sprite.x_pos.wrapping_sub(8);
            
            // Skip sprites with X=0 (they still count toward 10 sprite limit though)
            if sprite.x_pos == 0 {
                continue;
            }
            
            // Calculate the Y offset within the sprite
            let mut y_offset = (self.ly as i16 - sprite_y as i16) as u8;
            if sprite.is_y_flipped() {
                y_offset = (sprite_size - 1) - y_offset;
            }
            
            // Calculate the correct tile index based on sprite size
            let tile_idx = if sprite_size == 16 {
                // For 8x16 sprites, bit 0 of tile index is ignored
                // Upper/lower tile is determined by Y coordinate
                if y_offset < 8 {
                    sprite.tile_idx & 0xFE // Upper tile (even)
                } else {
                    (sprite.tile_idx & 0xFE) + 1 // Lower tile (odd)
                }
            } else {
                sprite.tile_idx
            };
            
            // Adjust y_offset for double-height sprites
            let adjusted_y_offset = y_offset % 8;
            
            // Calculate tile data address
            let tile_addr = 0x8000 + (tile_idx as u16 * 16) + (adjusted_y_offset as u16 * 2);
            
            // Read tile data for this sprite row
            let low_byte = self.vram[(tile_addr - 0x8000) as usize];
            let high_byte = self.vram[(tile_addr - 0x8000 + 1) as usize];
            
            // Get the palette for this sprite
            let palette = if sprite.palette() == 0 {
                Palette::OBP0
            } else {
                Palette::OBP1
            };
            
            // Process each pixel in the sprite
            for x in 0..8 {
                // Apply X flip if needed
                let bit_pos = if sprite.is_x_flipped() { x } else { 7 - x };
                
                // Get color index for this pixel
                let low_bit = (low_byte >> bit_pos) & 1;
                let high_bit = (high_byte >> bit_pos) & 1;
                let sprite_color_idx = (high_bit << 1) | low_bit;
                
                // Transparent pixel (color 0) - skip
                if sprite_color_idx == 0 {
                    continue;
                }
                
                // Calculate the screen X position for this sprite pixel
                let screen_x = sprite_x.wrapping_add(x);
                
                // Skip if outside the visible screen
                if screen_x >= SCREEN_WIDTH as u8 {
                    continue;
                }
                
                // Get background pixel information
                let bg_pixel = &self.scanline_data[screen_x as usize];
                
                // Apply priority rules:
                // 1. If sprite has priority and BG is color 0, sprite wins
                // 2. If BG has priority (sprite attr bit 7 is set) and BG is not color 0, BG wins
                // 3. Otherwise sprite wins
                let sprite_wins = if sprite.has_priority() {
                    bg_pixel.color_idx == 0
                } else {
                    true
                };
                
                if sprite_wins {
                    // Sprite is visible - update the pixel
                    self.scanline_data[screen_x as usize] = PixelData {
                        color_idx: sprite_color_idx,
                        color: self.get_dmg_color(sprite_color_idx, palette),
                        priority: BgOamPrio::OAMPrio,
                    };
                }
            }
        }
    }

    // Draw the current scanline to the frame buffer
    fn draw_scanline_to_frame_buffer(&mut self) {
        let y = self.ly as usize;
        if y >= SCREEN_HEIGHT {
            return; // Safety check
        }
        
        for x in 0..SCREEN_WIDTH {
            let pixel = &self.scanline_data[x];
            let frame_idx = (y * SCREEN_WIDTH + x) * 4;
            
            // Convert the color to RGBA
            match pixel.color {
                0 => { // White
                    self.frame_buffer[frame_idx] = 224;     // R
                    self.frame_buffer[frame_idx + 1] = 248; // G
                    self.frame_buffer[frame_idx + 2] = 208; // B
                    self.frame_buffer[frame_idx + 3] = 255; // A
                },
                1 => { // Light Gray
                    self.frame_buffer[frame_idx] = 136;     // R
                    self.frame_buffer[frame_idx + 1] = 192; // G
                    self.frame_buffer[frame_idx + 2] = 112; // B
                    self.frame_buffer[frame_idx + 3] = 255; // A
                },
                2 => { // Dark Gray
                    self.frame_buffer[frame_idx] = 52;      // R
                    self.frame_buffer[frame_idx + 1] = 104; // G
                    self.frame_buffer[frame_idx + 2] = 86;  // B
                    self.frame_buffer[frame_idx + 3] = 255; // A
                },
                3 => { // Black
                    self.frame_buffer[frame_idx] = 8;       // R
                    self.frame_buffer[frame_idx + 1] = 24;  // G
                    self.frame_buffer[frame_idx + 2] = 32;  // B
                    self.frame_buffer[frame_idx + 3] = 255; // A
                },
                _ => {} // Should not happen
            }
        }
    }

    // Helper function to get the tile data address based on addressing mode
    pub fn is_frame_ready(&mut self) -> bool {
        if self.frame_swapped {
            self.frame_swapped = false;
            true
        } else {
            false
        }
    }
    fn get_tile_data_address(&self, tile_idx: u8) -> u16 {
        if self.lcdc & 0x10 != 0 {
            // 8000 addressing mode (unsigned)
            0x8000 + (tile_idx as u16 * 16)
        } else {
            // 8800 addressing mode (signed)
            if tile_idx < 128 {
                0x9000 + (tile_idx as u16 * 16)
            } else {
                0x8800 + ((tile_idx - 128) as u16 * 16)
            }
        }
    }

    // Get DMG color based on palette and color index
    fn get_dmg_color(&self, color_idx: u8, palette: Palette) -> u8 {
        let palette_value = match palette {
            Palette::BGP => self.bgp,
            Palette::OBP0 => self.obp0,
            Palette::OBP1 => self.obp1,
        };
        
        // Extract the color from the palette (2 bits per color)
        (palette_value >> (color_idx * 2)) & 0x03
    }

    // Utility functions for PPU state
    
    pub fn is_lcd_on(&self) -> bool {
        self.lcdc & 0x80 != 0
    }
    
    pub fn is_window_enabled(&self) -> bool {
        self.lcdc & 0x20 != 0
    }
    
    pub fn is_obj_enabled(&self) -> bool {
        self.lcdc & 0x02 != 0
    }
    
    pub fn is_bg_enabled(&self) -> bool {
        self.lcdc & 0x01 != 0
    }
    
    pub fn is_sprite_8x16(&self) -> bool {
        self.lcdc & 0x04 != 0
    }
    
    pub fn is_window_visible(&self) -> bool {
        // Window is visible if WX <= 166 and WY <= LY
        self.wx <= 166 && self.wy <= self.ly
    }
}