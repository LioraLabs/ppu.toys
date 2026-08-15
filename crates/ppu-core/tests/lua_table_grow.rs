//! Table growth must never panic the VM.
//!
//! Two shapes of ordinary Lua used to abort the interpreter instead of running:
//! a table whose map part sits exactly on a hashbrown capacity while its array
//! part wants to grow, and a table with a killed key (`t.k = nil`) that is then
//! grown. Both aborted inside `RawTable::set`, which is fatal rather than a Lua
//! error — in the wasm build one such sketch bricks the whole module, so a
//! published toy renders as a dead canvas for every visitor.
//!
//! Driven through the public `LuaEngine` seam (the one `wasm.rs` and the studio
//! use), because the claim is about sketches crashing the engine, not about any
//! internal table API.
use ppu_core::LuaEngine;

/// Run a sketch body as `frame()` and return the resulting brightness, so the
/// assertion needs the VM to have actually finished the frame.
fn run(body: &str) -> u8 {
    let mut e = LuaEngine::new();
    e.set_sources(&[(
        "main.lua",
        &format!("function frame(t, f)\n{body}\nend\n") as &str,
    )])
    .unwrap();
    e.frame(0.0, 0).unwrap().rows[0].brightness
}

// PPU-97
#[test]
fn a_literal_with_both_named_and_list_fields_can_be_extended() {
    // Named-field count 3 lands the map part exactly on a hashbrown capacity;
    // the six list entries leave the array part full at a non-power-of-two
    // length. Assigning one more key then grew the array without reserving a
    // map slot: "map slot must be pre-reserved".
    let b = run(
        "  local t = { k0 = 1, k1 = 2, k2 = 3, 10, 20, 30, 40, 50, 60 }\n\
                  \x20 t.zz = 9\n\
                  \x20 brightness = t.zz + t.k1 + t[6] - 60",
    );
    // 9 + 2 + 60 - 60 = 11 — inside brightness's 4-bit range, so the assertion
    // reads the sum rather than a wrapped one.
    assert_eq!(b, 11, "table lost values while growing");
}

// PPU-97
#[test]
fn a_field_can_be_cleared_and_the_table_then_extended() {
    // `t.a = nil` leaves a dead key behind. Growing the map afterwards rehashed
    // it: "all keys must be live when table is grown".
    let b = run("  local t = {}\n\
                  \x20 t.a = 1; t.b = 2; t.c = 3\n\
                  \x20 t.a = nil\n\
                  \x20 t.d = 4\n\
                  \x20 if t.a ~= nil then error('cleared key came back') end\n\
                  \x20 brightness = t.b + t.c + t.d");
    // 2 + 3 + 4 = 9: the kill took, and the later insert did not disturb it.
    assert_eq!(
        b, 9,
        "clearing a field then extending the table lost values"
    );
}

// PPU-97
#[test]
fn seven_named_fields_and_twelve_list_entries_can_be_extended() {
    // The next hashbrown capacity up (7), to pin that the fix is not a
    // special case for one table size.
    let b = run(
        "  local t = { a=1,b=2,c=3,d=4,e=5,f=6,g=7, 1,2,3,4,5,6,7,8,9,10,11,12 }\n\
                  \x20 t.h = 8\n\
                  \x20 brightness = t.h + t.a + t[12] - 9",
    );
    // 8 + 1 + 12 - 9 = 12.
    assert_eq!(b, 12, "table lost values while growing");
}

// PPU-97
#[test]
fn the_length_operator_is_exact_on_hole_free_sequences() {
    // The table-growth fix is allowed to change WHICH border `#` returns for a
    // SPARSE table — Lua leaves that unspecified, and the vendored VM does not
    // carry upstream's later `length()` refinement. It is not allowed to change
    // `#` on a hole-free sequence, which is the only case with one right answer,
    // and the case the engine itself depends on: lua.rs appends per-frame hdma
    // hooks as a dense 1..n array and reads them back with `length()`.
    //
    // Outlives vendor/piccolo — this must still hold after PPU-98 migrates the
    // backend.
    let b = run("  local worst = 0\n\
                  \x20 for n = 0, 40 do\n\
                  \x20   local t = {}\n\
                  \x20   for i = 1, n do t[i] = i end\n\
                  \x20   if #t ~= n then worst = 15 end\n\
                  \x20   for i = n, 1, -1 do t[i] = nil end\n\
                  \x20   if #t ~= 0 then worst = 15 end\n\
                  \x20 end\n\
                  \x20 brightness = worst");
    // 0 = every grow and every shrink reported its exact length. A single
    // disagreement sets 15, which is also the power-on default — so this only
    // passes if frame() actually ran and every length matched.
    assert_eq!(b, 0, "# disagreed with a hole-free sequence's true length");
}
