use ppu_core::LuaEngine;

#[test]
fn binary_strings_preserve_bytes_and_lua_index_ranges() {
    let mut engine = LuaEngine::new();
    engine
        .set_source(
            r#"
            local data = "\000\127\128\255"
            local a,b,c,d = string.byte(data, 1, -1)
            assert(a == 0 and b == 127 and c == 128 and d == 255)
            assert(string.byte(data) == 0)
            assert(string.byte(data, -1) == 255)
            assert(string.byte(data, 5) == nil)
            assert(string.byte(data, 3, 2) == nil)
            assert(string.byte(data, -100, 1) == 0)
            assert(string.byte(123) == 49)
            function frame(t,f) cgram[0] = string.byte(data, 4) end
            "#,
        )
        .unwrap();
    engine.frame(0.0, 0).unwrap();
    assert_eq!(engine.memory().cgram[0], 255);
}

#[test]
fn bit_tests_bind_before_equality_as_in_lua() {
    let mut engine = LuaEngine::new();
    engine
        .set_source(
            r#"
      assert(0 & 1 ~= 0 == false)
      assert(3 & 1 == 1)
      assert(1 | 2 == 3)
      assert(1 << 3 == 8)
      assert(2 + 3 == 5)
      function frame(t,f) end
    "#,
        )
        .unwrap();
    engine.frame(0.0, 0).unwrap();
}
