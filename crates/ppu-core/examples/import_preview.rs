//! Dev tool: render the tile-BG importer's output for a synthetic photo-ish
//! image at each dither mode, as PNGs, for eyeballing quantizer changes.
//!
//!     cargo run -p ppu-core --example import_preview -- /tmp/out
//!
//! Writes source.png plus one <mode>.png per dither mode to the given dir.

use ppu_core::import::dither::{DitherMode, RemapOptions};
use ppu_core::import::{import_tile_bg, ImportOptions};

const W: usize = 256;
const H: usize = 224;

/// Smooth sky gradient + soft glow + color ramp: everything the old flat
/// nearest-color remap banded on.
fn test_image() -> Vec<u8> {
    let mut rgba = Vec::with_capacity(W * H * 4);
    for y in 0..H {
        for x in 0..W {
            let t = y as f32 / H as f32;
            // vertical dusk gradient
            let (mut r, mut g, mut b) = (
                40.0 + 180.0 * t,
                20.0 + 60.0 * (1.0 - t),
                90.0 + 120.0 * (1.0 - t),
            );
            // soft radial glow
            let (dx, dy) = (x as f32 - 190.0, y as f32 - 60.0);
            let d = (dx * dx + dy * dy).sqrt();
            let glow = (1.0 - (d / 70.0).min(1.0)).powi(2);
            r += 200.0 * glow;
            g += 190.0 * glow;
            b += 120.0 * glow;
            // horizontal hue ramp band
            if (150..170).contains(&y) {
                let u = x as f32 / W as f32;
                r = 255.0 * u;
                g = 80.0;
                b = 255.0 * (1.0 - u);
            }
            rgba.extend_from_slice(&[
                r.min(255.0) as u8,
                g.min(255.0) as u8,
                b.min(255.0) as u8,
                255,
            ]);
        }
    }
    rgba
}

/// Expand one BGR555 word to RGB8 like the renderer does.
fn rgb8(c: u16) -> [u8; 3] {
    let e = |v: u16| ((v << 3) | (v >> 2)) as u8;
    [e(c & 0x1f), e((c >> 5) & 0x1f), e((c >> 10) & 0x1f)]
}

fn write_png(path: &str, rgba: &[u8], w: usize, h: usize) {
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(file, w as u32, h as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(rgba).unwrap();
}

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    std::fs::create_dir_all(&dir).unwrap();
    let rgba = test_image();
    write_png(&format!("{dir}/source.png"), &rgba, W, H);

    for (name, mode) in [
        ("none", DitherMode::None),
        ("bayer", DitherMode::Bayer),
        ("diffusion", DitherMode::Diffusion),
    ] {
        let opts = ImportOptions {
            bit_depth: 4,
            tile_size: 8,
            remap: RemapOptions {
                dither: mode,
                strength: 50,
                alpha_threshold: 128,
            },
        };
        let (src, meta) = import_tile_bg(&rgba, W as u32, H as u32, &opts);

        // reconstruct the framebuffer straight from the emitted data
        let cols = W.div_ceil(8);
        let words_per_tile = 16; // 4bpp
        let mut out = vec![0u8; W * H * 4];
        for ty in 0..H.div_ceil(8) {
            for tx in 0..cols {
                let map_cols = if cols > 32 { 64 } else { 32 };
                let sc = (ty / 32) * (map_cols / 32) + (tx / 32);
                let word = src.tilemap_words[sc * 0x400 + (ty % 32) * 32 + (tx % 32)];
                let (tile, pal) = ((word & 0x3ff) as usize, ((word >> 10) & 7) as usize);
                let (hf, vf) = (word >> 14 & 1 == 1, word >> 15 & 1 == 1);
                for py in 0..8 {
                    for px in 0..8 {
                        let (sx, sy) = (if hf { 7 - px } else { px }, if vf { 7 - py } else { py });
                        let mut idx = 0u8;
                        for p in 0..4 {
                            let w = src.char_words[tile * words_per_tile + (p >> 1) * 8 + sy];
                            let bit = if p & 1 == 1 { 8 } else { 0 } + (7 - sx);
                            idx |= (((w >> bit) & 1) as u8) << p;
                        }
                        let (x, y) = (tx * 8 + px, ty * 8 + py);
                        if x >= W || y >= H {
                            continue;
                        }
                        let o = (y * W + x) * 4;
                        if idx > 0 {
                            let c = rgb8(src.palettes[pal][idx as usize - 1]);
                            out[o..o + 3].copy_from_slice(&c);
                            out[o + 3] = 255;
                        }
                    }
                }
            }
        }
        write_png(&format!("{dir}/{name}.png"), &out, W, H);
        let ppu_core::SourceReport::Tile { report } = &meta.report else {
            panic!()
        };
        println!(
            "{name}: {} unique tiles, {} palettes, {} colors, overflows: {:?}",
            report.unique_tiles, report.palettes_used, report.colors_used, report.overflows
        );
    }
}
