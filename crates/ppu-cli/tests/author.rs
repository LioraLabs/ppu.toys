use std::{fs, path::Path, process::Command};

/// `ppu` with an empty home, config and cache, against `site`: it reads nothing outside this
/// repository and the test's own files.
fn ppu(home: &Path, site: &str, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ppu"))
        .args(args)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("PPU_CONFIG_DIR", home.join("config"))
        .env("PPU_CACHE_DIR", home.join("cache"))
        .env("PPU_SITE", site)
        .output()
        .unwrap()
}

/// Every `ppu check` through here covers the preview half only: the site is unroutable and the
/// cache empty, so no check module runs and the cartridge half says NOT CHECKED.
fn run(args: &[&str]) -> std::process::Output {
    let home = tempfile::tempdir().unwrap();
    ppu(home.path(), "http://127.0.0.1:9", args)
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
    let checked = success(&["check", project, "--duration", "0.1", "--seek", "0,0.1"]);
    assert!(checked.contains("PASS: ") && checked.contains("NOT CHECKED: the cartridge check did not run"), "{checked}");
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
    for topic in ["cli", "mode7", "scanlines", "snes.budget", "sprites.group"] {
        assert!(!success(&["docs", topic]).is_empty());
    }
    assert!(success(&["--version"]).contains(env!("CARGO_PKG_VERSION")));
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
fn check_fails_on_the_contract_violations_the_studio_shows() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().to_str().unwrap();
    fs::write(
        tmp.path().join("main.luau"),
        "local x = makeBounded(0, 16, 240)\nfunction frame(t, f) x = 300 end\n",
    )
    .unwrap();
    fs::write(
        tmp.path().join("ppu.json"),
        r#"{"title":"test","files":["main.luau"],"sources":[]}"#,
    )
    .unwrap();
    let error = failure(&["check", path, "--duration", "0.1"]);
    assert!(
        error.contains("main.luau:1: error[contract]: x = 0 outside 16..240")
            && error.contains("at load")
            && error.contains("main.luau:2: error[contract]: x = 300 outside 16..240")
            && error.contains("2 contract violation(s)"),
        "{error}"
    );
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

/// `ppu new` scaffolds a Luau toy that runs: `check`/`render` round trip, and the manifest lists
/// only the toy's own file (no generated controls file).
#[test]
fn new_project_round_trips_through_check_and_render() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("demo");
    let project = project.to_str().unwrap();
    success(&["new", project]);

    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(Path::new(project).join("ppu.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["files"], serde_json::json!(["main.luau"]));
    assert_eq!(
        fs::read_dir(project).unwrap().count(),
        11,
        "ppu.json, main.luau, ppu.d.luau, assets/, .gitignore, .luaurc, .ppu/, .vscode/, AGENTS.md, .agents/, .claude/ and nothing else"
    );
    assert_eq!(
        fs::read_to_string(Path::new(project).join("ppu.d.luau")).unwrap(),
        ppu_core::LUAU_DECLARATIONS,
        "a Luau project carries the API declarations for editors"
    );

    // The toy must render two visibly different frames.
    let out = tmp.path().join("frames");
    let out = out.to_str().unwrap();
    success(&["check", project, "--duration", "0.5", "--seek", "0,0.25"]);
    success(&["render", project, "--at", "0,0.25", "-o", out]);
    let first = fs::read(Path::new(out).join("frame-00000000.png")).unwrap();
    let next = fs::read(Path::new(out).join("frame-00000015.png")).unwrap();
    assert_ne!(first, next, "the flight starter still animates");
}

/// `render --wav` writes the toy's stereo S-DSP output for the duration: a held button's sound
/// effect is heard, the same toy untouched is silence, and `--at` belongs to the PNG mode.
#[test]
fn render_wav_writes_what_the_toy_plays() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("beep");
    let project = project.to_str().unwrap();
    success(&["new", project]);
    fs::write(
        Path::new(project).join("main.luau"),
        "local zap = bank(\"pluck\")\nfunction init() dsp.mvol = { l = 127, r = 127 } end\n\
         function frame(t, f)\n  if pad.pressed.a then sfx(zap, 0, \"C5\") end\nend\n",
    )
    .unwrap();
    let render = |name: &str, extra: &[&str]| {
        let wav = tmp.path().join(name);
        let wav = wav.to_str().unwrap().to_string();
        let mut args = vec!["render", project, "--wav", &wav, "--duration", "1"];
        args.extend_from_slice(extra);
        let printed = success(&args);
        let (mono, rate) = ppu_core::wav::read_mono(&fs::read(&wav).unwrap()).unwrap();
        assert_eq!(rate, 32_000);
        assert!(
            mono.len().abs_diff(32_000) <= 534,
            "one second, give or take a frame: {}",
            mono.len()
        );
        (mono.iter().fold(0f32, |p, v| p.max(v.abs())), printed)
    };
    let (peak, printed) = render("held.wav", &["--hold", "a@10-20"]);
    assert!(peak > 0.1, "the held press sounds: peak {peak}");
    assert!(
        printed.contains("held.wav (1.000s") && printed.contains("dBFS"),
        "{printed}"
    );
    let (peak, printed) = render("quiet.wav", &[]);
    assert_eq!(peak, 0.0, "no press, no sound");
    assert!(printed.contains("peak silent"), "{printed}");
    let err = failure(&[
        "render",
        project,
        "--wav",
        "x.wav",
        "--duration",
        "1",
        "--at",
        "0",
    ]);
    assert!(err.contains("--at"), "{err}");
    assert!(success(&["--help"]).contains("--wav out.wav --duration"));
}

/// A Luau project carries the luau-lsp setup: every `@ppu/<name>` library has a stub typed by a
/// type ppu.d.luau exports, and `ppu types` refreshes the engine's files without touching settings
/// the author edited.
#[test]
fn new_project_sets_up_luau_lsp_and_types_refreshes_it() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("demo");
    success(&["new", project.to_str().unwrap()]);
    // ppu connect keeps replaced folder copies there; they are no part of the project.
    assert_eq!(
        fs::read_to_string(project.join(".gitignore")).unwrap(),
        ".ppu-backup/\n"
    );
    let luaurc: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(project.join(".luaurc")).unwrap()).unwrap();
    assert_eq!(luaurc["aliases"]["ppu"], ".ppu/lib");
    let settings: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(project.join(".vscode/settings.json")).unwrap())
            .unwrap();
    assert_eq!(
        settings["luau-lsp.types.definitionFiles"]["@ppu"],
        "ppu.d.luau"
    );
    serde_json::from_str::<serde_json::Value>(
        &fs::read_to_string(project.join(".vscode/extensions.json")).unwrap(),
    )
    .unwrap();
    assert!(!ppu_core::frontend::LIBRARIES.is_empty());
    for (file, _) in ppu_core::frontend::LIBRARIES {
        let name = file.trim_start_matches("@ppu/").trim_end_matches(".luau");
        let stub = fs::read_to_string(project.join(format!(".ppu/lib/{name}.luau"))).unwrap();
        assert!(stub.contains(&format!(":: ppu_{name}")), "{stub}");
        assert!(
            ppu_core::LUAU_DECLARATIONS.contains(&format!("\nexport type ppu_{name} = {{")),
            "ppu.d.luau exports ppu_{name}: luau-lsp only sees exported types"
        );
    }

    fs::write(project.join(".vscode/settings.json"), "{\"mine\": 1}").unwrap();
    fs::write(project.join("ppu.d.luau"), "stale").unwrap();
    fs::remove_file(project.join(".luaurc")).unwrap();
    success(&["types", project.to_str().unwrap()]);
    assert_eq!(
        fs::read_to_string(project.join("ppu.d.luau")).unwrap(),
        ppu_core::LUAU_DECLARATIONS
    );
    assert_eq!(
        fs::read_to_string(project.join(".vscode/settings.json")).unwrap(),
        "{\"mine\": 1}"
    );
    assert!(project.join(".luaurc").is_file());
    assert!(failure(&["types", tmp.path().to_str().unwrap()]).contains("not a ppu project"));
}

#[test]
fn new_project_with_lua_scaffolds_main_lua_and_no_declarations() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("demo");
    let project = project.to_str().unwrap();
    success(&["new", project, "--lua"]);
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(Path::new(project).join("ppu.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["files"], serde_json::json!(["main.lua"]));
    assert!(!Path::new(project).join("ppu.d.luau").exists());
    success(&["check", project, "--duration", "0.1"]);
}

/// `ppu new --world` scaffolds a Tiled world whose map, external tileset and source PNG agree:
/// it checks and renders, and a map re-saved with new layer data (as Tiled saves it) is what
/// pack and render pick up.
#[test]
fn new_world_project_scaffolds_tiled_files_that_load() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("world");
    let project = dir.to_str().unwrap();
    assert!(failure(&["new", project, "--world", "--lua"]).contains("usage"));
    success(&["new", project, "--world"]);
    for file in [
        "main.luau",
        "ppu.d.luau",
        "level1.tmj",
        "tiles.tsj",
        "project.tiled-project",
        "assets/tiles.png",
    ] {
        assert!(dir.join(file).is_file(), "{file}");
    }
    assert!(!dir.join("assets/floor.png").exists());
    let tileset: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.join("tiles.tsj")).unwrap()).unwrap();
    assert_eq!(
        tileset["image"], "assets/tiles.png",
        "the tileset draws the source's own PNG"
    );

    let render = |name: &str| {
        let out = tmp.path().join(name);
        success(&["check", project, "--duration", "0.1"]);
        success(&["render", project, "--at", "0", "-o", out.to_str().unwrap()]);
        fs::read(out.join("frame-00000000.png")).unwrap()
    };
    let before = render("before");

    // Paint the top-left on-screen cell with tile 3 (gid 4) and save the map back as JSON.
    let mut map: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.join("level1.tmj")).unwrap()).unwrap();
    map["layers"][0]["data"][0] = 4.into();
    fs::write(dir.join("level1.tmj"), serde_json::to_string(&map).unwrap()).unwrap();
    let packed = ppu_cli::pack(&dir).unwrap();
    assert!(packed.contains("tiles.tsj") && packed.contains("level1.tmj"));
    assert_ne!(before, render("after"), "the re-saved map is what renders");
}

#[test]
fn holds_press_either_controller() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().to_str().unwrap();
    // A frame that errors unless pad 1 holds A and pad 2 holds left, and only pad 2 holds left.
    lua_project(
        tmp.path(),
        "function frame(t,f) if f==3 and not (pad.a and pad2.left and not pad.left and not pad2.a) then error('pads') end end",
    );
    success(&["check", path, "--duration", "0.1", "--hold", "a+2.left@2-5"]);
    assert!(
        failure(&["check", path, "--duration", "0.1", "--hold", "a+left@2-5"]).contains("pads")
    );
    assert!(
        failure(&["check", path, "--duration", "0.1", "--hold", "2.nope@2-5"])
            .contains("unknown button")
    );
}

/// A scene module in a folder (`levels/w1-1.luau`) packs, unpacks into its folder, checks and
/// renders; names that would leave the project are still refused.
#[test]
fn folder_files_pack_unpack_check_and_render() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("demo");
    fs::create_dir_all(project.join("levels")).unwrap();
    fs::write(
        project.join("main.luau"),
        "W1 = scenes.add(require(\"./levels/w1-1\"))\n",
    )
    .unwrap();
    fs::write(
        project.join("levels/w1-1.luau"),
        "local function frame(_t: number, f: number)\n  bg[1].scroll.x = f\nend\nreturn {frame = frame}\n",
    )
    .unwrap();
    fs::write(
        project.join("ppu.json"),
        r#"{"title":"folders","files":["main.luau","levels/w1-1.luau"],"sources":[]}"#,
    )
    .unwrap();
    let path = project.to_str().unwrap();
    success(&["check", path, "--duration", "0.1"]);
    let out = tmp.path().join("frames");
    success(&["render", path, "--at", "0", "-o", out.to_str().unwrap()]);

    let packed = ppu_cli::pack(&project).unwrap();
    let file = tmp.path().join("demo.ppu.json");
    fs::write(&file, &packed).unwrap();
    success(&["check", file.to_str().unwrap(), "--duration", "0.1"]);
    let copy = tmp.path().join("copy");
    ppu_cli::unpack(&packed, &copy).unwrap();
    assert_eq!(
        fs::read_to_string(copy.join("levels/w1-1.luau")).unwrap(),
        fs::read_to_string(project.join("levels/w1-1.luau")).unwrap()
    );
    assert_eq!(ppu_cli::pack(&copy).unwrap(), packed);

    for bad in [
        "../evil.luau",
        "/abs.luau",
        "a\\\\b.luau",
        "a/../b.luau",
        "a//b.luau",
        "./a.luau",
        "ppu.json",
    ] {
        let text = packed.replace("levels/w1-1.luau", bad);
        let err = ppu_cli::unpack(&text, &tmp.path().join("bad")).unwrap_err();
        assert!(err.to_string().contains("unsafe"), "{bad}: {err}");
    }
}

/// A site answering `/api/check-module` with `sha256` and the module URL with `module`.
fn module_site(sha256: &'static str, module: &'static [u8]) -> String {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let mut header = String::new();
            while reader.read_line(&mut header).unwrap() > 2 {
                header.clear();
            }
            let body: Vec<u8> = if line.contains("/api/check-module") {
                serde_json::json!({"hash": "Ab12", "url": "/assets/check_bg-Ab12.wasm", "sha256": sha256})
                    .to_string()
                    .into_bytes()
            } else {
                module.to_vec()
            };
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
            stream.write_all(&body).unwrap();
        }
    });
    url
}

#[test]
fn a_check_module_that_does_not_match_its_served_hash_is_refused() {
    let home = tempfile::tempdir().unwrap();
    let toy = home.path().join("toy");
    fs::create_dir(&toy).unwrap();
    lua_project(&toy, "function frame(t,f) end");
    // the sha256 of b"\0asm", but the download has a byte more
    let site = module_site(
        "cd5d4935a48c0672cb06407bb443bc0087aff947c6b864bac886982c73b3027f",
        b"\0asm\x01",
    );
    let output = ppu(home.path(), &site, &["check", toy.to_str().unwrap(), "--duration", "0.1"]);
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{err}");
    assert!(err.contains("does not match the sha256"), "{err}");
    let cache = home.path().join("cache/check-module");
    assert!(!cache.join("Ab12.wasm").exists() && !cache.join("latest").exists());
}
