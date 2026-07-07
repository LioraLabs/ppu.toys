//! Golden compositor framebuffer over hand-authored VRAM: authentic Mode-1
//! per-pixel priority resolution (tilemap priority bit x mode layer order x OBJ
//! priority, honoring the BGMODE.3 BG3-priority bit) in a top band, split over a
//! Mode-7 floor + sprite overlay in a bottom band. No importer — solid-index
//! tiles packed into real bitplanes, `vhopppcc cccccccc` tilemap entries, and
//! the real binding registers, rendered through the actual `render_frame` seam.
//!
//! VRAM is partitioned so the two modes coexist in one 32K-word space:
//!   0x0000-0x3FFF  Mode-7 interleaved map(low)/char(high)
//!   0x4000         Mode-1 BG1/BG2 4bpp char data
//!   0x5000         Mode-1 BG3 2bpp char data
//!   0x6000         OBJ 4bpp char data (OBSEL base, 0x2000 multiple)
//!   0x7000/0x7400/0x7800  BG1/BG2/BG3 tilemaps
use ppu_core::{
    render_frame, rgb15, unpack_rgb15, LineTableBuilder, LineTableRow, Memory, Obj, HEIGHT, WIDTH,
};
use std::path::Path;

const GOLDEN: &str = "tests/fixtures/golden_composite.png";

// ── colors ───────────────────────────────────────────────────────────────
fn backdrop() -> [u8; 4] {
    unpack_rgb15(rgb15(16, 16, 40))
}
fn red() -> [u8; 4] {
    unpack_rgb15(rgb15(224, 32, 32))
} // BG1 pal 0 idx 1
fn green() -> [u8; 4] {
    unpack_rgb15(rgb15(48, 208, 80))
} // BG2 pal 2 idx 1
fn cyan() -> [u8; 4] {
    unpack_rgb15(rgb15(64, 224, 224))
} // BG3 pal 1 idx 1
fn yellow() -> [u8; 4] {
    unpack_rgb15(rgb15(248, 224, 64))
} // OBJ pal 0 idx 1
fn purple() -> [u8; 4] {
    unpack_rgb15(rgb15(160, 64, 224))
} // Mode-7 tile 1 idx 2

/// Pack an 8x8 index grid into 2bpp bitplanes (8 words; word `y` = plane 0 low
/// byte, plane 1 high, bit 7 leftmost).
fn pack_2bpp(px: [[u8; 8]; 8]) -> [u16; 8] {
    std::array::from_fn(|y| {
        (0..8).fold(0u16, |w, x| {
            let bit = 7 - x;
            w | ((px[y][x] & 1) as u16) << bit | (((px[y][x] >> 1) & 1) as u16) << (bit + 8)
        })
    })
}

/// Pack an 8x8 index grid into 4bpp bitplanes (16 words: planes 0/1 then 2/3).
fn pack_4bpp(px: [[u8; 8]; 8]) -> [u16; 16] {
    let p01 = pack_2bpp(px.map(|r| r.map(|v| v & 3)));
    let p23 = pack_2bpp(px.map(|r| r.map(|v| (v >> 2) & 3)));
    std::array::from_fn(|i| if i < 8 { p01[i] } else { p23[i - 8] })
}

/// A `vhopppcc cccccccc` tilemap entry word.
fn entry(tile: u16, pal: u16, prio: bool) -> u16 {
    tile | pal << 10 | (prio as u16) << 13
}

/// Fill BG map cell (col, row) of the 32x32 screen based at `map_base`.
fn set_cell(mem: &mut Memory, map_base: usize, col: usize, row: usize, word: u16) {
    mem.vram[map_base + row * 32 + col] = word;
}

/// Mode-7 map: low byte of word (ty*128 + tx) = tile#.
fn m7_map(mem: &mut Memory, tx: usize, ty: usize, tile: u8) {
    let i = ty * 128 + tx;
    mem.vram[i] = (mem.vram[i] & 0xff00) | tile as u16;
}

/// Mode-7 char: high byte of word (tile*64 + fy*8 + fx) = 8bpp index.
fn m7_char(mem: &mut Memory, tile: usize, fx: usize, fy: usize, idx: u8) {
    let i = tile * 64 + fy * 8 + fx;
    mem.vram[i] = (mem.vram[i] & 0x00ff) | ((idx as u16) << 8);
}

/// Build the scene. Three Mode-1 priority interactions in the top band, a
/// Mode-7 purple floor + sprite in the bottom band. Tile cells are 8x8 px.
fn fixture() -> (ppu_core::LineTable, Memory) {
    let mut mem = Memory::new();

    // ── CGRAM ────────────────────────────────────────────────────────────
    mem.cgram[0] = rgb15(16, 16, 40); // backdrop
    mem.cgram[1] = rgb15(224, 32, 32); // BG1 pal 0, idx 1 (red)
    mem.cgram[2] = rgb15(160, 64, 224); // Mode-7 idx 2 (purple)
    mem.cgram[32 + 1] = rgb15(48, 208, 80); // BG2 pal 2, idx 1 (green)
    mem.cgram[4 + 1] = rgb15(64, 224, 224); // BG3 2bpp pal 1 (base 4), idx 1 (cyan)
    mem.cgram[128 + 1] = rgb15(248, 224, 64); // OBJ pal 0, idx 1 (yellow)

    // ── Char data ────────────────────────────────────────────────────────
    // BG1/BG2 4bpp char 1 = solid index 1, at 0x4000.
    for (i, w) in pack_4bpp([[1u8; 8]; 8]).into_iter().enumerate() {
        mem.vram[0x4000 + 1 * 16 + i] = w;
    }
    // BG3 2bpp char 1 = solid index 1, at 0x5000.
    for (i, w) in pack_2bpp([[1u8; 8]; 8]).into_iter().enumerate() {
        mem.vram[0x5000 + 1 * 8 + i] = w;
    }
    // OBJ 4bpp char 1 = solid index 1, at 0x6000.
    for (i, w) in pack_4bpp([[1u8; 8]; 8]).into_iter().enumerate() {
        mem.vram[0x6000 + 1 * 16 + i] = w;
    }
    // Mode-7 tile 1 = solid index 2 (purple floor).
    for fy in 0..8 {
        for fx in 0..8 {
            m7_char(&mut mem, 1, fx, fy, 2);
        }
    }
    for ty in 0..28 {
        for tx in 0..32 {
            m7_map(&mut mem, tx, ty, 1);
        }
    }

    // ── Tilemaps (Mode-1 top band) ─────────────────────────────────────────
    // Interaction 1 — tilemap priority bit lifts BG2 above BG1. Cells (2, 2..3).
    // BG1 red prio 0 under BG2 green prio 1 -> green wins.
    for row in 2..=3 {
        set_cell(&mut mem, 0x7000, 2, row, entry(1, 0, false)); // BG1
        set_cell(&mut mem, 0x7400, 2, row, entry(1, 2, true)); // BG2 prio 1
    }
    // Interaction 2 — BG3-priority bit (BGMODE.3) lifts BG3 above BG1. Cells
    // (6, 2..3). BG1 red prio 0 vs BG3 cyan prio 1; an HDMA sets the bit for
    // y >= 24, so the upper half (y 16..24) shows red, lower half (y 24..32) cyan.
    for row in 2..=3 {
        set_cell(&mut mem, 0x7000, 6, row, entry(1, 0, false)); // BG1
        set_cell(&mut mem, 0x7800, 6, row, entry(1, 1, true)); // BG3 pal 1 prio 1
    }
    // Interaction 3 — sprite vs BG. Cells (10, 2..3) hold BG1 red with its
    // tilemap priority bit set. A prio-3 sprite sits above it; a prio-0 sprite
    // sits below it.
    for row in 2..=3 {
        set_cell(&mut mem, 0x7000, 10, row, entry(1, 0, true)); // BG1 prio 1
    }

    // ── Sprites (OAM) ──────────────────────────────────────────────────────
    mem.obsel.char_base = 0x6000;
    // S0 prio 3 over the BG1-prio1 tile (y 16..24) -> sprite yellow wins.
    mem.oam[0] = Obj {
        on: true,
        x: 80,
        y: 16,
        tile: 1,
        prio: 3,
        ..Obj::default()
    };
    // S1 prio 0 under the BG1-prio1 tile (y 24..32) -> BG1 red wins.
    mem.oam[1] = Obj {
        on: true,
        x: 80,
        y: 24,
        tile: 1,
        prio: 0,
        ..Obj::default()
    };
    // S2 over the Mode-7 floor (bottom band).
    mem.oam[2] = Obj {
        on: true,
        x: 120,
        y: 180,
        tile: 1,
        prio: 2,
        ..Obj::default()
    };

    // ── Registers / HDMA ───────────────────────────────────────────────────
    let mut def = LineTableRow::default(); // Mode 1, brightness 15
    def.bg[0].char_base = 0x4000;
    def.bg[0].map_base = 0x7000;
    def.bg[1].char_base = 0x4000;
    def.bg[1].map_base = 0x7400;
    def.bg[2].char_base = 0x5000;
    def.bg[2].map_base = 0x7800;
    let mut b = LineTableBuilder::new(def);
    // BG3-priority bit on the lower half of Interaction 2.
    b.hdma(24, 149, |_, r| {
        r.bg3_priority = true;
    });
    // Bottom band is Mode 7 (default identity m7 matrix = flat floor sampling).
    b.hdma(150, 223, |_, r| {
        r.mode = 7;
    });
    (b.build(HEIGHT), mem)
}

fn px(fb: &[u8], x: usize, y: usize) -> [u8; 4] {
    let o = (y * WIDTH + x) * 4;
    [fb[o], fb[o + 1], fb[o + 2], fb[o + 3]]
}

fn decode_png(path: &str) -> Vec<u8> {
    let decoder = png::Decoder::new(std::fs::File::open(path).unwrap());
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    buf
}

/// Structural checks independent of the committed PNG: every priority
/// interaction resolves to its expected winning color at a known pixel. Guards
/// against a degenerate golden being silently frozen.
#[test]
fn composite_priority_resolves_per_interaction() {
    let (lt, mem) = fixture();
    let fb = render_frame(&lt, &mem);

    // Interaction 1: BG2 tilemap-priority bit lifts green over BG1 red.
    assert_eq!(
        px(&fb, 18, 18),
        green(),
        "BG2 prio-1 should beat BG1 prio-0"
    );

    // Interaction 2: BG3-priority bit. Upper half (bit clear) red, lower half
    // (bit set) cyan — BG3 prio-1 lifts above every layer only when the bit is set.
    assert_eq!(
        px(&fb, 50, 18),
        red(),
        "BG3 prio-1 sits low with BGMODE.3 clear"
    );
    assert_eq!(
        px(&fb, 50, 28),
        cyan(),
        "BGMODE.3 lifts BG3 prio-1 to the front"
    );

    // Interaction 3: OBJ priority interleaves with a BG1-prio1 tile.
    assert_eq!(
        px(&fb, 82, 18),
        yellow(),
        "OBJ prio 3 sits above BG1 prio 1"
    );
    assert_eq!(px(&fb, 82, 28), red(), "OBJ prio 0 sits below BG1 prio 1");

    // Mode-7 bottom band: purple floor, with a sprite overlaid on top.
    assert_eq!(px(&fb, 8, 180), purple(), "Mode-7 floor missing");
    assert_eq!(
        px(&fb, 122, 182),
        yellow(),
        "sprite should overlay the Mode-7 floor"
    );

    // Backdrop where nothing is drawn.
    assert_eq!(px(&fb, 200, 100), backdrop(), "backdrop missing");
}

#[test]
fn composite_matches_golden_png() {
    assert!(
        Path::new(GOLDEN).exists(),
        "golden missing — run: cargo test -p ppu-core regen_golden_composite -- --ignored"
    );
    let (lt, mem) = fixture();
    let actual = render_frame(&lt, &mem);
    let expected = decode_png(GOLDEN);
    assert_eq!(actual.len(), WIDTH * HEIGHT * 4);
    assert_eq!(actual, expected, "framebuffer differs from golden PNG");
}

#[test]
#[ignore = "regenerates the committed golden PNG"]
fn regen_golden_composite() {
    let (lt, mem) = fixture();
    let fb = render_frame(&lt, &mem);
    std::fs::create_dir_all("tests/fixtures").unwrap();
    let file = std::fs::File::create(GOLDEN).unwrap();
    let mut encoder = png::Encoder::new(file, WIDTH as u32, HEIGHT as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&fb)
        .unwrap();
}
