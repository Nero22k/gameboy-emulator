use std::fs::File;
use std::io::Read;
use std::time::Duration;
use std::time::Instant;
use std::thread::sleep;
use std::env;

use emulator101::cpu::Cpu;
use emulator101::memory::MemoryBus;
use emulator101::ppu::{SCREEN_WIDTH, SCREEN_HEIGHT};
use emulator101::vram_viewer::VramViewer;

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
    if args.len() < 2 {
        println!("Usage: emulator101 [test|run <rom_path>]");
        return Ok(());
    }

    if args[1] == "test" {
        run_tests();
    } 
    else if args[1] == "run" {
        if args.len() < 3 {
            println!("Usage: emulator101 run <rom_path>");
            return Ok(());
        }
        run_emulator(&args[2])?;
    } else {
        println!("Usage: emulator101 [test|run <rom_path>]");
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
    let mut memory = MemoryBus::new(&rom_data);
    let mut cpu = Cpu::new();
    cpu.reset();

    // Initialize VRAM viewer
    let mut vram_viewer = VramViewer::new(&sdl_context)?;

    // Timing variables
    let mut last_frame_time = Instant::now();
    let frame_duration = Duration::from_nanos(1_000_000_000 / 60); // Target 60 FPS

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
                _ => {
                    if vram_viewer.is_open() {
                        if vram_viewer.handle_event(&event) {
                            continue; // Event was handled by viewer
                        }
                    }
                    
                    // Handle other events for the main emulator
                    match &event {
                        Event::KeyDown { keycode: Some(key), repeat: false, .. } => {
                            memory.handle_key_event(*key, true);
                        },
                        Event::KeyUp { keycode: Some(key), repeat: false, .. } => {
                            memory.handle_key_event(*key, false);
                        },
                        _ => {}
                    }
                }
            }
        }

        // Update joypad state
        memory.update_joypad();
        
        // Run CPU cycles until a frame is ready (at 60 FPS)
        let mut cycles_this_frame = 0;
        while !memory.ppu.frame_ready && cycles_this_frame < 70224 { // ~70224 cycles per frame (@59.73 fps)
            // Execute one CPU instruction
            let cycles = cpu.step(&mut memory);
            cycles_this_frame += cycles as u32;
            
            // Update timer with the number of cycles
            memory.update_timer(cycles);
            
            // Update PPU with the number of cycles
            memory.update_ppu(cycles);
            
            // Handle interrupts
            let interrupt_cycles = cpu.handle_interrupts(&mut memory);
            if interrupt_cycles > 0 {
                cycles_this_frame += interrupt_cycles as u32;
                memory.update_timer(interrupt_cycles);
                memory.update_ppu(interrupt_cycles);
            }
        }
        
        // Check if a frame is ready
        if memory.ppu.frame_ready {
            memory.ppu.frame_ready = false;
            
            // Update the texture with the new frame buffer
            texture.update(None, &memory.ppu.frame_buffer, SCREEN_WIDTH * 4)?;
            
            // Clear the screen
            canvas.clear();
            
            // Copy the texture to the canvas
            canvas.copy(&texture, None, Some(Rect::new(0, 0, SCREEN_WIDTH as u32 * SCALE, SCREEN_HEIGHT as u32 * SCALE)))?;
            
            // Present the canvas
            canvas.present();

            if vram_viewer.is_open() {
                vram_viewer.update(&memory.ppu)?;
            }
            
            // Frame timing for 60 FPS
            let now = Instant::now();
            let elapsed = now.duration_since(last_frame_time);
            if elapsed < frame_duration {
                sleep(frame_duration - elapsed);
            }
            last_frame_time = Instant::now();
        }
    }

    Ok(())
}

fn run_tests() {
    // Run tests
    let test_roms = vec![
        "..\\..\\gb-test-roms-master\\cpu_instrs\\individual\\01-special.gb",
        "..\\..\\gb-test-roms-master\\cpu_instrs\\individual\\02-interrupts.gb",
        "..\\..\\gb-test-roms-master\\cpu_instrs\\individual\\03-op sp,hl.gb",
        "..\\..\\gb-test-roms-master\\cpu_instrs\\individual\\04-op r,imm.gb",
        "..\\..\\gb-test-roms-master\\cpu_instrs\\individual\\05-op rp.gb",
        "..\\..\\gb-test-roms-master\\cpu_instrs\\individual\\06-ld r,r.gb",
        "..\\..\\gb-test-roms-master\\cpu_instrs\\individual\\07-jr,jp,call,ret,rst.gb",
        "..\\..\\gb-test-roms-master\\cpu_instrs\\individual\\08-misc instrs.gb",
        "..\\..\\gb-test-roms-master\\cpu_instrs\\individual\\09-op r,r.gb",
        "..\\..\\gb-test-roms-master\\cpu_instrs\\individual\\10-bit ops.gb",
        "..\\..\\gb-test-roms-master\\cpu_instrs\\individual\\11-op a,(hl).gb",
        "..\\..\\gb-test-roms-master\\instr_timing\\instr_timing.gb",
        //"..\\..\\gb-test-roms-master\\mem_timing\\individual\\01-read_timing.gb",
        //"..\\..\\gb-test-roms-master\\mem_timing\\individual\\02-write_timing.gb",
        //"..\\..\\gb-test-roms-master\\mem_timing\\individual\\03-modify_timing.gb",
        //"..\\..\\gb-test-roms-master\\halt_bug.gb", No serial output requies display LCD to see results
    ];

    let mut passed_count = 0;
    let mut failed_count = 0;
    let total_count = test_roms.len();

    for rom in test_roms {
        let rom_data = read_rom(rom).unwrap();
        let mut memory = MemoryBus::new(&rom_data);
        let mut cpu = Cpu::new();
        cpu.reset();

        println!("Running test: {}", rom);
        println!("Listening on serial port for output");
        while true
        {
            // Execute one CPU instruction
            let cycles = cpu.step(&mut memory);

            // Update timer with the number of cycles
            memory.update_timer(cycles);
            
            // Handle interrupts
            let interrupt_cycles: u8 = cpu.handle_interrupts(&mut memory);
            if interrupt_cycles > 0
            {
                // In a real emulator, we'd update timers, PPU, etc. here
                memory.update_timer(interrupt_cycles);
            }

            // Check for test output
            if memory.serial_output.contains("Passed") {
                passed_count += 1;
                break;
            } else if memory.serial_output.contains("Failed") {
                failed_count += 1;
                break;
            }
        }

        // Output results
        println!("Test finished!");
        println!("Cycles executed: {}", cpu.cycle_count);
        println!("Serial output: {}", memory.serial_output);
    }

    println!("-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-");
    println!("Tests passed: {}/{}", passed_count, total_count);
}