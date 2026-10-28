use ppu_core::LuaEngine;

fn frame(source: &str) -> Vec<i16> {
    let mut engine = LuaEngine::new();
    engine.set_source(source).unwrap();
    engine.frame(0., 0).unwrap();
    engine.memory().oam[..4].iter().map(|o| o.x).collect()
}

#[test]
fn raw_source_q10_expressions_and_store_rounding() {
    let source = "---@snes fixed x -10 10 step 1/16\nlocal x = .1 + .1\n---@snes fixed y -10 10\nlocal y = .1 + .1\nfunction frame(t,f)\n obj[0].x = floor(x * 16)\n obj[1].x = floor(y * 1024 + .5)\n local dt = 1 / 60\n x = dt * 60\n obj[2].x = floor(x * 16)\n x = -.03125\n obj[3].x = floor(x * 16)\nend\n";
    assert_eq!(frame(source), [3, 204, 16, -1]);
}

#[test]
fn arithmetic_uses_q10_before_a_coarse_store() {
    let source = "---@snes fixed x -1000 1000\nlocal x = -.1\nfunction frame(t,f)\n obj[0].x = floor((x * .1) * 1024)\n obj[1].x = floor((x / 3) * 1024)\n obj[2].x = floor((x % .0625) * 1024)\n obj[3].x = floor((max(1, .1) / 60) * 1024)\nend\n";
    assert_eq!(frame(source), [-10, -34, 26, 17]);
}

#[test]
fn lexical_parameters_captures_arrays_and_parallel_stores() {
    let source = "---@snes fixed A -4 4 step 1/16\nlocal A = {-.03125, .09375}\n---@snes fixed x -4 4 step 1/4\nlocal x = .1\n---@snes fixed n -4 4 step 1/16\nlocal function plus(n) return n + x end\nfunction frame(t,f)\n A[1], A[2] = A[2] + .03125, A[1]\n x = plus(.1)\n obj[0].x = floor(A[1] * 16)\n obj[1].x = floor(A[2] * 16)\n obj[2].x = floor(x * 16)\n do\n  ---@snes fixed x -4 4 step 1/2\n  local x = .25\n  ---@snes fixed n -4 4 step 1/16\n  local function captured(n) return x + n end\n  obj[3].x = floor(captured(-.03125) * 16)\n end\nend\n";
    assert_eq!(frame(source), [3, -1, 4, 7]);
}

#[test]
fn failed_parallel_store_keeps_all_destinations_and_calls_once() {
    let source = "local calls = 0\n---@snes fixed x -1 1 step 1/16\nlocal x = 1.01\n---@snes fixed y -1 1 step 1/16\nlocal y = -1.01\nlocal function once() calls = calls + 1 return .25 end\nfunction frame(t,f)\n if f == 0 then x, y = once(), 2 end\n obj[0].x = floor(x * 16)\n obj[1].x = floor(y * 16)\n obj[2].x = calls\nend\n";
    let mut e = LuaEngine::new();
    for assignment in ["once(), 2", "2, once()"] {
        let source = source.replace("once(), 2", assignment);
        e.set_sources(&[("atomic.lua", &source)]).unwrap();
        let error = e.frame(0., 0).unwrap_err();
        assert_eq!(error.file.as_deref(), Some("atomic.lua"));
        assert!(error.message.contains("source line 8"), "{error:?}");
        e.frame(0., 1).unwrap();
        assert_eq!(
            e.memory().oam[..3].iter().map(|o| o.x).collect::<Vec<_>>(),
            [16, -16, 1]
        );
    }
}

#[test]
fn array_capacity_uninitialized_access_and_index_calls_are_checked() {
    for expression in ["A[0]", "A[3]", "A[2]"] {
        let source = format!("---@snes array A 2 -2 2\n---@snes fixed A -2 2 step 1/16\nlocal A = {{0}}\nfunction frame(t,f) obj[0].x = floor({expression}) end\n");
        let mut e = LuaEngine::new();
        e.set_source(&source).unwrap();
        assert!(e.frame(0., 0).is_err(), "{expression}");
    }
    let source="local calls=0\n---@snes fixed A -2 2 step 1/16\nlocal A={0,0}\nlocal function index() calls=calls+1 return 2 end\nfunction frame(t,f)\n A[index()] = .09375\n obj[0].x = floor(A[index()] * 16)\n obj[1].x = calls\n obj[2].x = #A\nend";
    assert_eq!(&frame(source)[..3], [2, 2, 2]);
}

#[test]
fn cart_time_and_multifile_globals_use_the_normal_loading_api() {
    let mut e = LuaEngine::new();
    e.set_sources(&[
        (
            "state.lua",
            "---@snes fixed x -10 10 step 1/16\nx = .1+.1\n",
        ),
        (
            "frame.lua",
            "function frame(t,f) obj[0].x=floor(x*16) obj[1].x=floor(t*1024) end",
        ),
    ])
    .unwrap();
    e.frame(8. / 60., 8).unwrap();
    assert_eq!(e.memory().oam[0].x, 3);
    assert_eq!(e.memory().oam[1].x, 136);
}

#[test]
fn malformed_and_unsupported_input_refuses_before_replacing_the_live_program() {
    let mut e = LuaEngine::new();
    e.set_source("function frame() obj[0].x=7 end").unwrap();
    for source in [
        "---@snes fixed x -1 1 step 1/3\nlocal x=0",
        "---@snes fixed missing -1 1\nlocal x=0",
        "---@snes fixed x -1 1\nlocal x=0\nfunction frame(t,f) x=math.sin(t) end",
        "---@snes fixed x -1 1\nlocal x=0\nlocal function pair() return 0,0 end\nfunction frame(t,f) local y=0 x,y=pair() end",
        "---@snes fixed A -1 1\nlocal A={0}\nlocal alias=A",
    ] {
        let error=e.set_sources(&[("bad.lua",source)]).unwrap_err();
        assert_eq!(error.file.as_deref(),Some("bad.lua"));
        assert!(error.line.is_some() && error.message.len()>30,"{error:?}");
        e.frame(0.,0).unwrap(); assert_eq!(e.memory().oam[0].x,7);
    }
}

#[test]
fn legacy_programs_and_annotation_shaped_strings_keep_native_lua_numbers() {
    for prefix in [
        "---@snes fixed 10\n---@snes fixed10 x -1 1\n",
        "local ignored = [=[\n---@snes fixed x -1 1\n]=]\n",
    ] {
        let source =
            format!("{prefix}local x=.1+.1\nfunction frame() obj[0].x=floor(x*1024+.5) end");
        assert_eq!(frame(&source)[0], 205);
    }
}

#[test]
fn pure_lookup_bodies_and_mode7_constants_keep_authored_f64() {
    let source="---@snes fixed x -1 1\nlocal x=0\n---@snes lookup a 1 1\nlocal function lookup(a) return floor((.1+.1)*1024+.5) end\nfunction frame(t,f)\n obj[0].x=lookup(1)\n m7.a=.001953\n m7.b=1/512\nend";
    let mut e = LuaEngine::new();
    e.set_source(source).unwrap();
    e.frame(0., 0).unwrap();
    assert_eq!(e.memory().oam[0].x, 205);
    assert_eq!(e.frame(0., 0).unwrap().rows[0].m7.a, 0);
}

#[test]
fn helper_runtime_failures_retain_the_helpers_file_and_line() {
    let mut e = LuaEngine::new();
    e.set_sources(&[
        (
            "state.lua",
            "---@snes fixed x -1 1\nlocal x=0\nfunction update()\n x=2\nend",
        ),
        ("frame.lua", "function frame(t,f) update() end"),
    ])
    .unwrap();
    let error = e.frame(0., 0).unwrap_err();
    assert_eq!(error.file.as_deref(), Some("state.lua"));
    assert_eq!(error.line, Some(4));
}

#[test]
fn integer_selected_by_max_does_not_change_the_fixed_arithmetic_contract() {
    let mut e = LuaEngine::new();
    e.set_source("---@snes fixed x -1 1\nlocal x=0\nfunction frame(t,f) local v=max(2000,.1) obj[0].x=floor(v*v) end").unwrap();
    assert!(
        e.frame(0., 0).is_err(),
        "the Q10 product exceeds i32 even when max selected an integer argument"
    );
}

#[test]
fn unrelated_multiple_returns_and_array_length_arithmetic_keep_lua_semantics() {
    let source="---@snes fixed x -4 4\nlocal x=0\nlocal A={1,2}\nlocal function pair() return 3,4 end\nfunction frame(t,f)\n local a,b=pair()\n x=#A+.1+.1\n obj[0].x=a+b\n obj[1].x=floor((x-2)*1024+.5)\nend";
    assert_eq!(&frame(source)[..2], [7, 204]);
    let mut e = LuaEngine::new();
    e.set_source("---@snes fixed t -1 1 step 1/16\nfunction frame(t,f) obj[0].x=floor(t*1024) end")
        .unwrap();
    e.frame(8. / 60., 8).unwrap();
    assert_eq!(e.memory().oam[0].x, 128);
}
