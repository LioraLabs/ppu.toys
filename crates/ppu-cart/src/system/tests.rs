//! System tests that need no CPU: everything is driven through `Bus` as the 65816 would.
use super::*;
use ppu_core::{render_frame, LineTableBuilder, LineTableRow};

/// A 64 KiB LoROM image with a valid header; `code` lands at $00:8000.
fn lorom(map: u8) -> Vec<u8> {
    let mut r = vec![0u8; 0x10000];
    r[0x7fc0..0x7fc0 + 21].copy_from_slice(b"PPU CART TEST        ");
    r[0x7fd5] = map;
    r[0x7fdc..0x7fe0].copy_from_slice(&[0xff, 0xff, 0, 0]);
    r[0x7ffc..0x7ffe].copy_from_slice(&[0x00, 0x80]);
    r
}

fn sys() -> System {
    let mut r = lorom(0x30);
    for (i, b) in r.iter_mut().enumerate().skip(0x100).take(0x100) {
        *b = i as u8;
    }
    System::new(r, false, 0)
}

#[test]
fn header_detects_lorom_and_hirom() {
    let lo = lorom(0x30);
    assert!(!Snes::new(&lo).unwrap().hirom());
    let mut hi = vec![0u8; 0x10000];
    hi[0xffc0..0xffc0 + 21].copy_from_slice(b"PPU CART TEST        ");
    hi[0xffd5] = 0x31;
    hi[0xffdc..0xffe0].copy_from_slice(&[0xff, 0xff, 0, 0]);
    hi[0xfffc..0xfffe].copy_from_slice(&[0x00, 0x80]);
    assert!(Snes::new(&hi).unwrap().hirom());
}

#[test]
fn lorom_maps_rom_wram_and_speeds() {
    let mut s = sys();
    assert_eq!(s.peek(0x00_8100), 0x00);
    assert_eq!(s.peek(0x00_8105), 0x05);
    assert_eq!(s.peek(0x80_8105), 0x05); // mirror
    s.poke(0x7e_0010, 0x42);
    assert_eq!(s.peek(0x00_0010), 0x42); // low-bank WRAM mirror
    assert_eq!(s.peek(0x80_0010), 0x42);
    s.poke(0x7f_1234, 9);
    assert_eq!(s.wram[0x11234], 9);
    assert_eq!(s.speed(0x80_8000), 8);
    s.poke(0x00_420d, 1);
    assert_eq!(s.speed(0x80_8000), 6);
    assert_eq!(s.speed(0x00_8000), 8);
    assert_eq!(s.speed(0x00_4016), 12);
    assert_eq!(s.speed(0x7e_0000), 8);
    assert_eq!(s.speed(0x00_2100), 6);
}

#[test]
fn frame_is_262_lines_of_1364_clocks() {
    let mut s = sys();
    // Two frames: one has the short line 240.
    let start = s.clock;
    let mut frames = 0;
    while frames < 2 {
        s.frame_done = false;
        while !s.frame_done {
            s.idle();
        }
        frames += 1;
    }
    let total = s.clock - start;
    // The first frame ends at VBlank start (line 225); the second is a full frame (+ refresh is
    // inside the line, not added to it).
    let first = 225 * 1364;
    assert!((total as i64 - (first + 262 * 1364 - 4) as i64).abs() < 12, "{total}");
}

#[test]
fn wram_port_and_multiply_divide() {
    let mut s = sys();
    for (r, v) in [(0x2181, 0x34), (0x2182, 0x12), (0x2183, 0x01)] {
        s.write(r, v);
    }
    s.write(0x2180, 0xaa);
    s.write(0x2180, 0xbb);
    assert_eq!(&s.wram[0x11234..0x11236], &[0xaa, 0xbb]);
    s.write(0x4202, 200);
    s.write(0x4203, 100);
    assert_eq!(s.read(0x4216) as u16 | (s.read(0x4217) as u16) << 8, 20000);
    s.write(0x4204, 0x10);
    s.write(0x4205, 0x27); // 10000
    s.write(0x4206, 7);
    assert_eq!(s.read(0x4214) as u16 | (s.read(0x4215) as u16) << 8, 1428);
    assert_eq!(s.read(0x4216), 4);
}

/// Mode 1, BG1 4bpp at char $1000, map $0400, one tile with a gradient, CGRAM via DMA, VRAM
/// via DMA mode 1: the bridge's line must equal ppu-core's own frame for the same registers.
#[test]
fn raw_registers_render_like_the_line_table() {
    let mut s = sys();
    s.write(0x2100, 0x80); // force blank while loading
    // CGRAM 0..16 from WRAM via DMA mode 0 to $2122.
    for i in 0..32u32 {
        s.wram[0x100 + i as usize] = (i * 7) as u8 & if i % 2 == 1 { 0x7f } else { 0xff };
    }
    s.write(0x2121, 0);
    for (r, v) in [(0x4300, 0x00), (0x4301, 0x22), (0x4302, 0x00), (0x4303, 0x01), (0x4304, 0x7e), (0x4305, 32), (0x4306, 0)] {
        s.write(r, v);
    }
    s.write(0x420b, 1);
    // VRAM: char 1 at $1000 + 16 words, a tilemap of tile 1 everywhere at $0400 (1024 words).
    for w in 0..16usize {
        let v = (0x1111u32 * w as u32) as u16;
        s.wram[0x200 + w * 2] = v as u8;
        s.wram[0x201 + w * 2] = (v >> 8) as u8;
    }
    s.write(0x2115, 0x80);
    s.write(0x2116, 0x10);
    s.write(0x2117, 0x10); // $1010 = char 1
    for (r, v) in [(0x4310, 0x01), (0x4311, 0x18), (0x4312, 0x00), (0x4313, 0x02), (0x4314, 0x7e), (0x4315, 32), (0x4316, 0)] {
        s.write(r, v);
    }
    s.write(0x420b, 2);
    s.write(0x2116, 0x00);
    s.write(0x2117, 0x04);
    for _ in 0..1024 {
        s.write(0x2118, 0x01);
        s.write(0x2119, 0x00);
    }
    s.write(0x2105, 0x01);
    s.write(0x2107, 0x04);
    s.write(0x210b, 0x01);
    s.write(0x210d, 3);
    s.write(0x210d, 0);
    s.write(0x210e, 5);
    s.write(0x210e, 0);
    s.write(0x212c, 0x01);
    s.write(0x2100, 0x0f);
    let mut out = vec![0u8; 1024];
    s.ppu.render_line(10, &mut out);
    let mut row = LineTableRow { mode: 1, tm: 1, ..Default::default() };
    row.bg[0].map_base = 0x400;
    row.bg[0].char_base = 0x1000;
    row.bg[0].scroll_x = 3.0;
    row.bg[0].scroll_y = 5.0;
    let lt = LineTableBuilder::new(row).build(224);
    let want = render_frame(&lt, &s.ppu.mem);
    assert_eq!(&out[..], &want[10 * 1024..11 * 1024]);
    assert!(out.chunks(4).any(|p| p[..3] != [0, 0, 0]));
}

#[test]
fn bg_hofs_keeps_all_three_fine_bits() {
    // Every value with bit 2 set lost it (scroll.x = 4 drew at 0): the fine bits came from the
    // 10-bit register's bits 8-10. bsnes keeps them in a latch of their own, and so does ppu-cart now.
    let mut s = sys();
    for v in [4u16, 5, 7, 0x3fc, 0x3ff, 0x123] {
        s.write(0x210d, v as u8);
        s.write(0x210d, (v >> 8) as u8);
        s.write(0x210e, 9); // a VOFS write between HOFS pairs moves the shared latch, not the fine one
        s.write(0x210e, 0);
        assert_eq!(s.ppu.hofs(0), v);
    }
}

#[test]
fn hdma_direct_and_indirect_set_registers_per_line() {
    let mut s = sys();
    // Direct, mode 0 to COLDATA ($2132): 3 lines of $21, then 2 lines repeat-mode $22/$23.
    let t = [3u8, 0x21, 0x82, 0x22, 0x23, 0];
    s.wram[0x300..0x300 + t.len()].copy_from_slice(&t);
    // Indirect, mode 0 to BG1HOFS... use $2126 (WH0): count 2 -> data at $7E:0400.
    let ti = [2u8, 0x00, 0x04, 0];
    s.wram[0x380..0x384].copy_from_slice(&ti);
    s.wram[0x400] = 77;
    for (r, v) in [(0x4300, 0x00), (0x4301, 0x32), (0x4302, 0x00), (0x4303, 0x03), (0x4304, 0x7e)] {
        s.write(r, v);
    }
    for (r, v) in [(0x4310, 0x40), (0x4311, 0x26), (0x4312, 0x80), (0x4313, 0x03), (0x4314, 0x7e), (0x4317, 0x7e)] {
        s.write(r, v);
    }
    s.write(0x420c, 3);
    // Run to line 0, then record COLDATA red and WH0 after each line's HDMA.
    let mut seen = vec![];
    let mut wh = vec![];
    while s.v != 1 {
        s.idle();
    }
    while s.v != 0 {
        s.idle();
    }
    for _ in 0..6 {
        let v = s.v;
        while s.v == v {
            s.idle();
        }
        seen.push(s.ppu.row_coldata() & 0x1f);
        wh.push(s.ppu.row_wh0());
    }
    // Line 0: $21 (count 3: lines 0-2 transfer once, then hold), then repeat lines 3,4: $22, $23.
    assert_eq!(seen, vec![1, 1, 1, 2, 3, 3]);
    assert_eq!(wh, vec![77, 77, 77, 77, 77, 77]);
    assert!(s.dma[1].done);
}

#[test]
fn joypad_auto_read_and_serial() {
    let mut s = sys();
    s.pads[0] = crate::pad::B | crate::pad::A;
    s.write(0x4200, 0x01);
    while s.v != VDISP || s.h <= AUTOJOY_H {
        s.idle();
    }
    assert_eq!(s.read(0x4212) & 1, 1);
    assert_eq!(s.read(0x4219), 0x80);
    assert_eq!(s.read(0x4218), 0x80);
    s.write(0x4016, 1);
    s.write(0x4016, 0);
    assert_eq!(s.read(0x4016) & 1, 1); // B
    for _ in 0..7 {
        assert_eq!(s.read(0x4016) & 1, 0);
    }
    assert_eq!(s.read(0x4016) & 1, 1); // A
}

#[test]
fn oam_double_write_and_high_table() {
    let mut s = sys();
    s.write(0x2102, 0);
    s.write(0x2103, 0);
    for v in [0xf0, 20, 0x05, 0x31] {
        s.write(0x2104, v);
    }
    s.write(0x2102, 0x00);
    s.write(0x2103, 0x01); // high table
    s.write(0x2104, 0b11);
    let o = s.ppu.mem.oam[0];
    assert_eq!((o.x, o.y, o.tile, o.pal, o.prio, o.large), (-16, 20, 0x105, 0, 3, true));
}

#[test]
fn battery_ram_takes_an_srm_image_of_the_header_size() {
    let mut rom = lorom(0x30);
    rom[0x7fd6] = 0x02; // ROM + RAM + battery
    rom[0x7fd8] = 0x01; // 2 KiB
    let mut snes = Snes::new(&rom).unwrap();
    assert_eq!(snes.sram().len(), 2048);
    let image: Vec<u8> = (0..2048).map(|i| (i * 7) as u8).collect();
    snes.load_sram(&image).unwrap();
    assert_eq!(snes.sram(), &image[..]);
    // The game reads it at $70:0000 (and the mirror $F0).
    assert_eq!(snes.bus.peek(0x70_0003), image[3]);
    assert_eq!(snes.bus.peek(0xf0_0800 + 5), image[5]);
    assert!(snes.load_sram(&image[..1024]).is_err());
    assert!(Snes::new(&lorom(0x30)).unwrap().sram().is_empty());
}
