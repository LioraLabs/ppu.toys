//! Friendly `screen` namespace over TM/TS ($212C/$212D).
//! Coexistence contract: friendly is authoritative when moved (sets AND
//! clears its bits), raw mnemonics stay valid, both-off is byte-identical.
use ppu_core::LuaEngine;

#[test]
fn friendly_only_packs_both_registers() {
    // Power-on TM = 0x1f (all five on): clear bg1+obj on main, set bg3 on sub.
    let mut e = LuaEngine::new();
    e.set_source(
        "function frame(t,f) \
           screen.main.bg1 = false; screen.main.obj = false; \
           screen.sub.bg3 = true \
         end",
    )
    .unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    assert_eq!(lt.rows[0].tm, 0x0e); // 0x1f minus bg1(0x01) and obj(0x10)
    assert_eq!(lt.rows[0].ts, 0x04);
}

#[test]
fn raw_only_stays_valid_including_inside_hooks() {
    // The hook path is the hard case: write_state re-baselines the friendly
    // fields before each hook, so a raw TM write inside the hook must NOT
    // be clobbered by the (unchanged) friendly fields.
    let mut e = LuaEngine::new();
    e.set_source(
        "function frame(t,f) \
           TM = 0x03; TS = 0x10; \
           hdma(100, 120, function(y) TM = 0x11 end) \
         end",
    )
    .unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    assert_eq!(lt.rows[0].tm, 0x03);
    assert_eq!(lt.rows[0].ts, 0x10);
    assert_eq!(lt.rows[100].tm, 0x11); // raw write inside the hook wins
    assert_eq!(lt.rows[100].ts, 0x10); // untouched register persists
}

#[test]
fn both_off_is_byte_identical_to_power_on() {
    let mut e = LuaEngine::new();
    e.set_source("function frame(t,f) mode = 1 end").unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    assert_eq!((lt.rows[0].tm, lt.rows[0].ts), (0x1f, 0x00));
}

#[test]
fn friendly_owns_moved_bits_sets_and_clears_over_raw() {
    // f=0 latches raw TM=0x00 (sticky). f=1 sets bg2 via friendly: that bit
    // SETS (authoritative); the other four keep the raw byte. f=2 clears it.
    let mut e = LuaEngine::new();
    e.set_source(
        "function frame(t,f) \
           if f == 0 then TM = 0x00 end \
           if f == 1 then screen.main.bg2 = true end \
           if f == 2 then screen.main.bg2 = false end \
         end",
    )
    .unwrap();
    assert_eq!(e.frame(0.0, 0).unwrap().rows[0].tm, 0x00);
    assert_eq!(e.frame(0.0, 1).unwrap().rows[0].tm, 0x02);
    assert_eq!(e.frame(0.0, 2).unwrap().rows[0].tm, 0x00);
}

#[test]
fn same_frame_raw_write_plus_friendly_change_composes() {
    // Milestone contract: friendly folds last, wins on overlapping bits;
    // raw keeps the bits friendly didn't move.
    let mut e = LuaEngine::new();
    e.set_source("function frame(t,f) TS = 0x01; screen.sub.obj = true end")
        .unwrap();
    assert_eq!(e.frame(0.0, 0).unwrap().rows[0].ts, 0x11);
}

#[test]
fn write_state_round_trip_exposes_friendly_fields_to_hooks() {
    // frame() authors via friendly fields; the hook re-baseline (write_state)
    // must decode TM/TS back into `screen` so the hook can READ them.
    let mut e = LuaEngine::new();
    e.set_source(
        "function frame(t,f) \
           screen.main.bg1 = false; screen.sub.bg2 = true; \
           hdma(10, 20, function(y) \
             if screen.main.bg1 == false and screen.main.bg2 == true \
                and screen.sub.bg2 == true and screen.sub.obj == false \
                then screen.sub.obj = true end \
           end) \
         end",
    )
    .unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    assert_eq!(lt.rows[0].tm, 0x1e);
    assert_eq!(lt.rows[0].ts, 0x02);
    assert_eq!(lt.rows[10].ts, 0x12); // hook saw every round-tripped field
}

#[test]
fn friendly_state_is_sticky_across_frames() {
    // Assigned only on f=0; the end-of-frame write_state(defaults) plus the
    // next frame's read_state must reproduce identical bytes (round-trip).
    let mut e = LuaEngine::new();
    e.set_source(
        "function frame(t,f) \
           if f == 0 then screen.main.bg4 = false; screen.sub.bg1 = true end \
         end",
    )
    .unwrap();
    let a = e.frame(0.0, 0).unwrap();
    assert_eq!((a.rows[0].tm, a.rows[0].ts), (0x17, 0x01));
    let b = e.frame(0.0, 1).unwrap();
    assert_eq!((b.rows[0].tm, b.rows[0].ts), (0x17, 0x01));
}

#[test]
fn raw_upper_bits_do_not_corrupt_the_friendly_fold() {
    // Mask discipline: friendly owns TM/TS bits 0-4 only (SCREEN_MASK).
    // TM/TS are 5-bit registers — RegRow build quantizes with screen_mask
    // (v & 0x1f), so raw junk in bits 5-7 is never observable in rows[].
    // What IS observable: that junk must not perturb WHICH bits the fold
    // sees as friendly-moved (set and clear directions), and the sticky
    // raw TM global must keep its upper bits across frames.
    let mut e = LuaEngine::new();
    e.set_source(
        "function frame(t,f) \
           if f == 0 then TM = 0xE3 end \
           if f == 1 then screen.main.bg3 = true end \
           if f == 2 and TM == 0xE7 then \
             screen.main.bg1 = false; screen.main.bg2 = false \
           end \
         end",
    )
    .unwrap();
    assert_eq!(e.frame(0.0, 0).unwrap().rows[0].tm, 0x03);
    assert_eq!(e.frame(0.0, 1).unwrap().rows[0].tm, 0x07);
    // The f=2 guard proves the sticky raw TM global kept bits 5-7 through
    // the f=1 friendly move (the fold masks its diff to bits 0-4): the
    // clears only run — and rows[0].tm only drops to 0x04 — if TM == 0xE7.
    assert_eq!(e.frame(0.0, 2).unwrap().rows[0].tm, 0x04);
}

#[test]
fn non_boolean_assignment_falls_back_to_raw() {
    let mut e = LuaEngine::new();
    e.set_source("function frame(t,f) TM = 0x05; screen.main.bg1 = 'banana' end")
        .unwrap();
    assert_eq!(e.frame(0.0, 0).unwrap().rows[0].tm, 0x05);
}
