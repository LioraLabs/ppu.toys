//! Multi-hook composition: several independent `hdma()` hooks, each doing its
//! own thing, in one frame.
//!
//! `lua_output.rs` covers one hook (range, per-line variation) and two hooks on
//! the SAME field (later wins). This file covers the harder property the DSL
//! actually promises: hooks that touch different state must not interfere. The
//! mechanism that makes that work is per-row re-baselining — `write_state`
//! pushes the working row into the Lua globals (including `sync_win`'s reset of
//! the friendly `win` tables and their XOR baseline) before every hook call, so
//! a hook's write cannot leak into rows it does not cover or into another
//! hook's fields.

use ppu_core::{LineTable, LuaEngine, WIN_SCANLINE_STRIDE};

fn frame_of(src: &str) -> LineTable {
    let mut e = LuaEngine::new();
    e.set_sources(&[("main.lua".into(), src.into())]).unwrap();
    e.frame(0.0, 0).unwrap()
}

fn edges(t: &LineTable, y: usize) -> (u8, u8) {
    let b = ppu_core::window_scanline_bytes(t);
    (b[y * WIN_SCANLINE_STRIDE], b[y * WIN_SCANLINE_STRIDE + 1])
}

#[test]
fn a_hooks_write_does_not_leak_past_its_range_via_another_hook() {
    // The leak this guards: hook A writes `win.w1.lo` on rows 0..50 only, and
    // hook B runs on every row. If B's read-back saw A's still-set Lua global
    // rather than the re-baselined row, A's value would bleed down the frame.
    let t = frame_of(
        r#"
function frame(t, f)
  mode = 1
  hdma(0, 50, function(y) win.w1.lo = 5 end)
  hdma(0, 223, function(y) m7.cx = 2 end)
end
"#,
    );
    assert_eq!(edges(&t, 25).0, 5, "inside A's range");
    assert_eq!(edges(&t, 50).0, 5, "A's last covered line");
    assert_eq!(edges(&t, 51).0, 0, "one line past A: back to the default");
    assert_eq!(edges(&t, 200).0, 0, "far past A: still the default");
    assert_eq!(t.rows[200].m7.cx, 2, "B applies on every row regardless");
}

#[test]
fn the_same_guard_holds_for_raw_mnemonics_and_bg_fields() {
    let t = frame_of(
        r#"
function frame(t, f)
  mode = 1
  hdma(0, 50, function(y) WH0 = 5 end)
  hdma(0, 50, function(y) bg[1].scroll.x = 40 end)
  hdma(0, 223, function(y) brightness = 9 end)
end
"#,
    );
    assert_eq!((edges(&t, 25).0, edges(&t, 51).0), (5, 0), "raw WH0");
    assert_eq!(t.rows[25].bg[0].scroll_x, 40);
    assert_eq!(t.rows[51].bg[0].scroll_x, 0, "scroll does not leak either");
    assert_eq!(t.rows[200].brightness, 9);
}

#[test]
fn hooks_touching_different_fields_of_one_register_both_survive() {
    // W12SEL's two nibbles: BG1 low, BG2 high. Each hook must move only its own
    // bits — the friendly fold is per-field, not whole-byte.
    let t = frame_of(
        r#"
function frame(t, f)
  mode = 1
  hdma(0, 223, function(y) win.bg1.w1 = true end)
  hdma(0, 223, function(y) win.bg2.w1 = true end)
end
"#,
    );
    let w12sel = ppu_core::window_scanline_bytes(&t)[100 * WIN_SCANLINE_STRIDE + 4];
    assert_eq!(w12sel, 0x22, "both enable bits set, neither clobbered");
}

#[test]
fn a_later_hook_observes_an_earlier_hooks_write_on_the_same_row() {
    // Hooks compose in registration order against one working row, so a hook
    // can build on what came before it rather than on the frame defaults.
    let t = frame_of(
        r#"
function frame(t, f)
  mode = 1
  hdma(0, 223, function(y) win.w1.lo = 50 end)
  hdma(0, 223, function(y) win.w1.hi = win.w1.lo * 2 end)
end
"#,
    );
    assert_eq!(edges(&t, 100), (50, 100));
}

#[test]
fn read_modify_write_chains_across_hooks_but_restarts_each_row() {
    // Both hooks accumulate onto the frame-wide default (10 + 1 + 100)...
    let t = frame_of(
        r#"
function frame(t, f)
  mode = 1
  win.w1.lo = 10
  hdma(0, 223, function(y) win.w1.lo = win.w1.lo + 1 end)
  hdma(0, 223, function(y) win.w1.lo = win.w1.lo + 100 end)
end
"#,
    );
    assert_eq!(edges(&t, 100).0, 111);

    // ...and every row starts from the defaults again, so a += hook does NOT
    // ratchet down the frame. This is what per-row re-baselining buys.
    let t = frame_of(
        r#"
function frame(t, f)
  mode = 1
  win.w1.lo = 0
  hdma(0, 223, function(y) win.w1.lo = win.w1.lo + 1 end)
end
"#,
    );
    assert_eq!(
        (edges(&t, 0).0, edges(&t, 1).0, edges(&t, 100).0),
        (1, 1, 1),
        "each row resolves independently"
    );
}

#[test]
fn many_independent_hooks_all_apply() {
    let t = frame_of(
        r#"
function frame(t, f)
  mode = 1
  hdma(0, 223, function(y) win.w1.lo = 1 end)
  hdma(0, 223, function(y) win.w1.hi = 2 end)
  hdma(0, 223, function(y) win.w2.lo = 3 end)
  hdma(0, 223, function(y) win.w2.hi = 4 end)
  hdma(0, 223, function(y) brightness = 5 end)
  hdma(0, 223, function(y) m7.cx = 6 end)
  hdma(0, 223, function(y) bg[1].scroll.x = 7 end)
  hdma(0, 223, function(y) TM = 8 end)
end
"#,
    );
    let b = ppu_core::window_scanline_bytes(&t);
    let at = |i: usize| b[100 * WIN_SCANLINE_STRIDE + i];
    assert_eq!([at(0), at(1), at(2), at(3)], [1, 2, 3, 4]);
    let r = &t.rows[100];
    assert_eq!(
        (r.brightness, r.m7.cx, r.bg[0].scroll_x, r.tm),
        (5, 6, 7, 8)
    );
}

#[test]
fn hooks_registered_from_different_functions_compose() {
    // The generated-pokes shape: apply_pokes() registers one hook, the sketch
    // registers others from its own functions. All of them land.
    let t = frame_of(
        r#"
function apply_pokes()
  hdma(0, 223, function(y) win.w1.lo = 11 end)
end
function other()
  hdma(0, 223, function(y) win.w1.hi = 22 end)
end
function frame(t, f)
  apply_pokes()
  other()
  mode = 1
end
"#,
    );
    assert_eq!(edges(&t, 100), (11, 22));
}

#[test]
fn a_hook_closure_keeps_the_upvalue_it_captured() {
    // Registering in a loop captures each iteration's local; all three hooks
    // write the same field, so the last registered wins.
    let t = frame_of(
        r#"
function frame(t, f)
  mode = 1
  for i = 1, 3 do
    local v = i * 10
    hdma(0, 223, function(y) win.w1.lo = v end)
  end
end
"#,
    );
    assert_eq!(edges(&t, 100).0, 30);
}

#[test]
fn a_conditional_write_inside_a_hook_leaves_other_rows_alone() {
    // A hook covering the whole frame but writing only on some rows must not
    // hold its last written value on the rows it skipped.
    let t = frame_of(
        r#"
function frame(t, f)
  mode = 1
  hdma(0, 223, function(y) if y < 50 then win.w1.lo = 5 end end)
end
"#,
    );
    assert_eq!(edges(&t, 25).0, 5);
    assert_eq!(edges(&t, 100).0, 0, "skipped rows keep the default");
}

#[test]
fn non_overlapping_ranges_split_the_screen() {
    let t = frame_of(
        r#"
function frame(t, f)
  mode = 1
  hdma(0, 111, function(y) win.w1.lo = 5 end)
  hdma(112, 223, function(y) win.w1.lo = 9 end)
end
"#,
    );
    assert_eq!((edges(&t, 50).0, edges(&t, 150).0), (5, 9));
}
