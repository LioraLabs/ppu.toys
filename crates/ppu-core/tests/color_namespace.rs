//! Friendly `color` namespace over CGWSEL/CGADSUB/COLDATA.
//! Coexistence contract: friendly is authoritative when moved (sets AND
//! clears its bits), raw mnemonics stay valid, both-off is byte-identical.
use ppu_core::LuaEngine;

#[test]
fn friendly_only_packs_all_three_registers() {
    let mut e = LuaEngine::new();
    e.set_source(
        "function frame(t,f) \
           color.op = 'sub'; color.half = true; \
           color.on.bg1 = true; color.on.obj = true; \
           color.addend = 'sub'; color.region = 'inside'; \
           color.fixed = rgb(255,0,255) \
         end",
    )
    .unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    let row = &lt.rows[0];
    assert_eq!(row.cgadsub, 0x80 | 0x40 | 0x10 | 0x01); // sub + half + obj + bg1
    assert_eq!(row.cgwsel, 0x02 | 0x10); // addend=sub, region=inside (prevent-outside)
    assert_eq!(row.coldata, (31 << 10) | 31); // rgb(255,0,255) -> BGR555
}

#[test]
fn raw_only_stays_valid_including_inside_hooks() {
    // The hook path is the hard case: write_state re-baselines the friendly
    // fields before each hook, so a raw CGADSUB write inside the hook must
    // NOT be clobbered by the (unchanged) friendly fields.
    let mut e = LuaEngine::new();
    e.set_source(
        "function frame(t,f) \
           CGWSEL = 0xC2; CGADSUB = 0x41; COLDATA = rgb(0,255,0); \
           hdma(100, 120, function(y) CGADSUB = 0x81 end) \
         end",
    )
    .unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    assert_eq!(lt.rows[0].cgwsel, 0xC2);
    assert_eq!(lt.rows[0].cgadsub, 0x41);
    assert_eq!(lt.rows[0].coldata, 31 << 5);
    assert_eq!(lt.rows[100].cgadsub, 0x81); // raw write inside the hook wins
    assert_eq!(lt.rows[100].cgwsel, 0xC2); // untouched registers persist
}

#[test]
fn both_off_is_byte_identical_to_power_on() {
    let mut e = LuaEngine::new();
    e.set_source("function frame(t,f) mode = 1 end").unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    let row = &lt.rows[0];
    assert_eq!((row.cgwsel, row.cgadsub, row.coldata), (0, 0, 0));
}

#[test]
fn friendly_owns_moved_bits_sets_and_clears_over_raw() {
    // f=0 latches raw 0xFF (sticky). f=1 moves op/half through the friendly
    // fields: those bits CLEAR (authoritative), the six enables keep the raw byte.
    let mut e = LuaEngine::new();
    e.set_source(
        "function frame(t,f) \
           if f == 0 then CGADSUB = 0xFF end \
           if f == 1 then color.op = 'add'; color.half = false end \
         end",
    )
    .unwrap();
    assert_eq!(e.frame(0.0, 0).unwrap().rows[0].cgadsub, 0xFF);
    assert_eq!(e.frame(0.0, 1).unwrap().rows[0].cgadsub, 0x3F);
}

#[test]
fn same_frame_raw_write_plus_friendly_change_composes() {
    // Milestone contract: friendly folds last, wins on overlapping bits;
    // raw keeps the bits friendly didn't move.
    let mut e = LuaEngine::new();
    e.set_source("function frame(t,f) CGADSUB = 0x0F; color.op = 'sub' end")
        .unwrap();
    assert_eq!(e.frame(0.0, 0).unwrap().rows[0].cgadsub, 0x8F);
}

#[test]
fn write_state_round_trip_exposes_friendly_fields_to_hooks() {
    // frame() authors via friendly fields; the hook re-baseline (write_state)
    // must decode the packed bytes back into `color` so the hook can READ them.
    let mut e = LuaEngine::new();
    e.set_source(
        "function frame(t,f) \
           color.op = 'sub'; color.on.bg1 = true; color.region = 'outside'; \
           color.fixed = rgb(255,0,0); \
           hdma(10, 20, function(y) \
             if color.op == 'sub' and color.on.bg1 and color.region == 'outside' \
                and color.fixed == 31 then color.half = true end \
           end) \
         end",
    )
    .unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    assert_eq!(lt.rows[0].cgadsub, 0x81);
    assert_eq!(lt.rows[10].cgadsub, 0xC1); // hook saw every round-tripped field
    assert_eq!(lt.rows[10].cgwsel & 0x30, 0x20);
    assert_eq!(lt.rows[10].coldata, 31);
}

#[test]
fn friendly_state_is_sticky_across_frames() {
    // Assigned only on f=0; the end-of-frame write_state(defaults) plus the
    // next frame's read_state must reproduce identical bytes (round-trip).
    let mut e = LuaEngine::new();
    e.set_source(
        "function frame(t,f) \
           if f == 0 then color.op='sub'; color.on.backdrop=true; color.addend='sub' end \
         end",
    )
    .unwrap();
    let a = e.frame(0.0, 0).unwrap();
    let (w0, a0, c0) = (a.rows[0].cgwsel, a.rows[0].cgadsub, a.rows[0].coldata);
    assert_eq!(a0, 0xA0);
    assert_eq!(w0 & 0x02, 0x02);
    let b = e.frame(0.0, 1).unwrap();
    assert_eq!(
        (b.rows[0].cgwsel, b.rows[0].cgadsub, b.rows[0].coldata),
        (w0, a0, c0)
    );
}

#[test]
fn unrecognized_enum_values_fall_back_to_raw() {
    let mut e = LuaEngine::new();
    e.set_source("function frame(t,f) CGADSUB = 0x80; color.op = 'banana' end")
        .unwrap();
    assert_eq!(e.frame(0.0, 0).unwrap().rows[0].cgadsub, 0x80);
}

#[test]
fn cgwsel_non_owned_bits_survive_friendly_moves_in_both_directions() {
    // Mask discipline: friendly owns CGWSEL 0x32 only. Bit 0 (direct_color)
    // and bits 6-7 (clip-to-black) must survive a friendly MOVE — both when
    // friendly SETS its bits and when it CLEARS them. (Bit 0 persists via the
    // direct_color OR-fold precedent: write_state mirrors it back into the
    // global, which keeps it set.)
    let mut e = LuaEngine::new();
    e.set_source(
        "function frame(t,f) \
           if f == 0 then CGWSEL = 0xC3 end \
           if f == 1 then color.region = 'never' end \
           if f == 2 then color.region = 'everywhere'; color.addend = 'fixed' end \
         end",
    )
    .unwrap();
    assert_eq!(e.frame(0.0, 0).unwrap().rows[0].cgwsel, 0xC3);
    // Set direction: region bits 4-5 set, bits 0/6/7 untouched.
    assert_eq!(e.frame(0.0, 1).unwrap().rows[0].cgwsel, 0xF3);
    // Clear direction: region + addend cleared, bits 0/6/7 still intact.
    assert_eq!(e.frame(0.0, 2).unwrap().rows[0].cgwsel, 0xC1);
}
