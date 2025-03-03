use crate::ppu::Ppu;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::render::{Canvas, Texture, TextureCreator};
use sdl2::video::{Window, WindowContext};
use std::time::Duration;

// Constants for viewer layout
const TILE_WIDTH: u32 = 8;
const TILE_HEIGHT: u32 = 8;
const TILE_DISPLAY_SCALE: u32 = 2; // Scale tiles by this factor
const GRID_WIDTH: u32 = 16; // Number of tiles per row in tile viewer
const BG_MAP_WIDTH: u32 = 32; // Width of BG map in tiles
const BG_MAP_HEIGHT: u32 = 32; // Height of BG map in tiles
const PADDING: u32 = 1; // Padding between tiles
const SIDEBAR_WIDTH: u32 = 180; // Width of sidebar with info

// Tabs in the viewer
#[derive(PartialEq, Clone, Copy)]
enum ViewerTab {
    BgMap,
    Tiles,
    Oam,
    Palettes,
}

// Options for the viewer
struct ViewerOptions {
    show_grid: bool,
    show_palettes: bool,
    selected_palette: u8, // For CGB mode
    selected_bank: u8,    // For CGB mode
    tile_offset: u16,     // For scrolling through tiles
    bg_map_offset: u16,   // 0x9800 or 0x9C00
    current_tab: ViewerTab,
}

pub struct VramViewer {
    canvas: Canvas<Window>,
    texture_creator: TextureCreator<WindowContext>,
    options: ViewerOptions,
    is_open: bool,
}

impl VramViewer {
    pub fn new(sdl_context: &sdl2::Sdl) -> Result<Self, String> {
        let video_subsystem = sdl_context.video()?;
        
        // Calculate window dimensions based on largest view (BG map)
        let window_width = BG_MAP_WIDTH * TILE_WIDTH * TILE_DISPLAY_SCALE + PADDING * (BG_MAP_WIDTH - 1) + SIDEBAR_WIDTH;
        let window_height = BG_MAP_HEIGHT * TILE_HEIGHT * TILE_DISPLAY_SCALE + PADDING * (BG_MAP_HEIGHT - 1);
        
        let window = video_subsystem
            .window("VRAM viewer", window_width, window_height)
            .position_centered()
            .hidden() // Start hidden
            .build()
            .map_err(|e| e.to_string())?;
        
        let canvas = window.into_canvas().build().map_err(|e| e.to_string())?;
        let texture_creator = canvas.texture_creator();
        
        let options = ViewerOptions {
            show_grid: true,
            show_palettes: true,
            selected_palette: 0,
            selected_bank: 0,
            tile_offset: 0,
            bg_map_offset: 0x9800,
            current_tab: ViewerTab::BgMap,
        };
        
        Ok(VramViewer {
            canvas,
            texture_creator,
            options,
            is_open: false,
        })
    }

    pub fn toggle(&mut self) {
        self.is_open = !self.is_open;
        if self.is_open {
            self.canvas.window_mut().show();
        } else {
            self.canvas.window_mut().hide();
        }
    }
    
    pub fn is_open(&self) -> bool {
        self.is_open
    }
    
    pub fn handle_event(&mut self, event: &Event) -> bool {
        if !self.is_open {
            return false;
        }
        
        match event {
            Event::KeyDown { keycode: Some(Keycode::Tab), .. } => {
                // Cycle through tabs
                self.options.current_tab = match self.options.current_tab {
                    ViewerTab::BgMap => ViewerTab::Tiles,
                    ViewerTab::Tiles => ViewerTab::Oam,
                    ViewerTab::Oam => ViewerTab::Palettes,
                    ViewerTab::Palettes => ViewerTab::BgMap,
                };
                true
            },
            Event::KeyDown { keycode: Some(Keycode::G), .. } => {
                // Toggle grid
                self.options.show_grid = !self.options.show_grid;
                true
            },
            Event::KeyDown { keycode: Some(Keycode::P), .. } => {
                // Toggle palettes
                self.options.show_palettes = !self.options.show_palettes;
                true
            },
            Event::KeyDown { keycode: Some(Keycode::M), .. } => {
                // Toggle background map (0x9800 or 0x9C00)
                self.options.bg_map_offset = if self.options.bg_map_offset == 0x9800 { 0x9C00 } else { 0x9800 };
                true
            },
            Event::Window { win_event: sdl2::event::WindowEvent::Close, .. } => {
                self.toggle();
                true
            },
            _ => false,
        }
    }
    
    pub fn update(&mut self, ppu: &Ppu) -> Result<(), String> {
        if !self.is_open {
            return Ok(());
        }
        
        // Clear the canvas
        self.canvas.set_draw_color(Color::RGB(240, 240, 240));
        self.canvas.clear();
        
        // Render the current view
        match self.options.current_tab {
            ViewerTab::BgMap => self.render_bg_map(ppu)?,
            ViewerTab::Tiles => self.render_tiles(ppu)?,
            ViewerTab::Oam => self.render_oam(ppu)?,
            ViewerTab::Palettes => self.render_palettes(ppu)?,
        }
        
        // Render tab buttons
        self.render_tabs()?;
        
        // Render sidebar info
        self.render_sidebar(ppu)?;
        
        // Present the canvas
        self.canvas.present();
        
        Ok(())
    }
    
    fn render_tabs(&mut self) -> Result<(), String> {
        let tabs = ["BG map", "Tiles", "OAM", "Palettes"];
        let tab_width = 80;
        let tab_height = 25;
        let tab_padding = 5;
        
        for (i, &tab_name) in tabs.iter().enumerate() {
            let selected = match i {
                0 => self.options.current_tab == ViewerTab::BgMap,
                1 => self.options.current_tab == ViewerTab::Tiles,
                2 => self.options.current_tab == ViewerTab::Oam,
                3 => self.options.current_tab == ViewerTab::Palettes,
                _ => false,
            };
            
            // Draw tab background
            self.canvas.set_draw_color(if selected { 
                Color::RGB(200, 240, 200) 
            } else { 
                Color::RGB(180, 180, 180) 
            });
            
            let tab_rect = Rect::new(
                (i as i32) * (tab_width as i32 + tab_padding), 
                0, 
                tab_width, 
                tab_height
            );
            self.canvas.fill_rect(tab_rect)?;
            
            // Draw tab border
            self.canvas.set_draw_color(Color::RGB(100, 100, 100));
            self.canvas.draw_rect(tab_rect)?;
            
            // Draw tab label
            let text_x = (i as i32) * (tab_width as i32 + tab_padding) + 10;
            let text_y = 9; // Centered vertically in the tab
            self.draw_text(
                tab_name, 
                text_x, 
                text_y, 
                Color::RGB(0, 0, 0)
            )?;
        }
        
        // Draw separator line below tabs
        self.canvas.set_draw_color(Color::RGB(100, 100, 100));
        let separator_y = tab_height as i32;
        let window_width = self.canvas.window().size().0 as i32;
        self.canvas.draw_line((0, separator_y), (window_width, separator_y))?;
        
        Ok(())
    }
    
    fn render_sidebar(&mut self, ppu: &Ppu) -> Result<(), String> {
        // Draw sidebar background
        self.canvas.set_draw_color(Color::RGB(200, 200, 200));
        let sidebar_x = self.canvas.window().size().0 as i32 - SIDEBAR_WIDTH as i32;
        let sidebar_rect = Rect::new(sidebar_x, 30, SIDEBAR_WIDTH, self.canvas.window().size().1 - 30);
        self.canvas.fill_rect(sidebar_rect)?;
        
        // Draw sidebar title
        self.draw_text("Options", sidebar_x + 10, 40, Color::RGB(0, 0, 0))?;
        
        // Draw checkboxes for options
        self.canvas.set_draw_color(Color::RGB(255, 255, 255));
        let checkbox_size = 15;
        let checkbox_x = sidebar_x + 10;
        let mut checkbox_y = 60;
        
        // Grid checkbox
        let grid_checkbox = Rect::new(checkbox_x, checkbox_y, checkbox_size, checkbox_size);
        self.canvas.fill_rect(grid_checkbox)?;
        if self.options.show_grid {
            self.canvas.set_draw_color(Color::RGB(0, 0, 0));
            self.canvas.draw_line(
                (checkbox_x, checkbox_y), 
                (checkbox_x + checkbox_size as i32, checkbox_y + checkbox_size as i32)
            )?;
            self.canvas.draw_line(
                (checkbox_x + checkbox_size as i32, checkbox_y), 
                (checkbox_x, checkbox_y + checkbox_size as i32)
            )?;
        }
        self.draw_text("Show Grid", checkbox_x + checkbox_size as i32 + 5, checkbox_y + 4, Color::RGB(0, 0, 0))?;
        
        // Palette checkbox
        checkbox_y += 25;
        self.canvas.set_draw_color(Color::RGB(255, 255, 255));
        let palette_checkbox = Rect::new(checkbox_x, checkbox_y, checkbox_size, checkbox_size);
        self.canvas.fill_rect(palette_checkbox)?;
        if self.options.show_palettes {
            self.canvas.set_draw_color(Color::RGB(0, 0, 0));
            self.canvas.draw_line(
                (checkbox_x, checkbox_y), 
                (checkbox_x + checkbox_size as i32, checkbox_y + checkbox_size as i32)
            )?;
            self.canvas.draw_line(
                (checkbox_x + checkbox_size as i32, checkbox_y), 
                (checkbox_x, checkbox_y + checkbox_size as i32)
            )?;
        }
        self.draw_text("Show Palettes", checkbox_x + checkbox_size as i32 + 5, checkbox_y + 4, Color::RGB(0, 0, 0))?;
        
        // Display current info based on tab
        checkbox_y += 50;
        match self.options.current_tab {
            ViewerTab::BgMap => {
                // Show BG map info
                self.draw_text(&format!("Map: 0x{:04X}", self.options.bg_map_offset), 
                              sidebar_x + 10, checkbox_y, Color::RGB(0, 0, 0))?;
                
                checkbox_y += 20;
                self.draw_text(&format!("LCDC: 0x{:02X}", ppu.lcdc), 
                              sidebar_x + 10, checkbox_y, Color::RGB(0, 0, 0))?;
                
                checkbox_y += 20;
                self.draw_text(&format!("SCY: {}", ppu.scy), 
                              sidebar_x + 10, checkbox_y, Color::RGB(0, 0, 0))?;
                
                checkbox_y += 20;
                self.draw_text(&format!("SCX: {}", ppu.scx), 
                              sidebar_x + 10, checkbox_y, Color::RGB(0, 0, 0))?;
                
                checkbox_y += 20;
                self.draw_text(&format!("WY: {}", ppu.wy), 
                              sidebar_x + 10, checkbox_y, Color::RGB(0, 0, 0))?;
                
                checkbox_y += 20;
                self.draw_text(&format!("WX: {}", ppu.wx), 
                              sidebar_x + 10, checkbox_y, Color::RGB(0, 0, 0))?;
            },
            ViewerTab::Tiles => {
                // Show tile info
                self.draw_text("Tile Information", sidebar_x + 10, checkbox_y, Color::RGB(0, 0, 0))?;
                
                checkbox_y += 20;
                self.draw_text(&format!("Tile mode: {}", 
                                      if ppu.lcdc & 0x10 != 0 { "8000" } else { "8800" }),
                              sidebar_x + 10, checkbox_y, Color::RGB(0, 0, 0))?;
            },
            ViewerTab::Oam => {
                // Show OAM info
                self.draw_text("OAM Information", sidebar_x + 10, checkbox_y, Color::RGB(0, 0, 0))?;
                
                checkbox_y += 20;
                self.draw_text(&format!("Sprite size: {}x{}", 8, 
                                      if ppu.lcdc & 0x04 != 0 { 16 } else { 8 }),
                              sidebar_x + 10, checkbox_y, Color::RGB(0, 0, 0))?;
                
                checkbox_y += 20;
                self.draw_text(&format!("Sprites enabled: {}", 
                                      if ppu.lcdc & 0x02 != 0 { "Yes" } else { "No" }),
                              sidebar_x + 10, checkbox_y, Color::RGB(0, 0, 0))?;
            },
            ViewerTab::Palettes => {
                // Show palette info
                self.draw_text("Palette Information", sidebar_x + 10, checkbox_y, Color::RGB(0, 0, 0))?;
                
                checkbox_y += 20;
                self.draw_text(&format!("BGP: 0x{:02X}", ppu.bgp), 
                              sidebar_x + 10, checkbox_y, Color::RGB(0, 0, 0))?;
                
                checkbox_y += 20;
                self.draw_text(&format!("OBP0: 0x{:02X}", ppu.obp0), 
                              sidebar_x + 10, checkbox_y, Color::RGB(0, 0, 0))?;
                
                checkbox_y += 20;
                self.draw_text(&format!("OBP1: 0x{:02X}", ppu.obp1), 
                              sidebar_x + 10, checkbox_y, Color::RGB(0, 0, 0))?;
            },
        }
        
        Ok(())
    }
    
    fn render_bg_map(&mut self, ppu: &Ppu) -> Result<(), String> {
        // Create a texture to hold the entire map
        let mut texture = self.texture_creator.create_texture_streaming(
            PixelFormatEnum::RGB24, 
            BG_MAP_WIDTH * TILE_WIDTH, 
            BG_MAP_HEIGHT * TILE_HEIGHT
        ).unwrap();
        
        // Update the texture with the BG map data
        texture.with_lock(None, |buffer: &mut [u8], pitch: usize| {
            for y in 0..BG_MAP_HEIGHT {
                for x in 0..BG_MAP_WIDTH {
                    // Calculate map address and fetch tile index
                    let map_addr = self.options.bg_map_offset + y as u16 * 32 as u16 + x as u16;
                    let tile_index = ppu.read_vram(map_addr as u16);
                    
                    // Get tile data address (adjust based on LCDC bit 4)
                    let tile_data_addr = if ppu.lcdc & 0x10 != 0 {
                        // $8000 addressing mode
                        0x8000 + (tile_index as u16) * 16
                    } else {
                        // $8800 addressing mode
                        0x9000 + (tile_index as u16) * 16
                    };
                    
                    // Draw the tile
                    self.draw_tile(buffer, pitch, tile_data_addr, x as u32 * TILE_WIDTH, y as u32 * TILE_HEIGHT, ppu);
                }
            }
        })?;
        
        // Draw the texture to the canvas, scaled up
        let dest_rect = Rect::new(
            0, 
            30, // Start below the tabs
            BG_MAP_WIDTH * TILE_WIDTH * TILE_DISPLAY_SCALE, 
            BG_MAP_HEIGHT * TILE_HEIGHT * TILE_DISPLAY_SCALE
        );
        self.canvas.copy(&texture, None, dest_rect)?;
        
        // Draw grid if enabled
        if self.options.show_grid {
            self.canvas.set_draw_color(Color::RGB(100, 100, 100));
            
            // Draw vertical grid lines
            for x in 0..BG_MAP_WIDTH {
                let x_pos = (x * TILE_WIDTH * TILE_DISPLAY_SCALE) as i32;
                self.canvas.draw_line(
                    (x_pos, 30), 
                    (x_pos, 30 + (BG_MAP_HEIGHT * TILE_HEIGHT * TILE_DISPLAY_SCALE) as i32)
                )?;
            }
            
            // Draw horizontal grid lines
            for y in 0..BG_MAP_HEIGHT {
                let y_pos = 30 + (y * TILE_HEIGHT * TILE_DISPLAY_SCALE) as i32;
                self.canvas.draw_line(
                    (0, y_pos), 
                    ((BG_MAP_WIDTH * TILE_WIDTH * TILE_DISPLAY_SCALE) as i32, y_pos)
                )?;
            }
        }

        Ok(())
    }
    
    fn render_tiles(&mut self, ppu: &Ppu) -> Result<(), String> {
        // Calculate number of tiles to display and create texture
        let num_tiles = 384; // 384 tiles total (half in each bank)
        let rows = (num_tiles + GRID_WIDTH as usize - 1) / GRID_WIDTH as usize;
        
        let mut texture = self.texture_creator.create_texture_streaming(
            PixelFormatEnum::RGB24,
            GRID_WIDTH * TILE_WIDTH,
            rows as u32 * TILE_HEIGHT
        ).unwrap();
        
        // Update the texture with the tile data
        texture.with_lock(None, |buffer: &mut [u8], pitch: usize| {
            for tile_idx in 0..num_tiles {
                let tile_x = (tile_idx % GRID_WIDTH as usize) as u32;
                let tile_y = (tile_idx / GRID_WIDTH as usize) as u32;
                
                // Calculate tile address (0x8000-0x97FF)
                let tile_addr = 0x8000 + (tile_idx as u16) * 16;
                
                // Draw the tile
                self.draw_tile(
                    buffer,
                    pitch,
                    tile_addr,
                    tile_x * TILE_WIDTH,
                    tile_y * TILE_HEIGHT,
                    ppu
                );
            }
        })?;
        
        // Draw the texture to the canvas, scaled up
        let dest_rect = Rect::new(
            0,
            30, // Start below the tabs
            GRID_WIDTH * TILE_WIDTH * TILE_DISPLAY_SCALE,
            rows as u32 * TILE_HEIGHT * TILE_DISPLAY_SCALE
        );
        self.canvas.copy(&texture, None, dest_rect)?;
        
        // Draw grid if enabled
        if self.options.show_grid {
            self.canvas.set_draw_color(Color::RGB(100, 100, 100));
            
            // Draw vertical grid lines
            for x in 0..=GRID_WIDTH {
                let x_pos = (x * TILE_WIDTH * TILE_DISPLAY_SCALE) as i32;
                self.canvas.draw_line(
                    (x_pos, 30),
                    (x_pos, 30 + (rows as u32 * TILE_HEIGHT * TILE_DISPLAY_SCALE) as i32)
                )?;
            }
            
            // Draw horizontal grid lines
            for y in 0..=rows as u32 {
                let y_pos = 30 + (y * TILE_HEIGHT * TILE_DISPLAY_SCALE) as i32;
                self.canvas.draw_line(
                    (0, y_pos),
                    ((GRID_WIDTH * TILE_WIDTH * TILE_DISPLAY_SCALE) as i32, y_pos)
                )?;
            }
        }

        Ok(())
    }
    
    fn render_oam(&mut self, ppu: &Ppu) -> Result<(), String> {
        // Create a texture for OAM viewer
        let mut texture = self.texture_creator.create_texture_streaming(
            PixelFormatEnum::RGB24,
            10 * TILE_WIDTH, // 10 sprites per row
            4 * TILE_HEIGHT  // 40 sprites total, 4 rows
        ).unwrap();
        
        // Get sprite size from LCDC bit 2
        let sprite_size = if ppu.lcdc & 0x04 != 0 { 16 } else { 8 };
        
        // Update the texture with the OAM data
        texture.with_lock(None, |buffer: &mut [u8], pitch: usize| {
            for i in 0..40 {
                // Calculate sprite position in the grid
                let grid_x = (i % 10) as u32;
                let grid_y = (i / 10) as u32;
                
                // Get sprite attributes
                let sprite = &ppu.oam_entries[i];
                
                // Calculate tile address
                let tile_addr = 0x8000 + (sprite.tile_idx as u16) * 16;
                
                // Draw the sprite tile
                self.draw_tile(
                    buffer,
                    pitch,
                    tile_addr,
                    grid_x * TILE_WIDTH,
                    grid_y * TILE_HEIGHT,
                    ppu
                );
                
                // Draw the second tile for 8x16 sprites
                if sprite_size == 16 {
                    let next_tile_addr = 0x8000 + ((sprite.tile_idx & 0xFE) as u16 + 1) * 16;
                    self.draw_tile(
                        buffer,
                        pitch,
                        next_tile_addr,
                        grid_x * TILE_WIDTH,
                        grid_y * TILE_HEIGHT + 8,
                        ppu
                    );
                }
            }
        })?;
        
        // Draw the texture to the canvas, scaled up
        let dest_rect = Rect::new(
            0,
            30, // Start below the tabs
            10 * TILE_WIDTH * TILE_DISPLAY_SCALE,
            4 * TILE_HEIGHT * TILE_DISPLAY_SCALE
        );
        self.canvas.copy(&texture, None, dest_rect)?;
        
        // Draw grid if enabled
        if self.options.show_grid {
            self.canvas.set_draw_color(Color::RGB(100, 100, 100));
            
            // Draw vertical grid lines
            for x in 0..=10 {
                let x_pos = (x * TILE_WIDTH * TILE_DISPLAY_SCALE) as i32;
                self.canvas.draw_line(
                    (x_pos, 30),
                    (x_pos, 30 + (4 * TILE_HEIGHT * TILE_DISPLAY_SCALE) as i32)
                )?;
            }
            
            // Draw horizontal grid lines
            for y in 0..=4 {
                let y_pos = 30 + (y * TILE_HEIGHT * TILE_DISPLAY_SCALE) as i32;
                self.canvas.draw_line(
                    (0, y_pos),
                    ((10 * TILE_WIDTH * TILE_DISPLAY_SCALE) as i32, y_pos)
                )?;
            }
        }

        Ok(())
    }
    
    fn render_palettes(&mut self, ppu: &Ppu) -> Result<(), String> {
        // Draw DMG palettes (BGP, OBP0, OBP1)
        let palette_width = 100;
        let palette_height = 20;
        let palette_spacing = 30;
        let start_y = 50;
        
        // Draw BGP
        self.draw_dmg_palette(ppu.bgp, "BGP", 20, start_y, palette_width, palette_height)?;
        
        // Draw OBP0
        self.draw_dmg_palette(ppu.obp0, "OBP0", 20, start_y + palette_spacing, palette_width, palette_height)?;
        
        // Draw OBP1
        self.draw_dmg_palette(ppu.obp1, "OBP1", 20, start_y + 2 * palette_spacing, palette_width, palette_height)?;
        
        Ok(())
    }
    
    fn draw_dmg_palette(&mut self, palette: u8, name: &str, x: i32, y: i32, width: u32, height: u32) -> Result<(), String> {
        // Calculate the four colors in the palette
        let colors = [
            self.get_dmg_color((palette >> 0) & 0x3),
            self.get_dmg_color((palette >> 2) & 0x3),
            self.get_dmg_color((palette >> 4) & 0x3),
            self.get_dmg_color((palette >> 6) & 0x3),
        ];
        
        // Draw each color square
        let square_width = width / 4;
        for i in 0..4 {
            let square_x = x + (i as i32 * square_width as i32);
            let square_rect = Rect::new(square_x, y, square_width, height);
            
            self.canvas.set_draw_color(colors[i as usize]);
            self.canvas.fill_rect(square_rect)?;
            
            self.canvas.set_draw_color(Color::RGB(0, 0, 0));
            self.canvas.draw_rect(square_rect)?;
        }
        
        // TODO: Add text rendering for palette name
        
        Ok(())
    }
    
    fn get_dmg_color(&self, color_idx: u8) -> Color {
        // Convert the DMG color index to an RGB color
        // (using the standard Game Boy greenish palette)
        match color_idx {
            0 => Color::RGB(224, 248, 208), // Lightest
            1 => Color::RGB(136, 192, 112), // Light
            2 => Color::RGB(52, 104, 86),   // Dark
            3 => Color::RGB(8, 24, 32),     // Darkest
            _ => Color::RGB(0, 0, 0),       // Should not happen
        }
    }
    
    fn draw_tile(&self, buffer: &mut [u8], pitch: usize, tile_addr: u16, x: u32, y: u32, ppu: &Ppu) {
        // Draw an 8x8 tile to the buffer
        for row in 0..8 {
            let addr = tile_addr + (row * 2) as u16;
            let low_byte = ppu.read_vram(addr);
            let high_byte = ppu.read_vram(addr + 1);
            
            for col in 0..8 {
                let bit_pos = 7 - col;
                let low_bit = (low_byte >> bit_pos) & 0x01;
                let high_bit = (high_byte >> bit_pos) & 0x01;
                let color_idx = (high_bit << 1) | low_bit;
                
                // Get the palette color
                let palette_color = (ppu.bgp >> (color_idx * 2)) & 0x03;
                
                // Draw the pixel if it's within the buffer boundaries
                let pixel_x = x + col;
                let pixel_y = y + row;
                let offset = (pixel_y as usize * pitch) + (pixel_x as usize * 3);
                
                if offset + 2 < buffer.len() {
                    let color = self.get_dmg_color(palette_color);
                    buffer[offset] = color.r;
                    buffer[offset + 1] = color.g;
                    buffer[offset + 2] = color.b;
                }
            }
        }
    }

    fn draw_text(&mut self, text: &str, x: i32, y: i32, color: Color) -> Result<(), String> {
        // Simple 5x7 bitmap font implementation for Game Boy viewer
        // Each character is represented as a series of bits in a 5x7 grid
        
        // Define a simple font for the basic characters we need
        let font_data: std::collections::HashMap<char, [u8; 7]> = [
            // Each value represents a row of 5 pixels (1=on, 0=off)
            ('A', [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b00000]),
            ('B', [0b11110, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110, 0b00000]),
            ('C', [0b01110, 0b10001, 0b10000, 0b10000, 0b10001, 0b01110, 0b00000]),
            ('D', [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110, 0b00000]),
            ('E', [0b11111, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111, 0b00000]),
            ('F', [0b11111, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000, 0b00000]),
            ('G', [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b01111, 0b00000]),
            ('H', [0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001, 0b00000]),
            ('I', [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110, 0b00000]),
            ('J', [0b00111, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100, 0b00000]),
            ('K', [0b10001, 0b10010, 0b11100, 0b10010, 0b10001, 0b10001, 0b00000]),
            ('L', [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111, 0b00000]),
            ('M', [0b10001, 0b11011, 0b10101, 0b10001, 0b10001, 0b10001, 0b00000]),
            ('N', [0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b00000]),
            ('O', [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110, 0b00000]),
            ('P', [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b00000]),
            ('Q', [0b01110, 0b10001, 0b10001, 0b10001, 0b10011, 0b01111, 0b00000]),
            ('R', [0b11110, 0b10001, 0b10001, 0b11110, 0b10010, 0b10001, 0b00000]),
            ('S', [0b01111, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110, 0b00000]),
            ('T', [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00000]),
            ('U', [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110, 0b00000]),
            ('V', [0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100, 0b00000]),
            ('W', [0b10001, 0b10001, 0b10001, 0b10101, 0b11011, 0b10001, 0b00000]),
            ('X', [0b10001, 0b01010, 0b00100, 0b00100, 0b01010, 0b10001, 0b00000]),
            ('Y', [0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100, 0b00000]),
            ('Z', [0b11111, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111, 0b00000]),
            ('0', [0b01110, 0b10011, 0b10101, 0b10101, 0b11001, 0b01110, 0b00000]),
            ('1', [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b01110, 0b00000]),
            ('2', [0b01110, 0b10001, 0b00010, 0b00100, 0b01000, 0b11111, 0b00000]),
            ('3', [0b01110, 0b10001, 0b00010, 0b00110, 0b10001, 0b01110, 0b00000]),
            ('4', [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00000]),
            ('5', [0b11111, 0b10000, 0b11110, 0b00001, 0b10001, 0b01110, 0b00000]),
            ('6', [0b01110, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110, 0b00000]),
            ('7', [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b00000]),
            ('8', [0b01110, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110, 0b00000]),
            ('9', [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b01110, 0b00000]),
            (':', [0b00000, 0b00100, 0b00000, 0b00000, 0b00100, 0b00000, 0b00000]),
            (' ', [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000]),
            ('.', [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00100, 0b00000]),
            (',', [0b00000, 0b00000, 0b00000, 0b00000, 0b00100, 0b00100, 0b01000]),
            ('(', [0b00010, 0b00100, 0b01000, 0b01000, 0b00100, 0b00010, 0b00000]),
            (')', [0b01000, 0b00100, 0b00010, 0b00010, 0b00100, 0b01000, 0b00000]),
            ('[', [0b01110, 0b01000, 0b01000, 0b01000, 0b01000, 0b01110, 0b00000]),
            (']', [0b01110, 0b00010, 0b00010, 0b00010, 0b00010, 0b01110, 0b00000]),
            ('+', [0b00000, 0b00100, 0b01110, 0b00100, 0b00000, 0b00000, 0b00000]),
            ('-', [0b00000, 0b00000, 0b01110, 0b00000, 0b00000, 0b00000, 0b00000]),
            ('/', [0b00000, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b00000]),
            ('\\', [0b00000, 0b10000, 0b01000, 0b00100, 0b00010, 0b00001, 0b00000]),
            ('=', [0b00000, 0b00000, 0b11111, 0b00000, 0b11111, 0b00000, 0b00000]),
            ('_', [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b11111, 0b00000]),
            ('x', [0b00000, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b00000]),
            ('a', [0b00000, 0b00000, 0b01110, 0b00001, 0b01111, 0b01111, 0b00000]),
            ('b', [0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b11110, 0b00000]),
            ('c', [0b00000, 0b00000, 0b01110, 0b10000, 0b10000, 0b01110, 0b00000]),
            ('d', [0b00001, 0b00001, 0b01111, 0b10001, 0b10001, 0b01111, 0b00000]),
            ('e', [0b00000, 0b00000, 0b01110, 0b10001, 0b11110, 0b01111, 0b00000]),
            ('f', [0b00110, 0b01000, 0b11100, 0b01000, 0b01000, 0b01000, 0b00000]),
            ('g', [0b00000, 0b00000, 0b01111, 0b10001, 0b01111, 0b00001, 0b01110]),
            ('h', [0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b10001, 0b00000]),
            ('i', [0b00100, 0b00000, 0b01100, 0b00100, 0b00100, 0b01110, 0b00000]),
            ('j', [0b00010, 0b00000, 0b00110, 0b00010, 0b00010, 0b10010, 0b01100]),
            ('k', [0b10000, 0b10000, 0b10010, 0b11100, 0b10010, 0b10001, 0b00000]),
            ('l', [0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110, 0b00000]),
            ('m', [0b00000, 0b00000, 0b11010, 0b10101, 0b10101, 0b10001, 0b00000]),
            ('n', [0b00000, 0b00000, 0b11110, 0b10001, 0b10001, 0b10001, 0b00000]),
            ('o', [0b00000, 0b00000, 0b01110, 0b10001, 0b10001, 0b01110, 0b00000]),
            ('p', [0b00000, 0b00000, 0b11110, 0b10001, 0b11110, 0b10000, 0b10000]),
            ('q', [0b00000, 0b00000, 0b01111, 0b10001, 0b01111, 0b00001, 0b00001]),
            ('r', [0b00000, 0b00000, 0b10110, 0b11000, 0b10000, 0b10000, 0b00000]),
            ('s', [0b00000, 0b00000, 0b01111, 0b10000, 0b01110, 0b11110, 0b00000]),
            ('t', [0b01000, 0b01000, 0b11100, 0b01000, 0b01000, 0b00110, 0b00000]),
            ('u', [0b00000, 0b00000, 0b10001, 0b10001, 0b10001, 0b01111, 0b00000]),
            ('v', [0b00000, 0b00000, 0b10001, 0b10001, 0b01010, 0b00100, 0b00000]),
            ('w', [0b00000, 0b00000, 0b10001, 0b10101, 0b10101, 0b01010, 0b00000]),
            ('y', [0b00000, 0b00000, 0b10001, 0b01010, 0b00100, 0b01000, 0b10000]),
            ('z', [0b00000, 0b00000, 0b11111, 0b00010, 0b01100, 0b11111, 0b00000]),
        ].iter().cloned().collect();

        // Set drawing color
        self.canvas.set_draw_color(color);

        // Character dimensions
        let char_width = 6; // 5 pixels + 1 spacing
        let char_height = 8; // 7 pixels + 1 spacing
        
        // Draw each character
        let mut cursor_x = x;
        for c in text.chars() {
            // Convert to uppercase for consistency
            let c_upper = c.to_ascii_uppercase();
            
            // Get the bitmap data for this character (or use space if not found)
            let char_bitmap = font_data.get(&c_upper).unwrap_or(&font_data[&' ']);
            
            // Draw the character pixel by pixel
            for (row, &bitmap_row) in char_bitmap.iter().enumerate() {
                for col in 0..5 {
                    let bit = (bitmap_row >> (4 - col)) & 0x01;
                    if bit == 1 {
                        let pixel_x = cursor_x + col as i32;
                        let pixel_y = y + row as i32;
                        self.canvas.draw_point((pixel_x, pixel_y))?;
                    }
                }
            }
            
            // Move cursor to next character position
            cursor_x += char_width;
        }
        
        Ok(())
    }
}