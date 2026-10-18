use ppu_core::LuaEngine;
#[test]
fn register_raster_preserves_hud_irq_mosaic_palettes_and_later_overrides() {
    let mut e = LuaEngine::new();
    e.set_source(
        r#"
  function frame(t,f)
    local a={palette={}};for i=0,63 do a[i]=0 end
    for i=0,255 do a.palette[i]=0 end
    a[0]=15;a[4]=9;a[58]=19;a[14]=7;a.palette[0]=31
    local b={palette={}};for i=0,63 do b[i]=a[i]end
    for i=0,255 do b.palette[i]=a.palette[i]end
    b[14]=120;b.palette[0]=1023
    local rows={};for y=0,223 do rows[y]=y<31 and a or b end
    raster(rows)
    hdma(40,40,function(y)brightness=3 end)
  end
 "#,
    )
    .unwrap();
    let rows = e.frame(0.0, 0).unwrap().rows;
    assert_eq!(e.memory().cgram[0], 31);
    assert_eq!(rows[30].bg[0].scroll_x, 7);
    assert_eq!(rows[31].bg[0].scroll_x, 120);
    // Raster rows are hardware register values, used as they are.
    assert_eq!(rows[0].bg[0].scroll_y, 0);
    assert_eq!(rows[31].cgram, vec![(0, 1023)]);
    assert_eq!(rows[40].brightness, 3);
    assert_eq!(rows[41].brightness, 15);
}
