use ppu_core::LuaEngine;

#[test]
fn ordinary_numbers_records_closures_and_shadowing_keep_lua_semantics() {
    let mut e = LuaEngine::new();
    e.set_source(
        r#"
local health=makeBounded(100,0,100)
local enemy={damage=3,kind="seeker"}
local function hit() health=health-enemy.damage end
local function shadow(health) return health+200 end
local function custom(makeBounded) return makeBounded(4) end
function frame()
  hit()
  do local health=500; obj[1].x=health//2 end
  obj[0].x=health
  obj[2].x=shadow(0)
  obj[3].x=custom(function(x) return x*2 end)
end
"#,
    )
    .unwrap();
    e.frame(0., 0).unwrap();
    assert_eq!(
        e.memory().oam[..4].iter().map(|s| s.x).collect::<Vec<_>>(),
        [97, 250, 200, 8]
    );
}

#[test]
fn failed_parallel_assignment_is_atomic_and_evaluates_values_once() {
    let mut e = LuaEngine::new();
    e.set_source("local x=makeBounded(3,0,10)\nlocal y=makeBounded(4,0,10)\nlocal calls=0\nlocal function once() calls=calls+1;return 5,99 end\nfunction frame(t,f)\n if f==0 then x,y=once() end\n obj[0].x=x;obj[1].x=y;obj[2].x=calls\nend").unwrap();
    let error = e.frame(0., 0).unwrap_err();
    assert_eq!(error.line, Some(6));
    assert!(error.message.contains("binding y"), "{error:?}");
    e.frame(1. / 60., 1).unwrap();
    assert_eq!(
        e.memory().oam[..3].iter().map(|s| s.x).collect::<Vec<_>>(),
        [3, 4, 1]
    );
}

#[test]
fn constructors_keep_one_return_and_assignments_keep_lua_return_adjustment() {
    let mut e = LuaEngine::new();
    e.set_source(
        r#"
local calls=0
local function pair() calls=calls+1;return 7,8 end
local x,y=makeBounded(pair(),0,10)
local a=makeBounded(0,0,10)
local b,c
function frame()
  a,b,c=1,pair()
  obj[0].x=x
  obj[1].x=y==nil and 1 or 0
  obj[2].x=a+b+c
  obj[3].x=calls
end
"#,
    )
    .unwrap();
    e.frame(0., 0).unwrap();
    assert_eq!(
        e.memory().oam[..4].iter().map(|s| s.x).collect::<Vec<_>>(),
        [7, 1, 16, 2]
    );
}

#[test]
fn forward_global_contracts_check_stores_in_the_defining_helpers_file() {
    let mut e = LuaEngine::new();
    e.set_sources(&[
        ("helper.lua", "function hurt()\n health=health-4\nend"),
        (
            "state.lua",
            "health=makeBounded(3,0,100)\nfunction frame() hurt() end",
        ),
    ])
    .unwrap();
    let error = e.frame(0., 0).unwrap_err();
    assert_eq!(error.file.as_deref(), Some("helper.lua"));
    assert_eq!(error.line, Some(2));
}

#[test]
fn invalid_values_and_declarations_fail_without_clamping() {
    for value in ["-1", "101", "1.5", "nil", "true", "'3'", "0/0"] {
        let mut e = LuaEngine::new();
        let error = e
            .set_source(&format!("local x=makeBounded({value},0,100)"))
            .unwrap_err();
        assert!(
            error.message.contains("requires an integer"),
            "{value}: {error:?}"
        );
    }
    for source in [
        "local x=makeBounded(0,100,0)",
        "local x=makeBounded(0,0,2147483648)",
        "local x=makeBounded(0,0,1.5)",
        "local x=makeBounded(0,0)",
        "local x=makeBounded(0,0,10)+1",
        "local f=makeBounded",
        "local x=0;x=makeBounded(0,0,10)",
        "if true then x=makeBounded(0,0,10) end",
        "function init() x=makeBounded(0,0,10) end",
    ] {
        assert!(LuaEngine::new().set_source(source).is_err(), "{source}");
    }
    // Record fields keep their contract (see luau_surface.rs).
    assert!(LuaEngine::new().set_source("local t={hp=makeBounded(0,0,10)}").is_ok());
}

#[test]
fn bounded_integers_coexist_with_explicit_fractional_storage() {
    let mut e = LuaEngine::new();
    e.set_source("---@snes fixed y -4 4 step 1/16\nlocal y=.1\nlocal x=makeBounded(2,0,100)\nfunction frame()\n x=x+1\n obj[0].x=x;obj[1].x=floor(y*16)\nend").unwrap();
    e.frame(0., 0).unwrap();
    assert_eq!(e.memory().oam[0].x, 3);
    assert_eq!(e.memory().oam[1].x, 2);
}

#[test]
fn comments_and_custom_helpers_do_not_enable_the_constructor_dialect() {
    for source in ["-- makeBounded\n__ppu_bounded_assign=3\nfunction frame() obj[0].x=__ppu_bounded_assign end", "function makeBounded(x) return x+1 end\nlocal x=makeBounded(2)\nfunction frame() obj[0].x=x end"] {
        let mut e=LuaEngine::new();e.set_source(source).unwrap();e.frame(0.,0).unwrap();assert_eq!(e.memory().oam[0].x,3);
    }
    assert!(LuaEngine::new()
        .set_source("local x=makeBounded(0,0,10)\nfunction makeBounded(x) return x end")
        .is_err());
}
