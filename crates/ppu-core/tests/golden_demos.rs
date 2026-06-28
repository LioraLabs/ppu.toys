//! M1-ENGINE done gate: the two flagship worked examples (dusk-parallax,
//! mode7-floor) driven end-to-end through the real LuaEngine -> frame ->
//! render_frame pipeline, headless, compared to committed golden PNGs.
//!
//! The demo Lua embeds each spec "Worked example" verbatim; the only additions
//! are the runnable-demo glue the illustrative snippets omit (obj.sheet + on),
//! noted inline. Sources are procedural Direct-RGBA (the upload contract).
use ppu_core::{render_frame, LuaEngine, Source, HEIGHT, WIDTH};
use std::path::Path;

// ── golden PNG helpers (same convention as golden_composite/golden_mode7) ──────
fn decode_png(path: &str) -> Vec<u8> {
    let decoder = png::Decoder::new(std::fs::File::open(path).unwrap());
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    buf
}

fn write_png(path: &str, fb: &[u8]) {
    std::fs::create_dir_all("tests/fixtures").unwrap();
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(file, WIDTH as u32, HEIGHT as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().unwrap().write_image_data(fb).unwrap();
}

// ── dusk-parallax ─────────────────────────────────────────────────────────────
const DUSK_GOLDEN: &str = "tests/fixtures/golden_dusk_parallax.png";

/// Spec worked example, verbatim, plus the two runnable-demo lines the snippet
/// omits (obj.sheet + obj[0].on) so the sprite is actually drawn.
const DUSK_SRC: &str = r#"
local SPEED = 12
function frame(t, f)
  mode = 1; brightness = 15
  bg[1].source = "sky";   bg[2].source = "hills"
  bg[1].scroll.x = t * SPEED
  bg[2].scroll.x = t * SPEED * 3
  for i = 0, 7 do cgram[0x40 + i] = hsl((t*40 + i*12) % 360, 0.6, 0.5) end
  obj[0].tile = 4; obj[0].pal = 2; obj[0].x = 120; obj[0].y = 132 + sin(t*3) * 4
  -- demo glue the spec snippet omits (so the sprite renders):
  obj.sheet = "hero"; obj[0].on = true
end
"#;

/// 64x64 "sky": vertical dusk gradient with diagonal stripes (horizontal
/// variation so x-scroll is visible); lower half transparent so "hills" shows
/// through the topmost BG1.
fn sky_source() -> Source {
    let (w, h) = (64u32, 64u32);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            if y >= h / 2 {
                rgba[i + 3] = 0; // transparent lower half -> hills shows
                continue;
            }
            let stripe = (((x + y) / 4) % 2) as u8;
            rgba[i] = 80 + stripe * 60; // R: dusk warm
            rgba[i + 1] = 40 + (y as u8); // G: rises with height
            rgba[i + 2] = 120 + stripe * 40; // B: violet
            rgba[i + 3] = 255;
        }
    }
    Source { width: w, height: h, rgba }
}

/// 64x64 "hills": opaque vertical bands (distinct hues per column block) so the
/// faster BG2 x-scroll is clearly visible.
fn hills_source() -> Source {
    let (w, h) = (64u32, 64u32);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let band = (x / 8) as u8;
            rgba[i] = 20 + band * 16;
            rgba[i + 1] = 60 + band * 20;
            rgba[i + 2] = 30;
            rgba[i + 3] = 255;
        }
    }
    Source { width: w, height: h, rgba }
}

/// 64x8 OBJ sheet: 8 distinct 8x8 cells so `obj[0].tile = 4` resolves to a
/// recognizable solid cell.
fn hero_sheet() -> Source {
    let (w, h) = (64u32, 8u32);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let cell = (x / 8) as u8;
            rgba[i] = 255 - cell * 16;
            rgba[i + 1] = 200;
            rgba[i + 2] = cell * 24;
            rgba[i + 3] = 255;
        }
    }
    Source { width: w, height: h, rgba }
}

/// Run dusk-parallax through the real pipeline at a fixed (t,f).
fn dusk_frame(t: f64, f: u32) -> Vec<u8> {
    let mut engine = LuaEngine::new();
    engine.set_source(DUSK_SRC).expect("dusk-parallax compiles");
    {
        let mem = engine.memory_mut();
        mem.sources.insert("sky".into(), sky_source());
        mem.sources.insert("hills".into(), hills_source());
        mem.sources.insert("hero".into(), hero_sheet());
    }
    let lt = engine.frame(t, f).expect("dusk-parallax frame runs");
    render_frame(&lt, engine.memory())
}

#[test]
fn dusk_parallax_scroll_animates() {
    // Motion: the two BG layers x-scroll with t, so t=0 and t=1 must differ.
    let a = dusk_frame(0.0, 0);
    let b = dusk_frame(1.0, 60);
    assert_eq!(a.len(), WIDTH * HEIGHT * 4);
    assert_ne!(a, b, "dusk-parallax did not animate between t=0 and t=1");
}

#[test]
fn dusk_parallax_matches_golden_png() {
    assert!(
        Path::new(DUSK_GOLDEN).exists(),
        "golden missing — run: cargo test -p ppu-core --test golden_demos regen_golden_demos -- --ignored"
    );
    let actual = dusk_frame(1.0, 60);
    let expected = decode_png(DUSK_GOLDEN);
    assert_eq!(actual.len(), WIDTH * HEIGHT * 4);
    assert_eq!(actual, expected, "dusk-parallax framebuffer differs from golden PNG");
}

// ── mode7-floor ───────────────────────────────────────────────────────────────
const MODE7_GOLDEN: &str = "tests/fixtures/golden_mode7_floor.png";

/// Spec worked example, verbatim.
const MODE7_SRC: &str = r#"
function frame(t, f)
  mode = 7; brightness = 15; bg[1].source = "track"
  hdma(96, 223, function(y)
    local d = 64 / (y - 95)
    m7.a, m7.d = d, d
    m7.cx, m7.cy = 128, 0
    bg[1].scroll.y = (t*80) * d
  end)
end
"#;

/// 64x64 procedural "track": an 8x8 grid of distinctly-colored cells so the
/// receding-perspective warp is legible (same source style as golden_mode7).
fn track_source() -> Source {
    let (w, h) = (64u32, 64u32);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let cx = (x / 8) as u8;
            let cy = (y / 8) as u8;
            let i = ((y * w + x) * 4) as usize;
            rgba[i] = cx * 32;
            rgba[i + 1] = cy * 32;
            rgba[i + 2] = ((cx + cy) & 1) * 255;
            rgba[i + 3] = 255;
        }
    }
    Source { width: w, height: h, rgba }
}

/// Run mode7-floor through the real pipeline at a fixed (t,f).
fn mode7_frame(t: f64, f: u32) -> Vec<u8> {
    let mut engine = LuaEngine::new();
    engine.set_source(MODE7_SRC).expect("mode7-floor compiles");
    engine
        .memory_mut()
        .sources
        .insert("track".into(), track_source());
    let lt = engine.frame(t, f).expect("mode7-floor frame runs");
    render_frame(&lt, engine.memory())
}

#[test]
fn mode7_floor_is_per_scanline_affine() {
    // The hdma hook switches the affine per line: rows in the floor band carry a
    // shrinking 1/(y-95) scale, so adjacent floor scanlines are NOT identical.
    let fb = mode7_frame(1.0, 60);
    let row = |y: usize| &fb[y * WIDTH * 4..(y + 1) * WIDTH * 4];
    assert_ne!(row(150), row(200), "floor scanlines should differ (perspective)");
}

#[test]
fn mode7_floor_matches_golden_png() {
    assert!(
        Path::new(MODE7_GOLDEN).exists(),
        "golden missing — run: cargo test -p ppu-core --test golden_demos regen_golden_demos -- --ignored"
    );
    let actual = mode7_frame(1.0, 60);
    let expected = decode_png(MODE7_GOLDEN);
    assert_eq!(actual.len(), WIDTH * HEIGHT * 4);
    assert_eq!(actual, expected, "mode7-floor framebuffer differs from golden PNG");
}

// ── regen (ignored): (re)generate both committed golden fixtures ──────────────
#[test]
#[ignore = "regenerates the committed golden demo PNGs"]
fn regen_golden_demos() {
    write_png(DUSK_GOLDEN, &dusk_frame(1.0, 60));
    write_png(MODE7_GOLDEN, &mode7_frame(1.0, 60));
}
