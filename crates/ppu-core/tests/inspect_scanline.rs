use ppu_core::{inspect_scanline, trace_bg_screen, LuaEngine};

#[test]
fn inspector_resolves_registers_palette_and_sprite_limits_on_the_requested_line() {
    let mut engine = LuaEngine::new();
    engine
        .set_sources(&[(
            "main.lua".into(),
            r#"
function frame()
  mode = 1
  TM = 1
  CGADSUB = 0
  cgram[129] = 31
  obj[0].on = true
  obj[0].y = 20
  obj[0].tile = 0
  for i = 1, 39 do obj[i].on = true; obj[i].y = 100 end
  hdma(100, 107, function(y)
    TM = 16
    CGADSUB = 0x90
    brightness = 7
    bg[1].scroll.x = y
    m7.a = -0.5
    WH0 = y
    cgram[129] = 992
  end)
end
"#
            .into(),
        )])
        .unwrap();
    let lt = engine.frame(0.0, 0).unwrap();
    let mem = engine.memory();
    let top = inspect_scanline(&lt, mem, 20).unwrap();
    let line = inspect_scanline(&lt, mem, 100).unwrap();
    let after = inspect_scanline(&lt, mem, 108).unwrap();
    let reg = |addr| {
        line.registers
            .iter()
            .find(|r| r.addr == addr)
            .unwrap()
            .value
    };
    assert_eq!(line.scanline, 100);
    assert_eq!(reg(0x212c), 16);
    assert_eq!(reg(0x2131), 0x90);
    assert_eq!(reg(0x2100), 7);
    assert_eq!(reg(0x210d), 100);
    assert_eq!(reg(0x211b), 0xff80);
    assert_eq!(reg(0x2126), 100);
    assert_eq!(top.cgram[129], 31);
    assert_eq!(line.cgram[129], 992);
    assert_eq!(after.cgram[129], 31);
    assert_eq!(mem.cgram[129], 31, "inspection never mutates frame memory");
    assert_eq!(top.obj_line.sprites, vec![0]);
    assert_eq!(line.obj_line.sprite_count, 39);
    assert_eq!(line.obj_line.sprites.len(), 32);
    assert!(line.obj_line.range_over);
    assert!(!line.obj_line.sprites.contains(&0));
    assert!(after.obj_line.sprites.is_empty());
    assert!(inspect_scanline(&lt, mem, 224).is_none());
}

#[test]
fn traced_pixel_uses_the_scanlines_palette() {
    let mut mem = ppu_core::Memory::default();
    mem.vram[0] = 0x00ff; // tile 0, solid color 1
    mem.cgram[1] = 31;
    let mut defaults = ppu_core::LineTableRow::default();
    defaults.bg[0].map_base = 0x1000;
    defaults.cgram = vec![(1, 992)];
    let row = ppu_core::RegRow::from(&defaults);
    let trace = trace_bg_screen(&row, &mem, 0, 0, 0).unwrap();
    assert_eq!(trace.pixel.unwrap().bgr555, 992);
    assert_eq!(mem.cgram[1], 31);
}
