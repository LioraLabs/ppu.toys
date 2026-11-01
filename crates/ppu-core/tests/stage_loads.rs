//! Stage loads: `dma(name, opts)` called from frame() while force_blank is
//! true writes the source over VRAM/CGRAM from that update on, with the same
//! result table as at setup. Outside setup it is refused unless blanked, and
//! the update that loads must still be blanked when frame() returns.
mod common;

use ppu_core::LuaEngine;

/// A 16x8 two-tile 4bpp sheet: left tile `a`, right tile `b`.
fn two_tiles(a: [u8; 3], b: [u8; 3]) -> Vec<u8> {
    (0..16 * 8)
        .flat_map(|i| {
            let c = if i % 16 < 8 { a } else { b };
            [c[0], c[1], c[2], 255]
        })
        .collect()
}

fn engine(main: &str) -> LuaEngine {
    let mut e = LuaEngine::new();
    common::add_sheet(&mut e, "day", two_tiles([255, 0, 0], [0, 255, 0]), 16, 8, 4);
    common::add_sheet(
        &mut e,
        "night",
        two_tiles([0, 0, 255], [255, 255, 0]),
        16,
        8,
        4,
    );
    e.set_sources(&[("main.lua", main)]).unwrap();
    e
}

/// The same memory a setup placement of `name` at char 0, pal 0 gives.
fn placed(name: &str) -> (Vec<u16>, Vec<u16>) {
    let mut e = engine(&format!(
        "dma('{name}', {{char = 0x0000, pal = 0}})\nfunction frame(t, f) end"
    ));
    e.frame(0.0, 0).unwrap();
    (
        e.memory().vram[..0x1000].to_vec(),
        e.memory().cgram[..16].to_vec(),
    )
}

const SWAP: &str = r#"
local day = dma("day", {char = 0x0000, pal = 0})
loaded = nil
function frame(t, f)
  if f == 3 then
    force_blank = true
    loaded = dma("night", {char = 0x0000, pal = 0})
  elseif f == 5 then
    force_blank = false
  end
  if loaded then
    -- The result table matches setup's.
    assert(loaded.char == day.char and loaded.next_char == day.next_char and loaded.bit_depth == 4)
  end
end
"#;

#[test]
fn a_blanked_load_lands_in_its_update_and_stays() {
    let (day_vram, day_cgram) = placed("day");
    let (night_vram, night_cgram) = placed("night");
    assert_ne!(day_vram, night_vram);
    let mut e = engine(SWAP);
    for f in 0..8u32 {
        e.frame(f as f64 / 60.0, f).unwrap();
        let (vram, cgram) = if f < 3 {
            (&day_vram, &day_cgram)
        } else {
            (&night_vram, &night_cgram)
        };
        assert_eq!(&e.memory().vram[..0x1000], &vram[..], "vram at {f}");
        assert_eq!(&e.memory().cgram[..16], &cgram[..], "cgram at {f}");
    }
    // The setup layout is what the DMA panel shows; the load is reported apart.
    assert_eq!(e.memory_map().placements.len(), 1);
    let loads = e.loads();
    assert_eq!(loads.len(), 1);
    assert_eq!(loads[0].placement.name, "night");
    assert!(matches!(loads[0].vram[..], [(0, end)] if end > 0 && end <= 0x1000));
    assert_eq!(loads[0].cgram, (0, 16));
    // Run (t = 0) starts from the setup layout again.
    e.reset().unwrap();
    e.frame(0.0, 0).unwrap();
    assert_eq!(&e.memory().vram[..0x1000], &day_vram[..]);
    assert!(e.loads().is_empty());
}

#[test]
fn loading_again_replaces_rather_than_piles_up() {
    let mut e = engine(
        r#"
dma("day", {char = 0x0000, pal = 0})
function frame(t, f)
  force_blank = true
  dma(f % 2 == 0 and "night" or "day", {char = 0x0000, pal = 0})
end
"#,
    );
    let (day_vram, _) = placed("day");
    for f in 0..6 {
        e.frame(0.0, f).unwrap();
        assert_eq!(e.loads().len(), 1, "frame {f}");
    }
    assert_eq!(&e.memory().vram[..0x1000], &day_vram[..]);
}

#[test]
fn a_load_overwrites_earlier_raw_writes_and_later_ones_win() {
    let (night_vram, night_cgram) = placed("night");
    let mut e = engine(
        r#"
dma("day", {char = 0x0000, pal = 0})
function frame(t, f)
  if f == 0 then
    vram[0x0010] = 0x1234
    cgram[1] = 0x7fff
  elseif f == 1 then
    force_blank = true
    dma("night", {char = 0x0000, pal = 0})
  elseif f == 2 then
    vram[0x0011] = 0x4321
  end
end
"#,
    );
    e.frame(0.0, 0).unwrap();
    assert_eq!(e.memory().vram[0x10], 0x1234);
    assert_eq!(e.memory().cgram[1], 0x7fff);
    e.frame(0.0, 1).unwrap();
    assert_eq!(e.memory().vram[0x10], night_vram[0x10]);
    assert_eq!(e.memory().cgram[1], night_cgram[1]);
    e.frame(0.0, 2).unwrap();
    assert_eq!(e.memory().vram[0x10], night_vram[0x10]);
    assert_eq!(e.memory().vram[0x11], 0x4321);
}

#[test]
fn a_load_needs_force_blank_at_the_call() {
    let mut e = engine("function frame(t, f) dma('night', {char = 0x0000, pal = 0}) end");
    let err = e.frame(0.0, 0).unwrap_err();
    assert!(
        err.message.contains("force_blank is true"),
        "got: {}",
        err.message
    );
    assert_eq!(err.file.as_deref(), Some("main.lua"));
}

#[test]
fn unblanking_in_the_loading_update_is_an_error() {
    let mut e = engine(
        "function frame(t, f)\n  force_blank = true\n  dma('night', {char = 0x0000, pal = 0})\n  force_blank = false\nend",
    );
    let err = e.frame(0.0, 0).unwrap_err();
    assert!(err.message.contains("stay true"), "got: {}", err.message);
}

#[test]
fn a_load_from_a_hook_or_of_a_sample_is_refused() {
    let mut e = engine(
        "function frame(t, f)\n  force_blank = true\n  hdma(0, 10, function(y) dma('night') end)\nend",
    );
    assert!(e.frame(0.0, 0).unwrap_err().message.contains("not hooks"));
    let mut e = LuaEngine::new();
    common::add_sample(&mut e, "kick");
    e.set_source("function frame(t, f)\n  force_blank = true\n  dma('kick')\nend")
        .unwrap();
    assert!(e
        .frame(0.0, 0)
        .unwrap_err()
        .message
        .contains("setup for samples"));
}
