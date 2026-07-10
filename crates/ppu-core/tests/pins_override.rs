//! Pinned-override integration (M9): pins registered as the FINAL LineTable
//! hook win over both frame-wide script writes and hdma hooks on every
//! scanline, and clearing them restores script values (the ▶ Run restart path
//! calls clearPins through the seam).

use ppu_core::LuaEngine;

const SRC: &str = "function frame(t, f)\n\
    brightness = 4\n\
    TM = 0x11\n\
    hdma(100, 199, function(y) brightness = 2 end)\n\
end";

#[test]
fn pinned_register_wins_over_script_and_hdma_on_all_lines() {
    let mut e = LuaEngine::new();
    e.set_source(SRC).unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    assert_eq!(lt.rows[0].brightness, 4);
    assert_eq!(lt.rows[150].brightness, 2); // hdma override active

    e.pins_mut().pin(0x2100, 0x0f);
    e.pins_mut().pin(0x212c, 0x1f);
    let lt = e.frame(0.0, 0).unwrap();
    assert_eq!(lt.rows[0].brightness, 15); // beats the frame-wide write
    assert_eq!(lt.rows[150].brightness, 15); // beats the hdma hook too
    assert_eq!(lt.rows[223].brightness, 15);
    assert_eq!(lt.rows[50].tm, 0x1f);
}

#[test]
fn clearing_pins_restores_script_values() {
    let mut e = LuaEngine::new();
    e.set_source(SRC).unwrap();
    e.pins_mut().pin(0x2100, 0x0f);
    e.frame(0.0, 0).unwrap();
    e.pins_mut().clear();
    let lt = e.frame(0.0, 0).unwrap();
    assert_eq!(lt.rows[0].brightness, 4);
    assert_eq!(lt.rows[150].brightness, 2);
}

#[test]
fn unpin_restores_a_single_register() {
    let mut e = LuaEngine::new();
    e.set_source(SRC).unwrap();
    e.pins_mut().pin(0x2100, 0x0f);
    e.pins_mut().pin(0x212c, 0x1f);
    e.pins_mut().unpin(0x2100);
    let lt = e.frame(0.0, 0).unwrap();
    assert_eq!(lt.rows[0].brightness, 4); // unpinned -> script value
    assert_eq!(lt.rows[0].tm, 0x1f); // still pinned
}

#[test]
fn pins_do_not_leak_into_sticky_lua_globals() {
    // Pins mutate only the native LineTable rows; the restored sticky globals
    // stay the script's own defaults across pin/unpin cycles.
    let mut e = LuaEngine::new();
    e.set_source("brightness = 9").unwrap(); // top-level default, no frame()
    e.pins_mut().pin(0x2100, 0x03);
    let lt = e.frame(0.0, 0).unwrap();
    assert_eq!(lt.rows[0].brightness, 3);
    e.pins_mut().clear();
    let lt = e.frame(0.0, 0).unwrap();
    assert_eq!(lt.rows[0].brightness, 9);
}
