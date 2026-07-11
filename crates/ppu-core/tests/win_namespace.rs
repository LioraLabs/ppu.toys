//! Friendly `win` namespace over the window registers (WH0-3, W12SEL/W34SEL/
//! WOBJSEL, WBGLOG/WOBJLOG, TMW/TSW).
//! Coexistence contract: friendly is authoritative when moved (sets AND
//! clears its bits), raw mnemonics stay valid, both-off is byte-identical.
use ppu_core::LuaEngine;

#[test]
fn friendly_only_packs_every_register() {
    let mut e = LuaEngine::new();
    e.set_source(
        "function frame(t,f) \
           win.w1.lo = 40; win.w1.hi = 120; win.w2.lo = 8; win.w2.hi = 248; \
           win.bg1.w1 = true; win.bg1.main = true; \
           win.bg2.w2 = true; win.bg2.invert = true; win.bg2.combine = 'AND'; win.bg2.sub = true; \
           win.obj.w1 = true; win.obj.combine = 'XOR'; win.obj.main = true; \
           win.color.w1 = true; win.color.w2 = true; win.color.combine = 'XNOR' \
         end",
    )
    .unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    let r = &lt.rows[0];
    assert_eq!((r.wh0, r.wh1, r.wh2, r.wh3), (40, 120, 8, 248));
    assert_eq!(r.w12sel, 0xd2); // bg1 W1-enable (0x2) | bg2 (W2-enable + both inverts = 0xd) << 4
    assert_eq!(r.w34sel, 0x00);
    assert_eq!(r.wobjsel, 0xa2); // obj W1-enable | color (W1+W2 enables = 0xa) << 4
    assert_eq!(r.wbglog, 0x04); // bg2 slot = AND(1) << 2
    assert_eq!(r.wobjlog, 0x0e); // obj slot = XOR(2) | color slot = XNOR(3) << 2
    assert_eq!(r.tmw, 0x11); // bg1 + obj
    assert_eq!(r.tsw, 0x02); // bg2
}

#[test]
fn raw_only_stays_valid_including_inside_hooks() {
    // The hook path is the hard case: write_state re-baselines the friendly
    // fields before each hook, so a raw write inside the hook must NOT be
    // clobbered by the (unchanged) friendly fields.
    let mut e = LuaEngine::new();
    e.set_source(
        "function frame(t,f) \
           WH0 = 10; WH3 = 200; W12SEL = 0x21; WOBJSEL = 0x84; \
           WBGLOG = 0x1B; WOBJLOG = 0xF3; TMW = 0x0A; \
           hdma(100, 120, function(y) W34SEL = 0x55; WH1 = 99 end) \
         end",
    )
    .unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    let r = &lt.rows[0];
    assert_eq!((r.wh0, r.wh1, r.wh3), (10, 0, 200));
    assert_eq!((r.w12sel, r.w34sel, r.wobjsel), (0x21, 0x00, 0x84));
    assert_eq!((r.wbglog, r.wobjlog), (0x1B, 0xF3)); // WOBJLOG bits 4-7 pass through
    assert_eq!(r.tmw, 0x0A);
    let h = &lt.rows[100];
    assert_eq!((h.w34sel, h.wh1), (0x55, 99)); // raw writes inside the hook win
    assert_eq!((h.w12sel, h.wobjlog), (0x21, 0xF3)); // untouched registers persist
}

#[test]
fn both_off_is_byte_identical_to_power_on() {
    let mut e = LuaEngine::new();
    e.set_source("function frame(t,f) mode = 1 end").unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    let r = &lt.rows[0];
    assert_eq!((r.wh0, r.wh1, r.wh2, r.wh3), (0, 0, 0, 0));
    assert_eq!((r.w12sel, r.w34sel, r.wobjsel), (0, 0, 0));
    assert_eq!((r.wbglog, r.wobjlog, r.tmw, r.tsw), (0, 0, 0, 0));
}

#[test]
fn friendly_owns_moved_bits_sets_and_clears_over_raw() {
    // f=0 latches raw bytes (sticky). f=1 clears bg1's W1-enable, inverts and
    // TMW bit through the friendly fields: those bits CLEAR (authoritative);
    // bg1's W2-enable and the whole bg2 nibble keep the raw byte. f=2 clears
    // the remaining enable.
    let mut e = LuaEngine::new();
    e.set_source(
        "function frame(t,f) \
           if f == 0 then W12SEL = 0xFF; TMW = 0x1F end \
           if f == 1 then win.bg1.w1 = false; win.bg1.invert = false; win.bg1.main = false end \
           if f == 2 then win.bg1.w2 = false end \
         end",
    )
    .unwrap();
    let a = e.frame(0.0, 0).unwrap();
    assert_eq!((a.rows[0].w12sel, a.rows[0].tmw), (0xFF, 0x1F));
    let b = e.frame(0.0, 1).unwrap();
    assert_eq!((b.rows[0].w12sel, b.rows[0].tmw), (0xF8, 0x1E));
    let c = e.frame(0.0, 2).unwrap();
    assert_eq!((c.rows[0].w12sel, c.rows[0].tmw), (0xF0, 0x1E));
}

#[test]
fn same_frame_raw_write_plus_friendly_change_composes() {
    // Friendly folds last and wins on the bits it moved; raw keeps the rest —
    // including WOBJLOG's unused upper nibble, which win never owns.
    let mut e = LuaEngine::new();
    e.set_source("function frame(t,f) WOBJLOG = 0xF0; win.obj.combine = 'AND' end")
        .unwrap();
    assert_eq!(e.frame(0.0, 0).unwrap().rows[0].wobjlog, 0xF1);
}

#[test]
fn wh_edges_fold_as_whole_scalars_not_bit_blends() {
    // Same-frame conflict on an edge: the friendly value replaces the byte
    // outright (the COLDATA scalar precedent) — a naive bitwise fold would
    // blend 10 and 40 into 42, a value neither side wrote.
    let mut e = LuaEngine::new();
    e.set_source("function frame(t,f) WH0 = 10; win.w1.lo = 40 end")
        .unwrap();
    assert_eq!(e.frame(0.0, 0).unwrap().rows[0].wh0, 40);
}

#[test]
fn write_state_round_trip_exposes_friendly_fields_to_hooks() {
    // frame() authors via friendly fields; the hook re-baseline (write_state)
    // must decode the registers back into `win` so the hook can READ them.
    let mut e = LuaEngine::new();
    e.set_source(
        "function frame(t,f) \
           win.bg3.w1 = true; win.bg3.invert = true; win.bg3.combine = 'XOR'; \
           win.bg3.sub = true; win.w2.hi = 180; \
           hdma(10, 20, function(y) \
             if win.bg3.w1 == true and win.bg3.w2 == false \
                and win.bg3.invert == true and win.bg3.combine == 'XOR' \
                and win.bg3.sub == true and win.bg3.main == false \
                and win.w2.hi == 180 then win.bg4.w2 = true end \
           end) \
         end",
    )
    .unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    assert_eq!(lt.rows[0].w34sel, 0x07); // bg3: W1-enable + both inverts
    assert_eq!(lt.rows[0].wbglog, 0x20); // bg3 slot = XOR(2) << 4
    assert_eq!(lt.rows[0].tsw, 0x04);
    assert_eq!(lt.rows[0].wh3, 180);
    assert_eq!(lt.rows[10].w34sel, 0x87); // hook saw every round-tripped field
}

#[test]
fn friendly_state_is_sticky_across_frames() {
    // Assigned only on f=0; the end-of-frame write_state(defaults) plus the
    // next frame's read_state must reproduce identical bytes (round-trip).
    let mut e = LuaEngine::new();
    e.set_source(
        "function frame(t,f) \
           if f == 0 then win.bg2.w1 = true; win.w1.lo = 33 end \
         end",
    )
    .unwrap();
    let a = e.frame(0.0, 0).unwrap();
    assert_eq!((a.rows[0].w12sel, a.rows[0].wh0), (0x20, 33));
    let b = e.frame(0.0, 1).unwrap();
    assert_eq!((b.rows[0].w12sel, b.rows[0].wh0), (0x20, 33));
}

#[test]
fn lone_raw_invert_bit_survives_the_shared_invert_decode() {
    // The SEL nibble has TWO invert bits but the friendly field is ONE bool
    // (inspector granularity): decode is lossy (either bit reads true), so an
    // UNTOUCHED friendly bool must not rewrite a lone raw invert bit — only a
    // moved bool expands to both bits / neither.
    let mut e = LuaEngine::new();
    e.set_source(
        "function frame(t,f) \
           if f == 0 then W12SEL = 0x01 end \
           if f == 2 then win.bg1.invert = false end \
           if f == 3 then win.bg1.invert = true end \
         end",
    )
    .unwrap();
    assert_eq!(e.frame(0.0, 0).unwrap().rows[0].w12sel, 0x01);
    assert_eq!(e.frame(0.0, 1).unwrap().rows[0].w12sel, 0x01); // no phantom diff
    assert_eq!(e.frame(0.0, 2).unwrap().rows[0].w12sel, 0x00); // moved: clears BOTH
    assert_eq!(e.frame(0.0, 3).unwrap().rows[0].w12sel, 0x05); // moved: sets BOTH
}

#[test]
fn raw_upper_bits_do_not_corrupt_the_friendly_fold() {
    // TMW/TSW are 5-bit registers — RegRow build quantizes with screen_mask,
    // so raw junk in bits 5-7 never reaches rows[]. What IS observable: the
    // junk must not perturb WHICH bits the fold sees as friendly-moved, and
    // the sticky raw TMW global must keep its upper bits across frames (the
    // f=2 guard only fires — and tmw only drops to 0x04 — if TMW == 0xE7).
    let mut e = LuaEngine::new();
    e.set_source(
        "function frame(t,f) \
           if f == 0 then TMW = 0xE3 end \
           if f == 1 then win.bg3.main = true end \
           if f == 2 and TMW == 0xE7 then \
             win.bg1.main = false; win.bg2.main = false \
           end \
         end",
    )
    .unwrap();
    assert_eq!(e.frame(0.0, 0).unwrap().rows[0].tmw, 0x03);
    assert_eq!(e.frame(0.0, 1).unwrap().rows[0].tmw, 0x07);
    assert_eq!(e.frame(0.0, 2).unwrap().rows[0].tmw, 0x04);
}

#[test]
fn combine_strings_map_to_the_four_log_slots() {
    let mut e = LuaEngine::new();
    e.set_source(
        "function frame(t,f) \
           win.bg1.combine = 'OR'; win.bg2.combine = 'AND'; \
           win.bg3.combine = 'XOR'; win.bg4.combine = 'XNOR' \
         end",
    )
    .unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    assert_eq!(lt.rows[0].wbglog, 0xE4); // XNOR(3)<<6 | XOR(2)<<4 | AND(1)<<2 | OR(0)
    assert_eq!(lt.rows[0].wobjlog, 0x00);
}

#[test]
fn unrecognized_combine_falls_back_to_raw() {
    let mut e = LuaEngine::new();
    e.set_source("function frame(t,f) WBGLOG = 0xFF; win.bg1.combine = 'NAND' end")
        .unwrap();
    assert_eq!(e.frame(0.0, 0).unwrap().rows[0].wbglog, 0xFF);
}

#[test]
fn non_boolean_assignment_falls_back_to_raw() {
    let mut e = LuaEngine::new();
    e.set_source(
        "function frame(t,f) TMW = 0x05; W12SEL = 0x0F; \
           win.bg1.main = 'banana'; win.bg1.w1 = 17 \
         end",
    )
    .unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    assert_eq!((lt.rows[0].tmw, lt.rows[0].w12sel), (0x05, 0x0F));
}

#[test]
fn color_window_never_stomps_cgwsel() {
    // Overlap contract with the `color` namespace: win.color.*
    // selects WHICH pixels form the color window (WOBJSEL high nibble +
    // WOBJLOG slot); WHERE math is prevented (CGWSEL bits 4-5) stays
    // color.region's — and CGWSEL's raw-only bits 6-7 stay raw.
    let mut e = LuaEngine::new();
    e.set_source(
        "function frame(t,f) \
           CGWSEL = 0xC0; color.region = 'inside'; \
           win.color.w1 = true; win.color.invert = true; win.color.combine = 'AND' \
         end",
    )
    .unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    let r = &lt.rows[0];
    assert_eq!(r.wobjsel, 0x70); // color nibble: W1-enable + both inverts, << 4
    assert_eq!(r.wobjlog, 0x04); // color slot = AND(1) << 2
    assert_eq!(r.cgwsel, 0xD0); // raw bits 6-7 + color.region — win wrote nothing
}

#[test]
fn color_layer_has_no_tmw_tsw_bit() {
    // The hardware has no color-window bit in TMW/TSW; win.color.main/.sub
    // must be inert, not corrupt another layer's bit.
    let mut e = LuaEngine::new();
    e.set_source("function frame(t,f) win.color.main = true; win.color.sub = true end")
        .unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    assert_eq!((lt.rows[0].tmw, lt.rows[0].tsw), (0, 0));
}
