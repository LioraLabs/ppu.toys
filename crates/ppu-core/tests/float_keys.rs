//! The DSL floors floats everywhere: register values (ToInt) AND the keys of
//! the memory poke tables. `/` is float division in Lua, so `cgram[i / 2]`
//! must land on floor(i / 2), never vanish as a non-integer key.
use ppu_core::LuaEngine;

#[test]
fn fractional_keys_floor_into_cgram_vram_and_tilemaps() {
    let mut e = LuaEngine::new();
    e.set_source(
        "function frame(t, f)\n\
           cgram[3.9] = 0x7fff\n\
           vram[0x100 + 0.5] = 0x1234\n\
           bg[1].map[1.5] = {}\n\
           bg[1].map[1.5][2.7] = { tile = 7 }\n\
           m7.map[1.2] = {}\n\
           m7.map[1.2][3.8] = 9\n\
         end\n",
    )
    .unwrap();
    e.frame(0.0, 0).unwrap();
    let mem = e.memory();
    assert_eq!(mem.cgram[3], 0x7fff);
    assert_eq!(mem.vram[0x100], 0x1234);
    // bg1 map_base 0, screen 0: (col 1, row 2) -> word 2*32 + 1
    assert_eq!(mem.vram[2 * 32 + 1], 7);
    // m7 map: row 1, col 3 -> low byte of word 1*128 + 3
    assert_eq!(mem.vram[128 + 3] & 0xff, 9);
}
