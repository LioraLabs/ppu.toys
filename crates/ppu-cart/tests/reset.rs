//! Power-on: how long after the APU and PPU start the 65816 fetches its reset vector. The ROM spins
//! `ITER` * 65536 `dex; bne` loops from reset with interrupts off, then latches the H/V counters;
//! a CPU loop has no other clock to drift against, so the latch is the reset delay plus a constant.
//! The expected latches are bsnes v115.1's (libretro), measured on 2026-10-31 by playing the ROM
//! (`PPU_CART_RESET_ROM=reset.sfc`) in a libretro core; Snes9x latches 3 dots later. The delay
//! also sets where the APU stands when a game's upload starts: without it, a game that read a
//! voice's ENVX every frame heard its music 2-6 samples off bsnes.
use ppu_cart::Snes;

const DONE: &[u8; 8] = b"BOOTDONE";

fn build(iter: u16) -> Vec<u8> {
    let mut c: Vec<u8> = vec![
        0x78, 0x18, 0xfb, 0xc2, 0x10, 0xe2, 0x20, 0xa2, 0xff, 0x1f, 0x9a,
    ];
    c.extend([0xa0, iter as u8, (iter >> 8) as u8]); // ldy #iter
    let outer = c.len();
    c.extend([0xa2, 0x00, 0x00, 0xca, 0xd0, 0xfd, 0x88, 0xd0]); // ldx #0; dex; bne; dey; bne outer
    c.push((outer as i32 - (c.len() as i32 + 1)) as u8);
    c.extend([0xad, 0x3f, 0x21, 0xad, 0x37, 0x21]); // lda $213f; lda $2137
    for (k, reg) in [0x3c, 0x3c, 0x3d, 0x3d].into_iter().enumerate() {
        c.extend([0xad, reg, 0x21, 0x8f, k as u8, 0x1f, 0x7e]); // lda reg; sta $7e1f00+k
    }
    for (k, &ch) in DONE.iter().enumerate() {
        c.extend([0xa9, ch, 0x8f, 8 + k as u8, 0x1f, 0x7e]);
    }
    c.extend([0x80, 0xfe]);
    let mut rom = vec![0u8; 0x8000];
    rom[..c.len()].copy_from_slice(&c);
    rom[0x7fc0..0x7fc0 + 21].copy_from_slice(b"PPU CART RESET       ");
    rom[0x7fd5] = 0x20;
    rom[0x7fd7] = 0x05;
    rom[0x7ffc..0x7ffe].copy_from_slice(&[0x00, 0x80]);
    rom
}

/// (frames run, V, H) when the latch is in work RAM.
fn run(iter: u16) -> (u32, u16, u16) {
    let rom = build(iter);
    if let Ok(p) = std::env::var("PPU_CART_RESET_ROM") {
        std::fs::write(format!("{p}.{iter}"), &rom).unwrap();
    }
    let mut s = Snes::new(&rom).unwrap();
    for f in 1..1000 {
        s.run_frame();
        let w = &s.wram()[0x1f00..0x1f10];
        if &w[8..] == DONE {
            let h = u16::from_le_bytes([w[0], w[1]]) & 0x1ff;
            let v = u16::from_le_bytes([w[2], w[3]]) & 0x1ff;
            return (f, v, h);
        }
    }
    panic!("the reset ROM did not finish");
}

#[test]
fn reset_delay_matches_hardware() {
    // bsnes: (frames run, V, H) for ITER 1 and 3.
    for (iter, want) in [(1, (7, 210, 94)), (3, (21, 106, 92))] {
        assert_eq!(
            run(iter),
            want,
            "ITER {iter}: (frames, V, H) differs from bsnes"
        );
    }
}
