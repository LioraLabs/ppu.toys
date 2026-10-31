//! `cartrun ROM [FRAMES]`: play a ROM headless with no input and report frames per second.
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let rom = std::fs::read(args.get(1).expect("usage: cartrun ROM [FRAMES]")).expect("read ROM");
    let frames: u32 = args.get(2).map_or(600, |n| n.parse().expect("FRAMES"));
    let mut snes = ppu_cart::Snes::new(&rom).expect("load ROM");
    let t = Instant::now();
    for _ in 0..frames {
        snes.run_frame();
    }
    let s = t.elapsed().as_secs_f64();
    println!("{frames} frames in {s:.2}s: {:.0} fps", frames as f64 / s);
}
