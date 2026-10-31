//! The wasm speed check: `cargo build --release --target wasm32-unknown-unknown -p ppu-cart --example
//! wasmbench`, then from JS copy a ROM into `rom_buffer(len)` and time `run(frames)`. ppu-core's
//! wasm-bindgen imports are never called here, so stub every import with a throwing function.
#![allow(clippy::missing_safety_doc)]
static mut ROM: Vec<u8> = Vec::new();

#[no_mangle]
pub unsafe extern "C" fn rom_buffer(len: usize) -> *mut u8 {
    ROM = vec![0; len];
    (*std::ptr::addr_of_mut!(ROM)).as_mut_ptr()
}

/// Play `frames` frames of the ROM; returns a checksum of the last picture.
#[no_mangle]
pub unsafe extern "C" fn run(frames: u32) -> u32 {
    let mut snes = ppu_cart::Snes::new(&*std::ptr::addr_of!(ROM)).unwrap();
    for _ in 0..frames {
        snes.run_frame();
    }
    snes.framebuffer().iter().fold(0u32, |h, &b| h.wrapping_mul(31).wrapping_add(b as u32))
}
