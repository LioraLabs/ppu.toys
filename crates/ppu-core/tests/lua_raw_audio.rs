use ppu_core::LuaEngine;

#[test]
fn driver_events_preserve_upload_addresses_register_order_and_sample_offsets() {
    let mut e = LuaEngine::new();
    e.set_source(
        r#"
      function frame(t,f)
        dsp.raw=true
        dsp.events={
          {at=0,reset=true},
          {at=0,addr=0x8000,data="\001\129\010\129"},
          {at=0,addr=0x8100,data={3,0,0,0,0,0,0,0,0}},
          {at=0,reg=0x5d,value=0x80},
          {at=0,reg=0x6d,value=0x60},
          {at=0,reg=0x7d,value=2},
          {at=0,reg=0x6c,value=0x20},
          {at=128,reg=0x0c,value=127},
          {at=128,reg=0x1c,value=126},
          {at=512,reg=0x0c,value=99},
        }
      end
    "#,
    )
    .unwrap();
    e.frame(0.0, 0).unwrap();
    assert_eq!(&e.aram()[0x8000..0x8004], &[1, 129, 10, 129]);
    assert_eq!(e.aram()[0x8100], 3);
    assert_eq!(e.dsp().read(0x5d), 0x80);
    assert_eq!(e.dsp().read(0x6d), 0x60);
    assert_eq!(e.dsp().read(0x0c), 99);
    assert_eq!(e.dsp().read(0x1c), 126);
    assert_eq!(e.audio().len(), 1066);
    // An invalid late event must not partly apply the earlier valid one.
    e.set_source(
        r#"function frame(t,f)
      dsp.raw=true;dsp.events={{at=0,reg=0x0c,value=1},{at=600,reg=0x0c,value=2}}
    end"#,
    )
    .unwrap();
    assert!(e.frame(1.0 / 60.0, 1).is_err());
    assert_eq!(e.dsp().read(0x0c), 99);
}
