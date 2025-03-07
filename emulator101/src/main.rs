use std::fs::File;
use std::io::Read;
use std::time::Duration;
use std::time::Instant;
use std::thread::sleep;
use std::env;

use emulator101::ppu::{SCREEN_WIDTH, SCREEN_HEIGHT};
use emulator101::vram_viewer::VramViewer;
use emulator101::emulator::Emulator;

use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;
use sdl2::rect::Rect;

const SCALE: u32 = 3;

fn read_rom(path: &str) -> Result<Vec<u8>, std::io::Error> {
    let mut rom_data = Vec::new();
    let mut file = File::open(path)?;
    file.read_to_end(&mut rom_data)?;
    Ok(rom_data)
}

fn main() -> Result<(), Box<dyn std::error::Error>> 
{
    // Get command line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        println!("Usage: emulator101 [run <rom_path>]");
        return Ok(());
    }
    
    if args[1] == "run" {
        run_emulator(&args[2])?;
    } else {
        println!("Usage: emulator101 [run <rom_path>]");
    }

    Ok(())
}

fn run_emulator(rom_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Load the ROM
    let rom_data = read_rom(rom_path)?;
    
    // Initialize SDL2
    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;
    
    let window = video_subsystem
        .window("Game Boy Emulator", SCREEN_WIDTH as u32 * SCALE, SCREEN_HEIGHT as u32 * SCALE)
        .position_centered()
        .build()?;
    
    let mut canvas = window.into_canvas().build()?;
    let texture_creator = canvas.texture_creator();
    
    let mut texture = texture_creator
        .create_texture_streaming(PixelFormatEnum::RGBA32, SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32)?;
    
    let mut event_pump = sdl_context.event_pump()?;

    // Initialize emulator components
    let mut emulator = Emulator::new(&rom_data);

    // Initialize VRAM viewer
    let mut vram_viewer = VramViewer::new(&sdl_context)?;

    // Timing variables
    let mut last_frame_time = Instant::now();
    let frame_duration = Duration::from_nanos(1_000_000_000 / 60); // Target 60 FPS

    let mut fps_update_timer = Instant::now();
    let mut frames_counted = 0;
    let mut current_fps = 0.0;
    let fps_update_interval = Duration::from_secs(1); // Update FPS display every second
    let mut show_fps = true;
    
    // Main emulation loop
    'running: loop {
        // Handle SDL2 events
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => {
                    break 'running;
                },
                Event::KeyDown { keycode: Some(Keycode::Escape), .. } => {
                    break 'running;
                },
                Event::KeyDown { keycode: Some(Keycode::V), repeat: false, .. } => {
                    vram_viewer.toggle();
                },
                Event::KeyDown { keycode: Some(Keycode::F), repeat: false, .. } => {
                    show_fps = !show_fps;
                    if !show_fps {
                        canvas.window_mut().set_title("Game Boy Emulator")?;
                    }
                },
                _ => {
                    if vram_viewer.is_open() {
                        if vram_viewer.handle_event(&event) {
                            continue; // Event was handled by viewer
                        }
                    }
                    
                    // Handle other events for the main emulator
                    match &event {
                        Event::KeyDown { keycode: Some(key), repeat: false, .. } => {
                            emulator.bus.handle_key_event(*key, true);
                        },
                        Event::KeyUp { keycode: Some(key), repeat: false, .. } => {
                            emulator.bus.handle_key_event(*key, false);
                        },
                        _ => {}
                    }
                }
            }
        }
        
        // Run emulator
        emulator.run_until_frame();

        // Update the texture with the new frame buffer
        texture.update(None, &emulator.bus.ppu.ui_frame_buffer, SCREEN_WIDTH * 4)?;
        
        // Render the frame
        canvas.clear();
        canvas.copy(&texture, None, Some(Rect::new(0, 0, SCREEN_WIDTH as u32 * SCALE, SCREEN_HEIGHT as u32 * SCALE)))?;
        canvas.present();

        if vram_viewer.is_open() {
            vram_viewer.update(&emulator.bus.ppu)?;
        }
        
        // FPS calculation
        frames_counted += 1;
        let now = Instant::now();
        if now.duration_since(fps_update_timer) >= fps_update_interval {
            current_fps = frames_counted as f64 / now.duration_since(fps_update_timer).as_secs_f64();
            frames_counted = 0;
            fps_update_timer = now;
            
            // Update window title with FPS if enabled
            if show_fps {
                canvas.window_mut().set_title(&format!("Game Boy Emulator - FPS: {:.1}", current_fps))?;
            }
        }
        
        // Frame timing for 60 FPS
        let elapsed = now.duration_since(last_frame_time);
        if elapsed < frame_duration {
            sleep(frame_duration - elapsed);
        }
        last_frame_time = Instant::now();
    }

    Ok(())
}