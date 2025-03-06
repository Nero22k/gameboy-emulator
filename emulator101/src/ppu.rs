// Pixel Processing Unit (PPU) module
// The PPU is responsible for rendering the graphics of the Game

// TODO: FIX STAT INTERRUPTS
// TODO: FIX SLOW FRAME RATE CAUSED BY BUSY WAITING
// TODO: FIX PPU SO THAT IT PASSES DMG-ACID TESTS and MOONEYE TESTS

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

// LCD Mode
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LcdMode {
    HBlank = 0,		// Horizontal blanking (mode 0)
    VBlank = 1,		// Vertical blanking (mode 1)
    OamScan = 2,	// OAM RAM access (mode 2)
    Drawing = 3,	// Pixel transfer (mode 3)
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

pub struct Ppu {
	pub frame_buffer: [u8; SCREEN_WIDTH * SCREEN_HEIGHT * 4], // RGBA
	// VRMA
	vram: [u8; 0x2000],
	// OAM
	oam: [u8; 0xA0],
    // Parsed OAM entries for quick access
    pub oam_entries: [OamEntry; 40],
    // Current scanline sprites (max 10 per line)
    scanline_sprites: Vec<(usize, OamEntry)>, // (index, entry) pairs
	// LCD Registers
	pub lcdc: u8,
	pub stat: u8,
	pub scy: u8,
	pub scx: u8,
	pub ly: u8,
	pub lyc: u8,
	pub dma: u8,
	pub bgp: u8,
	pub obp0: u8,
	pub obp1: u8,
	pub wy: u8,
    wy_triggered: bool,
	pub wx: u8,

    // Window internal position counter
    window_line: u8, // Current line in the window, separate from LY

	// PPU Mode
	mode: LcdMode,
	mode_cycles: u32,

    // Access control flags
    vram_accessible: bool,
    oam_accessible: bool,

	// For tracking when the frame is ready
	pub frame_ready: bool,

    // For tracking OAM Corruption
    pub oam_dma_active: bool,
    oam_dma_byte: u8,
    last_frame_window_active: bool,
    
    // LY=LYC interrupt already triggered for this line
    lyc_interrupt_triggered: bool,
    
    // CPU last read/write a locked area
    cpu_vram_bus_conflict: bool,
    cpu_oam_bus_conflict: bool,
}

impl Ppu {
	pub fn new() -> Self {
		let mut ppu = Self {
			frame_buffer: [0xFF; SCREEN_WIDTH * SCREEN_HEIGHT * 4], // Initialize with white
			vram: [0; 0x2000],
			oam: [0; 0xA0],
            oam_entries: [OamEntry::new(); 40],
            scanline_sprites: Vec::with_capacity(10),
			lcdc: 0x91, // LCD & PPU are on by default
			stat: 0x85, // Set STAT to mode 2
			scy: 0,
			scx: 0,
			ly: 0,
            lyc: 0,
            dma: 0xFF,
            bgp: 0xFC,
            obp0: 0xFF,
            obp1: 0xFF,
            wy: 0,
            wx: 0,
            wy_triggered: false,
            window_line: 0,
            mode: LcdMode::VBlank,
            mode_cycles: 0,
            vram_accessible: true,
            oam_accessible: true,
            frame_ready: false,
            oam_dma_active: false,
            oam_dma_byte: 0,
            last_frame_window_active: false,
            lyc_interrupt_triggered: false,
            cpu_vram_bus_conflict: false,
            cpu_oam_bus_conflict: false,
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
        if !self.vram_accessible && self.lcdc & 0x80 != 0 {
            return 0xFF;
        }
        self.vram[(addr - 0x8000) as usize]
    }

    // Write to VRAM
    pub fn write_vram(&mut self, addr: u16, value: u8) {
        if !self.vram_accessible && self.lcdc & 0x80 != 0 {
            self.cpu_vram_bus_conflict = true;
            return;
        }
        self.vram[(addr - 0x8000) as usize] = value;
    }

    pub fn get_dma_source(&self) -> u16 {
        (self.dma as u16) << 8
    }
    
    pub fn get_dma_byte(&self) -> u8 {
        self.oam_dma_byte
    }
    
    pub fn process_dma_byte(&mut self, value: u8) {
        if !self.oam_dma_active {
            return;
        }
        
        // Write to OAM directly (bypassing access check)
        self.oam[self.oam_dma_byte as usize] = value;
        
        // Update OAM entry if it's a 4-byte boundary
        if self.oam_dma_byte % 4 == 3 {
            let entry_idx = (self.oam_dma_byte / 4) as usize;
            if entry_idx < 40 {  // Safety check
                let start = entry_idx * 4;
                let mut bytes = [0u8; 4];
                bytes.copy_from_slice(&self.oam[start..start + 4]);
                self.oam_entries[entry_idx] = OamEntry::from_bytes(&bytes);
            }
        }
        
        self.oam_dma_byte += 1;
        
        // Check if DMA is complete
        if self.oam_dma_byte >= 160 {
            self.oam_dma_active = false;
            self.oam_dma_byte = 0;
            // Update all OAM entries after DMA completes
            self.update_oam_entries();
        }
    }
    
    // Read from OAM
    pub fn read_oam(&self, addr: u16) -> u8 {
        let oam_addr = (addr - 0xFE00) as usize;
        if oam_addr >= 0xA0 {
            return 0xFF; // Out of bounds
        }
        
        // Check if OAM is accessible based on the current mode
        if !self.oam_accessible {
            if self.lcdc & 0x80 != 0 { // LCD enabled
                // During modes 2 & 3 (OAM scan & pixel transfer), OAM is inaccessible
                return 0xFF;
            }
        }
        
        // Simulate OAM corruption during DMA
        if self.oam_dma_active {
            // OAM corruption - complex bug, simplified simulation 
            return 0xFF; // Corrupted read during DMA
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
        if !self.oam_accessible && self.lcdc & 0x80 != 0 {
            self.cpu_oam_bus_conflict = true;
            return;
        }
        
        // Simulate OAM corruption during DMA
        if self.oam_dma_active {
            // OAM is locked during DMA
            return;
        }
        
        self.oam[oam_addr] = value;
        
        // Update the corresponding OAM entry
        let entry_idx = oam_addr / 4;
        let byte_idx = oam_addr % 4;
        
        match byte_idx {
            0 => self.oam_entries[entry_idx].y_pos = value,
            1 => self.oam_entries[entry_idx].x_pos = value,
            2 => self.oam_entries[entry_idx].tile_idx = value,
            3 => self.oam_entries[entry_idx].attributes = value,
            _ => unreachable!(),
        }
    }
    
    // Begin DMA transfer
    fn begin_oam_dma(&mut self, value: u8) {
        self.dma = value;
        self.oam_dma_active = true;
        self.oam_dma_byte = 0;
    }

	// Read from a PPU register
    pub fn read_register(&self, addr: u16) -> u8 {
        match addr {
            LCDC => self.lcdc,
            STAT => {
                // Combine STAT register with current mode
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
            _ => 0xFF, // Should not happen
        }
    }
    
    // Write to a PPU register
    pub fn write_register(&mut self, addr: u16, value: u8) {
        match addr {
            LCDC => {
                let old_lcd_enable = self.lcdc & 0x80 != 0;
                let new_lcd_enable = value & 0x80 != 0;
                
                // Store the old value to detect changes
                let old_lcdc = self.lcdc;
                self.lcdc = value;
                
                // Turning LCD off
                if old_lcd_enable && !new_lcd_enable {
                    self.ly = 0;
                    self.mode = LcdMode::HBlank;
                    self.mode_cycles = 0;
                    self.vram_accessible = true;
                    self.oam_accessible = true;
                    self.window_line = 0;
                } else if !old_lcd_enable && new_lcd_enable {
                    // LCD turned on - initialize state
                    self.mode_cycles = 0;
                    self.mode = LcdMode::OamScan;
                }
                
                // Handle window enable/disable
                if (old_lcdc & 0x20) != (value & 0x20) {
                    // Window was toggled, make sure state is consistent
                    if value & 0x20 == 0 {
                        // Window disabled mid-frame
                        // Don't reset window_line here!
                    }
                }
            },
            STAT => {
                // Only bits 3-6 are writable, bit 7 always reads as 1
                let old_stat = self.stat;
                self.stat = 0x80 | (value & 0x78) | (self.stat & 0x07);
                
                // Check if LYC=LY interrupt was just enabled and condition is true
                if (old_stat & 0x40) == 0 && (value & 0x40) != 0 && (self.stat & 0x04) != 0 {
                    self.lyc_interrupt_triggered = true;
                }
            },
            SCY => self.scy = value,
            SCX => self.scx = value,
            LY => {}, // LY is read-only
            LYC => {
                let old_lyc = self.lyc;
                self.lyc = value;
                
                // Update coincidence flag immediately
                if self.ly == value {
                    self.stat |= 0x04; // Set coincidence flag
                    
                    // If coincidence interrupt enabled and LYC changed to match LY
                    if (self.stat & 0x40) != 0 && old_lyc != value {
                        self.lyc_interrupt_triggered = true;
                    }
                } else {
                    self.stat &= !0x04; // Clear coincidence flag
                }
            },
            DMA => self.begin_oam_dma(value),
            BGP => self.bgp = value,
            OBP0 => self.obp0 = value,
            OBP1 => self.obp1 = value,
            WY => self.wy = value,
            WX => self.wx = value,
            _ => {}, // Should not happen
        }
    }

    #[allow(dead_code)]
    fn debugging(&self) {
        println!("");
        println!("LCDC: {:#04X}", self.lcdc);
        println!("STAT: {:#04X}", self.stat);
        println!("SCY: {:#04X}", self.scy);
        println!("SCX: {:#04X}", self.scx);
        println!("LY: {:#04X}", self.ly);
        println!("LYC: {:#04X}", self.lyc);
        println!("BGP: {:#04X}", self.bgp);
        println!("OBP0: {:#04X}", self.obp0);
        println!("OBP1: {:#04X}", self.obp1);
        println!("WY: {:#04X}", self.wy);
        println!("WX: {:#04X}", self.wx);
        println!("Window Line: {:#04X}", self.window_line);
        println!("MODE: {:?}", self.mode);
        println!("MODE CYCLES: {}", self.mode_cycles);
    }

	// Update the PPU for the specified number of cycles
    pub fn update_cycle(&mut self) -> Option<InterruptType> {
        // Skip if LCD is off
        if self.lcdc & 0x80 == 0 {
            return None;
        }
        
        // Store old state for edge detection
        let old_mode = self.mode;
        let old_ly = self.ly;
        
        // Initialize interrupt to None
        let mut interrupt = None;
        
        // Add cycles to mode counter
        self.mode_cycles += 1;
        
        // PPU state machine
        match self.mode {
            LcdMode::OamScan => { // Mode 2
                // OAM scan mode - OAM locked, VRAM accessible
                self.oam_accessible = false;
                self.vram_accessible = true;
                
                // Check WY condition at the start of Mode 2 (OAM Scan)
                if self.ly == self.wy && (self.lcdc & 0x20) != 0 {
                    self.wy_triggered = true;
                    self.last_frame_window_active = true;
                }
                
                // Mode 2 (OAM scan) takes 80 cycles
                if self.mode_cycles >= 80 {
                    // Move to Mode 3 (Drawing)
                    self.mode = LcdMode::Drawing;
                    self.mode_cycles -= 80;
                    self.vram_accessible = false;
                    
                    // Prepare sprites for this scanline
                    self.prepare_sprites_for_scanline();
                }
            },
            
            LcdMode::Drawing => { // Mode 3
                // Drawing mode - both OAM and VRAM locked
                self.oam_accessible = false;
                self.vram_accessible = false;
                
                // Calculate Mode 3 length based on sprites
                let sprite_penalty = (self.scanline_sprites.len() as u32 * 6).min(60);
                let drawing_time = 172 + sprite_penalty;
                
                if self.mode_cycles >= drawing_time {
                    // Move to Mode 0 (HBlank)
                    self.mode = LcdMode::HBlank;
                    self.mode_cycles -= drawing_time;
                    self.vram_accessible = true;
                    self.oam_accessible = true;
                    
                    // Render this scanline
                    self.render_scanline();
                    
                    // Update window line counter after rendering
                    if self.wy_triggered && self.ly >= self.wy {
                        self.window_line = self.window_line.wrapping_add(1);
                    }
                }
            },
            
            LcdMode::HBlank => { // Mode 0
                // HBlank mode - both OAM and VRAM accessible
                self.oam_accessible = true;
                self.vram_accessible = true;
                
                // Calculate HBlank duration
                let sprite_penalty = (self.scanline_sprites.len() as u32 * 6).min(60);
                let hblank_time = 456 - (80 + 172 + sprite_penalty);
                
                if self.mode_cycles >= hblank_time {
                    self.mode_cycles -= hblank_time;
                    
                    // Increment LY
                    self.ly = (self.ly + 1) % 154;
                    
                    // Check window activation on LY change
                    if self.ly == self.wy && (self.lcdc & 0x20) != 0 {
                        self.wy_triggered = true;
                        self.last_frame_window_active = true;
                    }
                    
                    // Check if we've reached the end of visible screen
                    if self.ly == 144 {
                        // Enter VBlank (Mode 1)
                        self.mode = LcdMode::VBlank;
                        self.frame_ready = true;
                        
                        // VBlank interrupt is always generated
                        interrupt = Some(InterruptType::VBlank);
                    } else {
                        // Start next scanline with OAM scan (Mode 2)
                        self.mode = LcdMode::OamScan;
                    }
                }
            },
            
            LcdMode::VBlank => { // Mode 1
                // VBlank mode - both OAM and VRAM accessible
                self.oam_accessible = true;
                self.vram_accessible = true;
                
                // Each scanline in VBlank still takes 456 cycles
                if self.mode_cycles >= 456 {
                    self.mode_cycles -= 456;
                    
                    // Increment LY
                    self.ly = (self.ly + 1) % 154;
                    
                    // Check for end of VBlank
                    if self.ly == 0 {
                        // Always reset window line counter at frame start
                        self.window_line = 0;
                        self.last_frame_window_active = false;
                        self.wy_triggered = false;
                        
                        // Start new frame with OAM scan (Mode 2)
                        self.mode = LcdMode::OamScan;
                    }
                }
            },
        }
        
        // Update STAT register with current mode
        let mode_bits = self.mode as u8;
        self.stat = (self.stat & 0xFC) | (mode_bits & 0x3);
        
        interrupt
    }

    // Prepare sprites for the current scanline (OAM scan)
    fn prepare_sprites_for_scanline(&mut self) {
        self.scanline_sprites.clear();
        
        // If objects are disabled, don't collect any sprites
        if self.lcdc & 0x02 == 0 {
            return;
        }
        
        // Determine sprite size based on LCDC bit 2
        let sprite_size = if self.lcdc & 0x04 != 0 { 16 } else { 8 };
        
        // First pass: collect all sprites visible on this scanline
        for (idx, sprite) in self.oam_entries.iter().enumerate() {
            // Check if sprite is on the current scanline
            if sprite.is_on_scanline(self.ly, sprite_size) {
                // Include all sprites in range, even those with X=0
                // They count toward the 10 sprite limit even if not rendered
                self.scanline_sprites.push((idx, *sprite));
            }
        }
        
        // Sort sprites according to DMG priority rules:
        // 1. Lower X-coordinate has higher priority
        // 2. If X-coordinates are equal, lower OAM index has higher priority
        self.scanline_sprites.sort_by(|(idx_a, sprite_a), (idx_b, sprite_b)| {
            sprite_a.x_pos.cmp(&sprite_b.x_pos)
                .then_with(|| idx_a.cmp(idx_b))
        });
        
        // Limit to 10 sprites per scanline (DMG hardware limitation)
        if self.scanline_sprites.len() > 10 {
            self.scanline_sprites.truncate(10);
        }
        
        // Reverse the array so we can process from highest priority to lowest
        // This makes the rendering code cleaner as earlier sprites overwrite later ones
        self.scanline_sprites.reverse();
    }

	// Render a single scanline to the frame buffer
    fn render_scanline(&mut self) {
        // Only render if LCD is enabled
        if self.lcdc & 0x80 == 0 {
            return;
        }
        
        // Create a scanline buffer for priority handling
        let mut scanline_buffer = [(0u8, false); SCREEN_WIDTH];
        
        // Background
        if self.lcdc & 0x01 != 0 { // BG enabled
            self.render_background(&mut scanline_buffer);
        } else {
            // If background is disabled, fill with color 0
            for x in 0..SCREEN_WIDTH {
                scanline_buffer[x] = (0, false);
            }
        }
        
        // Window
        /*if self.lcdc & 0x20 != 0 { // Window enabled
            self.render_window(&mut scanline_buffer);
        }*/

        if self.lcdc & 0x20 != 0 && self.last_frame_window_active { // Window enabled
            self.render_window(&mut scanline_buffer);
        }
        
        // Sprites
        if self.lcdc & 0x02 != 0 { // Sprites enabled
            self.render_sprites(&mut scanline_buffer);
        }
        
        // Now transfer scanline buffer to frame buffer
        self.finalize_scanline(&scanline_buffer);
    }

	// Render the background for the current scanline
    fn render_background(&mut self, scanline_buffer: &mut [(u8, bool)]) {
        // Get tile map address based on LCDC bit 3
        let tile_map_addr = if self.lcdc & 0x08 != 0 { 0x9C00 } else { 0x9800 };
        
        // Get tile data address based on LCDC bit 4
        let tile_data_signed = self.lcdc & 0x10 == 0;
        let tile_data_addr = if !tile_data_signed { 0x8000 } else { 0x8800 };
        
        // Calculate y position within background
        let y_pos = (self.ly.wrapping_add(self.scy)) & 0xFF;
        
        // Calculate which tile row we're on
        let tile_row = (y_pos / 8) as u16;
        
        // Calculate which pixel row within the tile
        let tile_y = (y_pos % 8) as u16;
        
        // For each pixel in the scanline
        for x in 0..SCREEN_WIDTH {
            // Calculate x position within background
            let x_pos = (x as u8).wrapping_add(self.scx);
            
            // Calculate which tile column we're on
            let tile_col = (x_pos / 8) as u16;
            
            // Calculate which pixel column within the tile
            let tile_x = (x_pos % 8) as u16;
            
            // Calculate tile index address in the tile map
            let tile_map_index = tile_map_addr + tile_row * 32 + tile_col;
            
            // Get the tile index from the tile map
            let tile_index = self.read_vram(tile_map_index);
            
            // Calculate tile data address
            let tile_data_index = if !tile_data_signed {
                tile_data_addr + (tile_index as u16) * 16
            } else {
                tile_data_addr + ((tile_index as i8 as i16 + 128) as u16) * 16
            };
            
            // Read the two bytes of tile data for this row
            let tile_data_low = self.read_vram(tile_data_index + tile_y * 2);
            let tile_data_high = self.read_vram(tile_data_index + tile_y * 2 + 1);
            
            // Calculate the bit position within the tile data
            let bit_pos = 7 - tile_x;
            
            // Get the pixel color (2 bits, one from each byte)
            let color_bit_low = (tile_data_low >> bit_pos) & 0x01;
            let color_bit_high = (tile_data_high >> bit_pos) & 0x01;
            let color_idx = (color_bit_high << 1) | color_bit_low;
            
            // Map to real color from the palette
            let color = self.get_color(color_idx, self.bgp);
            
            // Store in the scanline buffer - mark as non-zero if color_idx > 0
            scanline_buffer[x] = (color, color_idx > 0);
        }
    }
    
    // Render the window for the current scanline
    /*fn render_window(&mut self, scanline_buffer: &mut [(u8, bool)]) {
        // Check if window is disabled by LCDC bit 5
        if self.lcdc & 0x20 == 0 {
            return;
        }
        
        // In DMG mode, window is also disabled if BG is disabled (LCDC bit 0)
        if self.lcdc & 0x01 == 0 {
            return;
        }
        
        // Check if WY condition was triggered for this frame
        if !self.wy_triggered {
            return;
        }
        
        // Check if window X position is valid
        // WX=7 puts the window at the left edge of the screen
        // WX>=167 means window is not visible on this scanline
        if self.wx > 166 {
            return;
        }
        
        // Get window tile map address based on LCDC bit 6
        let tile_map_addr = if self.lcdc & 0x40 != 0 { 0x9C00 } else { 0x9800 };
        
        // Get tile data address based on LCDC bit 4
        let tile_data_signed = self.lcdc & 0x10 == 0;
        let tile_data_addr = if !tile_data_signed { 0x8000 } else { 0x8800 };
        
        // Use internal window line counter
        let window_y = self.window_line;
        
        // Calculate which tile row we're on
        let tile_row = (window_y / 8) as u16;
        
        // Calculate which pixel row within the tile
        let tile_y = (window_y % 8) as u16;
        
        // Flag to track if we actually rendered any window pixels
        let mut rendered = false;
        
        // For each pixel in the scanline
        for x in 0..SCREEN_WIDTH {
            // Skip pixels that are before the window's X position
            // WX-7 is the actual starting X position on the screen
            let wx_adjusted = if self.wx < 7 { 0 } else { self.wx - 7 };
            if (x as u8) < wx_adjusted {
                continue;
            }
            
            rendered = true;
            
            // Calculate X position within window
            let window_x = (x as u8).wrapping_sub(wx_adjusted);
            
            // Calculate which tile column we're on
            let tile_col = (window_x / 8) as u16;
            
            // Calculate which pixel column within the tile
            let tile_x = (window_x % 8) as u16;
            
            // Calculate tile index address in the tile map
            let tile_map_index = tile_map_addr + tile_row * 32 + tile_col;
            
            // Get the tile index from the tile map
            let tile_index = self.read_vram(tile_map_index);
            
            // Calculate tile data address
            let tile_data_index = if !tile_data_signed {
                tile_data_addr + (tile_index as u16) * 16
            } else {
                // $8800 addressing uses signed tile indices
                tile_data_addr + ((tile_index as i8 as i16 + 128) as u16) * 16
            };
            
            // Read the two bytes of tile data for this row
            let tile_data_low = self.read_vram(tile_data_index + tile_y * 2);
            let tile_data_high = self.read_vram(tile_data_index + tile_y * 2 + 1);
            
            // Calculate the bit position within the tile data
            let bit_pos = 7 - tile_x;
            
            // Get the pixel color (2 bits, one from each byte)
            let color_bit_low = (tile_data_low >> bit_pos) & 0x01;
            let color_bit_high = (tile_data_high >> bit_pos) & 0x01;
            let color_idx = (color_bit_high << 1) | color_bit_low;
            
            // Map to real color from the palette
            let color = self.get_color(color_idx, self.bgp);
            
            // Store in the scanline buffer
            scanline_buffer[x] = (color, color_idx > 0);
        }
        
        // Only increment window line counter if we actually rendered any window pixels
        if rendered {
            self.window_line += 1;
            //self.last_frame_window_active = true;
        }
    }*/

    fn render_window(&mut self, scanline_buffer: &mut [(u8, bool)]) {
        // Should we be checkin wy or wx ?
        if self.lcdc & 0x20 == 0 || self.wy > 143 || !self.wy_triggered {
            return;
        }

        let wx_adj = self.wx.saturating_sub(7);
        let tile_map_addr = if self.lcdc & 0x40 != 0 { 0x9C00 } else { 0x9800 };
        let signed_tiles = (self.lcdc & 0x10) == 0;

        let window_y = self.window_line;
        let tile_row = (window_y / 8) as u16;
        let tile_y = (window_y % 8) as u16;

        for pixel_x in 0..SCREEN_WIDTH {
            let wx_start = wx_adj as i16;
            let x_start = wx_start.clamp(0, 159) as usize;
            if pixel_x < x_start {
                continue;
            }

            let window_x = (pixel_x - x_start) as u16;
            let tile_col = (window_x / 8) as u16;
            let tile_x = (window_x % 8) as u16;
            
            let tile_map_index = tile_map_addr + tile_row * 32 + tile_col;
            let tile_index = self.read_vram(tile_map_index);
            let tile_addr = if signed_tiles {
                0x9000u16.wrapping_add((tile_index as i8 as i16 * 16) as u16)
            } else {
                0x8000u16 + (tile_index as u16 * 16)
            };
            
            let addr = tile_addr + tile_y * 2;
            let byte1 = self.read_vram(addr);
            let byte2 = self.read_vram(addr + 1);
            
            let bit_index = 7 - tile_x;
            let color_bit_low = (byte1 >> bit_index) & 0x01;
            let color_bit_high = (byte2 >> bit_index) & 0x01;
            let color_idx = (color_bit_high << 1) | color_bit_low;
            let color = self.get_color(color_idx, self.bgp);
            
            scanline_buffer[pixel_x] = (color, color_idx > 0);
        }
        self.last_frame_window_active = true;
    }
    
    // Render the sprites for the current scanline
    fn render_sprites(&mut self, scanline_buffer: &mut [(u8, bool)]) {
        // Skip sprite rendering entirely if sprites are disabled
        if self.lcdc & 0x02 == 0 {
            return;
        }
    
        // Get sprite size (8x8 or 8x16)
        let sprite_size = if self.lcdc & 0x04 != 0 { 16 } else { 8 };
        
        // Process the sprites that were found during OAM scan
        // Important: DMG renders sprites from lowest X-coordinate to highest
        // with OAM index as tie-breaker, so we should process in reverse order
        // since our prepare_sprites_for_scanline sorts by X and then OAM index
        for &(_, sprite) in self.scanline_sprites.iter() {
            let sprite_y = sprite.y_pos.wrapping_sub(16);
            let sprite_x = sprite.x_pos.wrapping_sub(8);
            
            // Skip sprites with X=0 (these count toward the 10 sprite limit but aren't rendered)
            if sprite.x_pos == 0 {
                continue;
            }
            
            // Get sprite attributes
            let priority = sprite.has_priority(); // OBJ-to-BG Priority (bit 7)
            let flip_y = sprite.is_y_flipped();
            let flip_x = sprite.is_x_flipped();
            let palette = if sprite.palette() == 1 { self.obp1 } else { self.obp0 };
            
            // Calculate the correct tile index for the sprite
            let mut tile_idx = sprite.tile_idx as u16;
            
            // For 8x16 sprites, bit 0 of the tile index is ignored
            if sprite_size == 16 {
                tile_idx &= 0xFE; // Clear bit 0
            }
            
            // Calculate the y offset within the sprite
            let mut y_offset = (self.ly - sprite_y) as u16;
            if flip_y {
                y_offset = (sprite_size as u16) - 1 - y_offset;
            }
            
            // For 8x16 sprites, determine if we're in the bottom tile
            if sprite_size == 16 && y_offset >= 8 {
                tile_idx += 1; // Use next tile for bottom half
                y_offset -= 8; // Adjust offset for the second tile
            }
            
            // Calculate the tile data address (sprites always use $8000 addressing mode)
            let tile_data_addr = 0x8000 + tile_idx * 16 + y_offset * 2;
            
            // Read the two bytes of tile data for this row
            let tile_data_low = self.read_vram(tile_data_addr);
            let tile_data_high = self.read_vram(tile_data_addr + 1);
            
            // For each pixel in the sprite's width
            for x_offset in 0..8 {
                // Calculate the screen X position
                let screen_x = sprite_x.wrapping_add(x_offset);
                
                // Skip if outside screen bounds
                if screen_x >= SCREEN_WIDTH as u8 {
                    continue;
                }
                
                // Calculate bit position based on flip status
                let bit_pos = if flip_x { x_offset } else { 7 - x_offset };
                
                // Extract color bits from tile data
                let color_bit_low = (tile_data_low >> bit_pos) & 0x01;
                let color_bit_high = (tile_data_high >> bit_pos) & 0x01;
                let color_idx = (color_bit_high << 1) | color_bit_low;
                
                // Color 0 is transparent for sprites - skip this pixel
                if color_idx == 0 {
                    continue;
                }
                
                // Map to actual color using the appropriate palette
                let color = self.get_color(color_idx, palette);
                
                // Get the background pixel color and priority flag
                let x = screen_x as usize;
                let (_, bg_color_nonzero) = scanline_buffer[x];
                
                // Priority rules:
                // 1. If BG color is 0, sprite always shows
                // 2. Otherwise, if sprite priority bit is 0, sprite shows
                // 3. Otherwise, if BG is enabled (LCDC.0) and BG pixel is non-zero, BG shows
                
                if !bg_color_nonzero || !priority {
                    // Either BG is color 0 or sprite has priority over BG
                    scanline_buffer[x] = (color, false);
                } else if self.lcdc & 0x01 == 0 {
                    // Background is disabled, so draw sprite regardless of priority
                    scanline_buffer[x] = (color, false);
                }
                // Otherwise, BG has priority, so keep the background pixel
            }
        }
    }

    // Transfer the scanline buffer to the frame buffer with color mapping
    fn finalize_scanline(&mut self, scanline_buffer: &[(u8, bool)]) {
        let ly = self.ly as usize;
        if ly >= SCREEN_HEIGHT {
            return; // Safety check
        }
        
        for x in 0..SCREEN_WIDTH {
            let (color, _) = scanline_buffer[x];
            let frame_idx = (ly * SCREEN_WIDTH + x) * 4;
            
            // Set RGBA values with a more pleasant green-tinted Game Boy palette
            match color {
                0 => { // Lightest (almost white)
                    self.frame_buffer[frame_idx] = 224;
                    self.frame_buffer[frame_idx + 1] = 248;
                    self.frame_buffer[frame_idx + 2] = 208;
                    self.frame_buffer[frame_idx + 3] = 255;
                },
                1 => { // Light green
                    self.frame_buffer[frame_idx] = 136;
                    self.frame_buffer[frame_idx + 1] = 192;
                    self.frame_buffer[frame_idx + 2] = 112;
                    self.frame_buffer[frame_idx + 3] = 255;
                },
                2 => { // Dark green
                    self.frame_buffer[frame_idx] = 52;
                    self.frame_buffer[frame_idx + 1] = 104;
                    self.frame_buffer[frame_idx + 2] = 86;
                    self.frame_buffer[frame_idx + 3] = 255;
                },
                3 => { // Darkest (almost black)
                    self.frame_buffer[frame_idx] = 8;
                    self.frame_buffer[frame_idx + 1] = 24;
                    self.frame_buffer[frame_idx + 2] = 32;
                    self.frame_buffer[frame_idx + 3] = 255;
                },
                _ => unreachable!(),
            }
        }
    }
    
    // Get a color from a palette
    fn get_color(&self, color_idx: u8, palette: u8) -> u8 {
        let idx = 2 * color_idx;
        let palette_color = (palette >> idx) & 0x03;
        palette_color
    }
}