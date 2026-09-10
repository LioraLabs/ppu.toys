use std::{fs, path::Path, process::Command};

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ppu"))
        .args(args)
        .output()
        .unwrap()
}

fn success(args: &[&str]) -> String {
    let output = run(args);
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn failure(args: &[&str]) -> String {
    let output = run(args);
    assert!(!output.status.success(), "unexpected success: {args:?}");
    String::from_utf8(output.stderr).unwrap()
}

#[test]
fn standalone_png_project_to_portable_upload() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("demo");
    let packed = tmp.path().join("demo.ppu.json");
    let output = tmp.path().join("frames");
    let project = project.to_str().unwrap();
    let packed = packed.to_str().unwrap();
    let output = output.to_str().unwrap();
    success(&["new", project]);
    assert!(Path::new(project).join("assets/floor.png").is_file());
    failure(&["new", project]); // existing projects are preserved
    success(&["check", project, "--duration", "0.1", "--seek", "0,0.1"]);
    success(&["render", project, "--at", "0,0.1", "-o", output]);
    let first = fs::read(Path::new(output).join("frame-00000000.png")).unwrap();
    let next = fs::read(Path::new(output).join("frame-00000006.png")).unwrap();
    assert_eq!(&first[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(u32::from_be_bytes(first[16..20].try_into().unwrap()), 256);
    assert_eq!(u32::from_be_bytes(first[20..24].try_into().unwrap()), 224);
    assert_ne!(first, next, "starter visibly animates");
    success(&["pack", project, "-o", packed]);
    let body: serde_json::Value = serde_json::from_slice(&fs::read(packed).unwrap()).unwrap();
    assert_eq!(body["sources"][0]["kind"], "m7");
    assert!(body["sources"][0]["payload"].as_str().unwrap().len() > 100);
    fs::remove_dir_all(project).unwrap();
    success(&["check", packed, "--duration", "0.1", "--seek", "0,0.1"]);
    success(&["render", packed, "--at", "0.1", "-o", output]);
    assert_eq!(
        next,
        fs::read(Path::new(output).join("frame-00000006.png")).unwrap()
    );
    let copy = tmp.path().join("copy");
    success(&["unpack", packed, copy.to_str().unwrap()]);
    let repacked: serde_json::Value = serde_json::from_str(&ppu_cli::pack(&copy).unwrap()).unwrap();
    assert_eq!(body, repacked);
    for topic in ["cli", "registers", "dma", "mode7", "scanlines", "all"] {
        assert!(!success(&["docs", topic]).is_empty());
    }
    assert!(success(&["--version"]).contains("0.1.0"));
}

fn lua_project(dir: &Path, code: &str) {
    fs::write(dir.join("main.lua"), code).unwrap();
    fs::write(
        dir.join("ppu.json"),
        r#"{"title":"test","files":["main.lua"],"sources":[]}"#,
    )
    .unwrap();
}

#[test]
fn checks_detect_runtime_overflow_seek_and_loop_failures() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().to_str().unwrap();
    lua_project(
        tmp.path(),
        "function frame(t,f) if f==2 then error('broken') end end",
    );
    let error = failure(&["check", path, "--duration", "0.1"]);
    assert!(
        error.contains("frame 2") && error.contains("main.lua") && error.contains("broken"),
        "{error}"
    );
    lua_project(
        tmp.path(),
        "local n=0; function frame(t,f) n=n+1; cgram[0]=rgb(n*8,0,0) end",
    );
    assert!(
        failure(&["check", path, "--duration", "0.1", "--seek", "0.1"]).contains("seek differs")
    );
    lua_project(tmp.path(), "function frame(t,f) cgram[0]=rgb(f*8,0,0) end");
    assert!(
        failure(&["check", path, "--duration", "0.1", "--loop", "0.1"]).contains("loop differs")
    );
    lua_project(
        tmp.path(),
        "function frame(t,f) cgram[0]=rgb((f%6)*8,0,0) end",
    );
    assert!(
        success(&["check", path, "--duration", "0.1", "--loop", "0.1"])
            .contains("loop comparisons at 0.100000s")
    );
    lua_project(
        tmp.path(),
        "function frame(t,f) for i=0,32 do obj[i].on=true; obj[i].x=i*4; obj[i].y=8 end end",
    );
    assert!(failure(&["check", path, "--duration", "0.1"]).contains("sprite overflow"));
    success(&["check", path, "--duration", "0.1", "--allow-overflow"]);
}

#[test]
fn invalid_cli_times_and_source_paths_fail() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().to_str().unwrap();
    lua_project(tmp.path(), "function frame(t,f) end");
    for time in ["NaN", "inf", "-1", "1e100", "0"] {
        failure(&["check", path, "--duration", time]);
    }
    failure(&["check", path, "--duration", "0.1", "--seek", "1"]);
    failure(&["check", path, "--duration", "0.1", "--loop", "0"]);
    failure(&["render", path, "--at", "NaN", "-o", path]);
    failure(&["check", path, "--unknown"]);
    failure(&["docs", "nonexistent"]);
    for source in [
        serde_json::json!({"name":"s","kind":"bg","file":"../outside.png"}),
        serde_json::json!({"name":"s","kind":"bg","file":"foo.png","payload":"foo.bin"}),
        serde_json::json!({"name":"s","kind":"bg"}),
    ] {
        let manifest = serde_json::json!({"title":"t","files":["main.lua"],"sources":[source]});
        fs::write(tmp.path().join("ppu.json"), manifest.to_string()).unwrap();
        assert!(ppu_cli::pack(tmp.path()).is_err());
    }
}

#[test]
fn indexed_png_is_expanded_and_reconverted_after_edit() {
    let tmp = tempfile::tempdir().unwrap();
    lua_project(tmp.path(), "local sky=dma('sky',{char=0x1000,map=0,pal=0}); function frame(t,f) mode=1;screen.main.bg1=true;bg[1].char_base=sky.char;bg[1].map_base=sky.map end");
    fs::write(tmp.path().join("ppu.json"), r#"{"title":"png","files":["main.lua"],"sources":[{"name":"sky","kind":"bg","file":"sky.png"}]}"#).unwrap();
    let make_png = |rgb: [u8; 3]| {
        let file = fs::File::create(tmp.path().join("sky.png")).unwrap();
        let mut png = png::Encoder::new(file, 8, 8);
        png.set_color(png::ColorType::Indexed);
        png.set_depth(png::BitDepth::Eight);
        png.set_palette([vec![0, 0, 0], rgb.to_vec()].concat());
        png.set_trns(vec![0, 255]);
        png.write_header()
            .unwrap()
            .write_image_data(&[1; 64])
            .unwrap();
    };
    make_png([255, 0, 0]);
    let red = ppu_cli::pack(tmp.path()).unwrap();
    success(&["check", tmp.path().to_str().unwrap(), "--duration", "0.05"]);
    make_png([0, 255, 0]);
    assert_ne!(red, ppu_cli::pack(tmp.path()).unwrap());
}

/// `ppu init` scaffolds `ppuglobals.lua` (not the retired `timeline.lua`), and
/// the manifest lists it first. This is a real round trip through the
/// binary (`check`/`render`), not a string compare of the scaffold against
/// its own template constant: `starter.lua` reads `markers.loop_end` out of
/// the generated file, so a wrong shape would fail `check`, not just look
/// wrong. The exact bytes are pinned too (the middle dot in the header and
/// the three-decimal marker times are load-bearing: they match what
/// `formatControls` on the web side emits for this same timeline data).
#[test]
fn new_project_scaffolds_controls_lua_and_it_round_trips_through_check_and_render() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("demo");
    let project = project.to_str().unwrap();
    success(&["new", project]);

    assert!(
        !Path::new(project).join("timeline.lua").exists(),
        "the legacy timeline.lua must not be scaffolded any more"
    );
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(Path::new(project).join("ppu.json")).unwrap())
            .unwrap();
    assert_eq!(
        manifest["files"],
        serde_json::json!(["ppuglobals.lua", "main.lua"])
    );

    let controls = fs::read_to_string(Path::new(project).join("ppuglobals.lua")).unwrap();
    assert_eq!(
        controls,
        "-- ppuglobals.lua \u{b7} generated by the Studio panels. Edit values here or in the panels.\n\
-- timeline: end=8 in=0 out=8 loop=true\n\
-- loop markers: in=start out=loop_end\n\
markers = {\n  start = 0.000,\n  loop_end = 8.000,\n}\n\
\n\
scanlines = {\n}\n\
\n\
function apply_pokes()\nend\n"
    );

    // Real round trip: starter.lua's `markers.loop_end` must actually
    // resolve, and the toy must render two visibly different frames.
    let out = tmp.path().join("frames");
    let out = out.to_str().unwrap();
    success(&["check", project, "--duration", "0.5", "--seek", "0,0.25"]);
    success(&["render", project, "--at", "0,0.25", "-o", out]);
    let first = fs::read(Path::new(out).join("frame-00000000.png")).unwrap();
    let next = fs::read(Path::new(out).join("frame-00000015.png")).unwrap();
    assert_ne!(first, next, "the flight starter still animates");
}

/// The CLI used to inject a `function apply_pokes() end` no-op stub as an
/// ordinary chunk whenever a packed toy had no `pokes.lua`, papering over an
/// explicit `apply_pokes()` call left over from the pre-`ppuglobals.lua` era.
/// That shim is gone: a toy with no `ppuglobals.lua` and an explicit
/// `apply_pokes()` call now errors, attributed to the CALLER's own file
/// (`main.lua`), the same shape
/// `explicit_apply_pokes_call_after_fast_path_clear_errors_on_the_callers_own_file`
/// pins at the `ppu-core` level for the analogous "the global genuinely
/// doesn't exist" case.
#[test]
fn explicit_apply_pokes_call_with_no_controls_lua_errors_instead_of_silently_no_op_ing() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().to_str().unwrap();
    lua_project(tmp.path(), "function frame(t, f) apply_pokes() end");
    let error = failure(&["check", path, "--duration", "0.1"]);
    assert!(
        error.contains("main.lua"),
        "error must name the caller's own file, not swallow it: {error}"
    );
}
