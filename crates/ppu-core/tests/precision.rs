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
fn fixed_constructor_checks_later_stores_and_chooses_binary_accuracy() {
    let source = "local x=makeFixed(.1+.1,-4,4,16)\nfunction frame(t,f)\n obj[0].x=floor(x*16)\n x=-.03125\n obj[1].x=floor(x*16)\nend";
    assert_eq!(&frame(source)[..2], [3, -1]);
    let mut e = LuaEngine::new();
    e.set_source("local x=makeFixed(0,-1,1)\nfunction frame(t,f) x=f;obj[0].x=x end")
        .unwrap();
    let error = e.frame(0., 2).unwrap_err();
    assert_eq!(error.line, Some(2));
    assert!(error.message.contains("range contract"), "{error:?}");
}

#[test]
fn precision_records_keep_aliases_nested_lists_and_fractional_feedback() {
    let source = r#"
local time=makeFixed(0,0,10)
local state={members={{x=0.0,velocity=46/188}},label="ready"}
function frame(t,f)
 local member=state.members[1]
 member.x=member.x+member.velocity
 time=time+1/60
 obj[0].x=floor(member.x*16)
 obj[1].x=floor(time*1024)
 obj[2].x=#state.members
 obj[3].x=state.label=="ready" and 1 or 0
end
"#;
    let mut e = LuaEngine::new();
    e.set_source(source).unwrap();
    for f in 0..5 {
        e.frame(0., f).unwrap();
    }
    assert_eq!(
        e.memory().oam[..4].iter().map(|o| o.x).collect::<Vec<_>>(),
        [19, 85, 1, 1]
    );
}

#[test]
fn precision_records_refuse_a_later_fractional_field_type_change() {
    let source = "local time=makeFixed(0,0,10)\nlocal state={x=0}\nfunction frame() state.x=.1 end";
    assert!(LuaEngine::new().set_source(source).is_err());
}

#[test]
fn record_alias_indexes_and_parallel_values_are_evaluated_once() {
    let source = r#"
local clock=makeFixed(0,0,10)
local count=0
local rows={{x=.1,y=.2},{x=.3,y=.4}}
local function index() count=count+1;return 1 end
function frame(t,f)
 local row=rows[index()]
 row["x"],row.y=row.y,row.x
 obj[0].x=floor(rows[1].x*1024)
 obj[1].x=floor(row.y*1024)
 obj[2].x=count
end
"#;
    assert_eq!(&frame(source)[..3], [205, 102, 1]);
}

#[test]
fn precision_field_stores_and_comparisons_enforce_the_numeric_representation() {
    for action in [
        "state.x=2097152",
        "local ignored=state.x<3000000",
        "local ignored=state.x==3000000",
    ] {
        let mut e = LuaEngine::new();
        e.set_source(&format!("local clock=makeFixed(0,0,10)\nlocal state={{x=0.0}}\nfunction frame(t,f)\n {action}\nend")).unwrap();
        let error = e.frame(0., 0).unwrap_err();
        assert_eq!(error.line, Some(4), "{action}: {error:?}");
        assert!(
            error.message.contains("representation"),
            "{action}: {error:?}"
        );
    }
}

#[test]
fn fixed_constructor_rejects_invalid_contracts_and_respects_custom_helpers() {
    for declaration in [
        "local x=makeFixed(0,-1,1,0)",
        "local x=makeFixed(0,-1,1,3)",
        "local x=makeFixed(0,-1,1,2048)",
        "local x=makeFixed(0,-1,1,-2)",
        "local x=makeFixed(0,-1,1,1/16)",
        "local x=makeFixed(0,1,-1)",
        "local x=makeFixed(0,-2097153,1)",
        "local x=makeFixed(0,-1,2097152)",
        "local x=makeFixed(0,-1.5,1)",
        "local x=makeFixed(0,-1)",
        "local x=makeFixed(0,-1,1)+1",
        "local x={y=makeFixed(0,-1,1)}",
        "local x=makeFixed",
        "---@snes fixed x -1 1\nlocal x=makeFixed(0,-1,1)",
    ] {
        assert!(
            LuaEngine::new().set_source(declaration).is_err(),
            "{declaration}"
        );
    }
    for source in [
        "-- makeFixed\nfunction frame() obj[0].x=floor((.1+.1)*1024+.5) end",
        "local function makeFixed(v) return v end\nlocal x=makeFixed(.1+.1)\nfunction frame() obj[0].x=floor(x*1024+.5) end",
    ] {assert_eq!(frame(source)[0],205);}
}

#[test]
fn precision_constructor_tail_results_are_explicit_instead_of_silently_lost() {
    for constructor in [
        "local rows={pair()}",
        "---@snes fixed rows -1 1\nlocal rows={pair()}",
    ] {
        let source=format!("local clock=makeFixed(0,0,10)\nlocal function pair() return .1,.2 end\n{constructor}\nfunction frame() end");
        let error = LuaEngine::new().set_source(&source).unwrap_err();
        assert!(error.message.contains("constructor tail"), "{error:?}");
    }
}

#[test]
fn typed_luau_record_fields_and_multifile_globals_share_precision() {
    let mut e = LuaEngine::new();
    e.set_sources(&[
        ("state.lua","local clock: number=makeFixed(0,0,10)\ntype State={x:number}\nlocal initial: State={x=0.0}\nstate=initial"),
        ("main.lua","function frame(t: number,f: number)\n state.x += .1\n obj[0].x=floor(state.x*1024)\nend"),
    ]).unwrap();
    e.frame(0., 0).unwrap();
    assert_eq!(e.memory().oam[0].x, 102);
}

#[test]
fn precision_tuples_keep_position_types_and_allow_ragged_numeric_lists() {
    let source = r#"
local clock=makeFixed(0,0,10)
local levels={{4,1,{0}},{24,2,{5,6,7,8}}}
function frame(t,f)
 local row=levels[f%2+1]
 obj[0].x=row[1]
 obj[1].x=row[2]
 obj[2].x=#row[3]
 obj[3].x=row[3][1]
end
"#;
    let mut e = LuaEngine::new();
    e.set_source(source).unwrap();
    e.frame(0., 0).unwrap();
    assert_eq!(
        e.memory().oam[..4].iter().map(|o| o.x).collect::<Vec<_>>(),
        [4, 1, 1, 0]
    );
    e.frame(0., 1).unwrap();
    assert_eq!(
        e.memory().oam[..4].iter().map(|o| o.x).collect::<Vec<_>>(),
        [24, 2, 4, 5]
    );
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
