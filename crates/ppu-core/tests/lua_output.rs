//! frame() -> LineTable + CGRAM/OAM assertions for small DSL snippets and the
//! two worked-example acceptance sources.
use ppu_core::{rgb15, LuaEngine};

fn engine(src: &str) -> LuaEngine {
    let mut e = LuaEngine::new();
    e.set_source(src).expect("source should compile");
    e
}

#[test]
fn bare_assignments_become_frame_wide_defaults() {
    let mut e = engine("function frame(t,f) mode=7; brightness=8; bg[1].scroll.x=10; m7.a=2.5 end");
    let lt = e.frame(0.0, 0).unwrap();
    assert_eq!(lt.rows.len(), 224);
    for y in [0usize, 100, 223] {
        assert_eq!(lt.rows[y].mode, 7);
        assert_eq!(lt.rows[y].brightness, 8);
        assert_eq!(lt.rows[y].bg[0].scroll_x, 10); // whole-px absolute
        assert_eq!(lt.rows[y].m7.a, 640); // 2.5 in Q8
    }
}

#[test]
fn cgram_and_obj_writes_land_in_memory() {
    let mut e = engine(
        "function frame(t,f) \
           cgram[0] = rgb(255,0,0); \
           obj.sheet = 'sprites'; \
           obj[0].tile=4; obj[0].x=120; obj[0].pal=2; obj[0].on=true \
         end",
    );
    e.frame(0.0, 0).unwrap();
    let m = e.memory();
    assert_eq!(m.cgram[0], rgb15(255, 0, 0));
    assert_eq!(m.obj_sheet.as_deref(), Some("sprites"));
    assert_eq!(m.oam[0].tile, 4);
    assert_eq!(m.oam[0].x, 120);
    assert_eq!(m.oam[0].pal, 2);
    assert!(m.oam[0].on);
    assert!(!m.oam[1].on);
}

#[test]
fn hdma_hook_overrides_only_covered_scanlines_and_varies_per_line() {
    let mut e =
        engine("function frame(t,f) mode=7; hdma(96,223, function(y) m7.a = (y-95)*2 end) end");
    let lt = e.frame(0.0, 0).unwrap();
    assert_eq!(lt.rows[50].m7.a, 256); // default 1.0 (uncovered), Q8
    assert_eq!(lt.rows[96].m7.a, 512); // (96-95)*2 = 2.0, Q8
    assert_eq!(lt.rows[100].m7.a, 2560); // (100-95)*2 = 10.0, Q8
    assert_eq!(lt.rows[223].mode, 7);
}

#[test]
fn later_hook_wins_on_overlap() {
    let mut e = engine(
        "function frame(t,f) \
           hdma(0,223, function(y) brightness=4 end); \
           hdma(0,223, function(y) brightness=9 end) \
         end",
    );
    let lt = e.frame(0.0, 0).unwrap();
    assert_eq!(lt.rows[10].brightness, 9);
}

#[test]
fn parse_error_reports_message_and_line() {
    let mut e = LuaEngine::new();
    let err = e
        .set_source("function frame(t,f)\n  mode = = 7\nend")
        .unwrap_err();
    assert_eq!(err.line, Some(2));
    assert!(!err.message.is_empty());
}

#[test]
fn dusk_parallax_acceptance() {
    let src = r#"
local SPEED = 12
function frame(t, f)
  mode = 1; brightness = 15
  bg[1].source = "sky";   bg[2].source = "hills"
  bg[1].scroll.x = t * SPEED
  bg[2].scroll.x = t * SPEED * 3
  for i = 0, 7 do cgram[0x40 + i] = hsl((t*40 + i*12) % 360, 0.6, 0.5) end
  obj[0].tile = 4; obj[0].pal = 2; obj[0].x = 120; obj[0].y = 132 + sin(t*3) * 4
end
"#;
    let mut e = engine(src);
    let lt = e.frame(1.0, 0).unwrap();
    let r = &lt.rows[0];
    assert_eq!(r.mode, 1);
    assert_eq!(r.brightness, 15);
    assert_eq!(r.bg[0].source.as_deref(), Some("sky"));
    assert_eq!(r.bg[1].source.as_deref(), Some("hills"));
    assert_eq!(r.bg[0].scroll_x, 12); // t*SPEED = 12, whole px
    assert_eq!(r.bg[1].scroll_x, 36); // t*SPEED*3 = 36, whole px
    let m = e.memory();
    assert_ne!(m.cgram[0x40], 0); // color-cycle wrote palette
    assert_eq!(m.oam[0].tile, 4);
    assert_eq!(m.oam[0].pal, 2);
    assert_eq!(m.oam[0].x, 120);
    // Lua: 132 + sin(1.0*3)*4 ≈ 132.56 -> sprite_y rounds to 133
    assert_eq!(m.oam[0].y, 133);
}

#[test]
fn mode7_floor_acceptance() {
    let src = r#"
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
    let mut e = engine(src);
    let lt = e.frame(1.0, 0).unwrap();
    // Above the horizon: frame-wide defaults (mode 7, identity-ish m7.a default 1.0).
    assert_eq!(lt.rows[0].mode, 7);
    assert_eq!(lt.rows[0].m7.a, 256); // default 1.0, Q8
    // On the horizon line and below: per-scanline affine.
    assert_eq!(lt.rows[96].m7.a, 16384); // 64/(96-95) = 64.0, Q8
    assert_eq!(lt.rows[96].m7.d, 16384);
    assert_eq!(lt.rows[96].m7.cx, 128); // whole-px center
    assert_eq!(lt.rows[120].m7.a, 655); // 64/25 = 2.56 -> round(2.56*256)
    assert_eq!(lt.rows[120].bg[0].scroll_y, 205); // 80*(64/25) = 204.8 -> whole px
    assert_eq!(e.memory().obj_sheet, None);
}

#[test]
fn vram_raw_word_poke_lands_in_memory() {
    let mut e = engine("function frame(t,f) vram[0]=0xbeef; vram[0x7fff]=0x1234 end");
    e.frame(0.0, 0).unwrap();
    let m = e.memory();
    assert_eq!(m.vram[0], 0xbeef);
    assert_eq!(m.vram[0x7fff], 0x1234);
    assert_eq!(m.vram[1], 0); // untouched
}

#[test]
fn bg_map_cell_packs_tilemap_word() {
    let mut e = engine(
        "function frame(t,f) bg[1].map_base=0; \
         bg[1].map[0]={} bg[1].map[0][0]={tile=5,pal=3,prio=1,flip_x=true,flip_y=false}; \
         bg[1].map[1]={} bg[1].map[1][2]={tile=1} end",
    );
    e.frame(0.0, 0).unwrap();
    let m = e.memory();
    // (col=0,row=0): 5 | 3<<10 | 1<<13 | 1<<14
    assert_eq!(m.vram[0], 5 | (3 << 10) | (1 << 13) | (1 << 14));
    // (col=1,row=2): addr = row*32 + col = 65
    assert_eq!(m.vram[2 * 32 + 1], 1);
}

#[test]
fn m7_map_and_char_write_correct_lanes() {
    let mut e = engine("function frame(t,f) m7.map[0]={} m7.map[0][1]=7; m7pixel(2, 3, 1, 200) end");
    e.frame(0.0, 0).unwrap();
    let m = e.memory();
    assert_eq!(m.vram[1] & 0x00ff, 7); // map low byte at ty=0,tx=1
    assert_eq!(m.vram[1] >> 8, 0); // char lane untouched
    assert_eq!(m.vram[2 * 64 + 11] >> 8, 200); // char high byte, off = 1*8 + 3
    assert_eq!(m.vram[2 * 64 + 11] & 0x00ff, 0); // map lane untouched
}

#[test]
fn source_triggers_tile_bg_import_into_vram_cgram() {
    let mut e = LuaEngine::new();
    // 16x8: left 12px red, right 4px blue -> two tiles, one palette.
    let mut rgba = Vec::new();
    for _y in 0..8 {
        for x in 0..16 {
            rgba.extend_from_slice(if x < 12 { &[255, 0, 0, 255] } else { &[0, 0, 255, 255] });
        }
    }
    e.upload_asset("sky".into(), 16, 8, rgba);
    e.set_source("function frame(t,f) mode=1; bg[1].source='sky' end").unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    let m = e.memory();
    assert_eq!(m.cgram[1], rgb15(255, 0, 0)); // sub-palette 0 idx 1
    assert_eq!(m.cgram[2], rgb15(0, 0, 255)); // idx 2
    assert_eq!(m.vram[0], 0x0001); // tilemap cell 0 = tile 1
    assert_eq!(m.vram[1], 0x0002); // cell 1 = tile 2
    assert_eq!(m.vram[0x1000 + 16], 0x00ff); // char base 0x1000, tile 1 row0 plane0
    assert_eq!(lt.rows[0].bg[0].char_base, 0x1000); // echoed binding register
    assert_eq!(lt.rows[0].bg[0].map_base, 0x0000);
    assert_eq!(e.import_reports().len(), 1); // budget surfaced
}

#[test]
fn source_triggers_mode7_import_interleaved() {
    let mut e = LuaEngine::new();
    let rgba = [255u8, 0, 0, 255].repeat(64); // 8x8 solid red = 256 bytes
    e.upload_asset("track".into(), 8, 8, rgba);
    e.set_source("function frame(t,f) mode=7; bg[1].source='track' end").unwrap();
    e.frame(0.0, 0).unwrap();
    let m = e.memory();
    assert_eq!(m.cgram[1], rgb15(255, 0, 0));
    assert_eq!(m.vram[0] >> 8, 1); // char lane index 1
    assert_eq!(m.vram[0] & 0x00ff, 0); // map cell 0 = tile 0
}

#[test]
fn manual_pokes_override_source_import() {
    let mut e = LuaEngine::new();
    let mut rgba = Vec::new();
    for _ in 0..64 {
        rgba.extend_from_slice(&[255, 0, 0, 255]);
    }
    e.upload_asset("sky".into(), 8, 8, rgba);
    e.set_source(
        "function frame(t,f) mode=1; bg[1].source='sky'; vram[0]=0xabcd; cgram[1]=rgb(0,255,0) end",
    )
    .unwrap();
    e.frame(0.0, 0).unwrap();
    assert_eq!(e.memory().vram[0], 0xabcd); // raw vram wins over the tilemap
    assert_eq!(e.memory().cgram[1], rgb15(0, 255, 0)); // cgram poke wins over import
}

#[test]
fn obj_sheet_triggers_obj_import_into_vram_and_obj_cgram() {
    let mut e = LuaEngine::new();
    // 16x8: left 12px red, right 4px blue -> two OBJ tiles, one OBJ palette (8).
    let mut rgba = Vec::new();
    for _y in 0..8 {
        for x in 0..16 {
            rgba.extend_from_slice(if x < 12 { &[255, 0, 0, 255] } else { &[0, 0, 255, 255] });
        }
    }
    e.upload_asset("hero".into(), 16, 8, rgba);
    e.set_source("function frame(t,f) mode=1; obj.sheet='hero'; obj.char_base=0x2000 end").unwrap();
    e.frame(0.0, 0).unwrap();
    let m = e.memory();
    // OBJ palettes land in CGRAM base 128 (sub-palette 8, indices 1/2).
    assert_eq!(m.cgram[129], rgb15(255, 0, 0));
    assert_eq!(m.cgram[130], rgb15(0, 0, 255));
    // OBJ char data lands at the snapped OBJ char base (tile 0 = reserved blank,
    // tile 1 = all-red -> row0 plane0 = 0x00ff at char_base + 1*16).
    assert_eq!(m.vram[0x2000], 0); // reserved blank tile 0
    assert_eq!(m.vram[0x2000 + 16], 0x00ff);
    assert_eq!(m.obsel.char_base, 0x2000); // obsel echoes the same snapped base
    // Budget report surfaced through the shared import_reports() path.
    assert_eq!(e.import_reports().len(), 1);
}

#[test]
fn scanline_is_an_alias_for_hdma() {
    let mut e = engine("function frame(t,f) scanline(0,223, function(y) brightness=3 end) end");
    let lt = e.frame(0.0, 0).unwrap();
    assert_eq!(lt.rows[10].brightness, 3);
}

#[test]
fn init_runs_once_and_seeds_globals() {
    // `init` sets a global that `frame` never touches; it must survive into the
    // frame-wide defaults (proves init ran and globals are shared).
    let mut e = engine("function init() mode = 3 end\nfunction frame(t,f) end");
    let lt = e.frame(0.0, 0).unwrap();
    assert_eq!(lt.rows[0].mode, 3);
}

#[test]
fn sticky_defaults_survive_hook_mutation_across_frames() {
    // `m7.a` is set only on frame 0; a hook stomps it to 99 on lines 100..223.
    // After frame 0's build, globals must be restored to the frame-wide default
    // (5), so frame 1 (which does not re-set m7.a) sees 5 on uncovered lines.
    let src = "function frame(t,f)\n  if f == 0 then m7.a = 5 end\n  hdma(100,223, function(y) m7.a = 99 end)\nend";
    let mut e = engine(src);
    let lt0 = e.frame(0.0, 0).unwrap();
    assert_eq!(lt0.rows[0].m7.a, 1280); // uncovered: frame-wide default 5.0, Q8
    assert_eq!(lt0.rows[150].m7.a, 25344); // covered: hook override 99.0, Q8
    let lt1 = e.frame(0.0, 1).unwrap();
    assert_eq!(lt1.rows[0].m7.a, 1280); // sticky restore held; not stomped to 99
    assert_eq!(lt1.rows[150].m7.a, 25344);
}

#[test]
fn runtime_error_in_hook_propagates() {
    let mut e = engine("function frame(t,f) hdma(0,10, function(y) error('boom') end) end");
    let err = match e.frame(0.0, 0) {
        Err(err) => err,
        Ok(_) => panic!("expected a runtime error from the hook"),
    };
    assert!(err.message.contains("boom"), "got: {}", err.message);
}
