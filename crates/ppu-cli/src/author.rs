use super::*;
use anyhow::{anyhow, ensure};
use ppu_core::{render_frame_stats, LuaEngine, LuaError, ObjOverflow, HEIGHT, WIDTH};
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::BufWriter;

pub(super) fn source_bytes(
    dir: &Path,
    source: &ManifestSource,
) -> Result<(Vec<u8>, serde_json::Value)> {
    let read_path = |name: &str| -> Result<std::path::PathBuf> {
        let name = name.replace('\\', "/");
        ensure!(safe_relative(&name), "unsafe source path {name:?}");
        Ok(dir.join(name))
    };
    match (&source.payload, &source.file) {
        (Some(payload), None) => {
            ensure!(
                source.priority_file.is_none(),
                "priority_file requires a PNG file"
            );
            Ok((fs::read(read_path(payload)?)?, source.meta.clone()))
        }
        (None, Some(file)) => {
            let path = read_path(file)?;
            let read =
                |p: &Path| fs::read(p).with_context(|| format!("cannot read {}", p.display()));
            // A song has no SourceKind string (it is no convertSource kind): one arm per song
            // file format, by extension.
            if source.kind == "song" {
                ensure!(
                    source.priority_file.is_none(),
                    "priority_file requires a PNG file"
                );
                let song = if file.ends_with(".song.json") {
                    let text =
                        String::from_utf8(read(&path)?).context("a .song.json file is text")?;
                    ppu_core::song::from_json(&text).map_err(|e| anyhow!("{file}: {e}"))?
                } else if file.to_lowercase().ends_with(".mid") {
                    let song = ppu_core::import::midi::song_from_midi(&read(&path)?)
                        .map_err(|e| anyhow!("{file}: {e}"))?;
                    ppu_core::song::compile(&song).map_err(|e| anyhow!("{file}: {e}"))?;
                    song
                } else {
                    bail!("{file}: a song file is a .song.json or a .mid");
                };
                let psng = ppu_core::song::encode(&song);
                let meta = ppu_core::song::source_meta(&song, &psng);
                return Ok((ppu_core::SourcePayload::Song(psng).encode(), meta));
            }
            let kind: ppu_core::SourceKind = source.kind.parse().map_err(|e: String| anyhow!(e))?;
            ensure!(
                source.priority_file.is_none() || ppu_core::row_size_for(kind, 4).is_some(),
                "priority_file requires a PNG file"
            );
            let (payload, meta) = match kind {
                ppu_core::SourceKind::Palette => {
                    let text = String::from_utf8(read(&path)?).context("a .gpl file is text")?;
                    ppu_core::gpl::convert_palette(&text)
                }
                ppu_core::SourceKind::Sample => {
                    let options = serde_json::from_value(source.options.clone())
                        .context("invalid source options")?;
                    ppu_core::wav::read(&read(&path)?)
                        .and_then(|pcm| ppu_core::convert_sample(&pcm, &options))
                        .map_err(|e| format!("{file}: {e}"))
                }
                _ => {
                    let options = serde_json::from_value(source.options.clone())
                        .context("invalid source options")?;
                    let priority = source
                        .priority_file
                        .as_ref()
                        .map(|name| read(&read_path(name)?))
                        .transpose()?;
                    ppu_core::convert_png_source(kind, &options, &read(&path)?, priority.as_deref())
                }
            }
            .map_err(|e| anyhow!(e))?;
            Ok((payload.encode(), serde_json::to_value(meta)?))
        }
        _ => {
            bail!("specify exactly one of file (PNG, .gpl, .wav, .song.json or .mid) or payload (binary)")
        }
    }
}

fn write_png(path: &Path, width: u32, height: u32, pixels: &[u8]) -> Result<()> {
    let mut encoder = png::Encoder::new(BufWriter::new(File::create(path)?), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(pixels)?;
    Ok(())
}

/// Create a complete animated project, including an editable PNG source.
/// Scaffold a project: a Luau toy (`main.luau` and the luau-lsp setup, see `write_editor_files`), or with
/// `lua` a Lua one (`main.lua`).
pub fn new_project(dir: &Path, lua: bool) -> Result<()> {
    let main = if lua { "main.lua" } else { "main.luau" };
    // create_dir deliberately refuses existing directories: never clobber a project.
    fs::create_dir(dir).with_context(|| format!("cannot create new project {}", dir.display()))?;
    fs::create_dir(dir.join("assets"))?;
    let manifest = serde_json::json!({
        "title": dir.file_name().and_then(|n| n.to_str()).unwrap_or("My demo"),
        "description": "Mode 7 flight with per-scanline perspective and palette lighting.",
        "files": [main],
        "sources": [{"name": "floor", "kind": "m7", "file": "assets/floor.png", "options": {}}]
    });
    fs::write(
        dir.join(MANIFEST),
        format!("{}\n", serde_json::to_string_pretty(&manifest)?),
    )?;
    write_ignore(dir)?;
    if lua {
        fs::write(dir.join(main), include_str!("starter.lua"))?;
    } else {
        fs::write(dir.join(main), include_str!("starter.luau"))?;
        crate::write_editor_files(dir)?;
    }
    crate::write_skills(dir, false)?;
    let mut pixels = Vec::with_capacity(128 * 128 * 4);
    for y in 0..128 {
        for x in 0..128 {
            let color = if x % 16 == 0 || y % 16 == 0 {
                [88, 216, 232, 255]
            } else if (x / 16 + y / 16) % 2 == 0 {
                [24, 48, 96, 255]
            } else {
                [48, 24, 88, 255]
            };
            pixels.extend_from_slice(&color);
        }
    }
    write_png(&dir.join("assets/floor.png"), 128, 128, &pixels)
}

/// Scaffold a Luau world toy for Tiled: `ppu new` plus a map (`level1.tmj`), an external tileset
/// (`tiles.tsj`) whose image is the `tiles` source's PNG, and a Tiled project file to open.
pub fn new_world_project(dir: &Path) -> Result<()> {
    new_project(dir, false)?;
    // Swap the flight starter for the world one; the Luau editor files stay.
    fs::remove_file(dir.join("assets/floor.png"))?;
    fs::write(dir.join("main.luau"), include_str!("starter_world.luau"))?;
    let mut manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.join(MANIFEST))?)?;
    manifest["description"] =
        "A side-scrolling world painted in Tiled: level1.tmj on bg[1].".into();
    manifest["files"] = serde_json::json!(["main.luau", "level1.tmj", "tiles.tsj"]);
    manifest["sources"] = serde_json::json!([{"name": "tiles", "kind": "sheet",
        "file": "assets/tiles.png", "options": {"bit_depth": 4, "tile_size": 8}}]);
    let json = |v: serde_json::Value| -> Result<String> {
        Ok(format!("{}\n", serde_json::to_string_pretty(&v)?))
    };
    fs::write(dir.join(MANIFEST), json(manifest)?)?;

    // Eight 8x8 tiles in a row. Tile 0 is fully transparent: empty map cells draw it.
    const TILES: u32 = 8;
    let mut pixels = Vec::with_capacity((TILES * 8 * 8 * 4) as usize);
    for y in 0..8u32 {
        for x in 0..TILES * 8 {
            let (tile, tx) = (x / 8, x % 8);
            let edge = tx == 0 || y == 0;
            let color: [u8; 4] = match tile {
                1 if y < 2 => [72, 184, 64, 255], // grass over dirt
                1 | 2 if (tx + y * 3) % 5 == 0 => [96, 56, 32, 255],
                1 | 2 => [144, 88, 48, 255], // dirt
                3 if edge || (y == 4) => [200, 192, 176, 255],
                3 => [176, 64, 48, 255], // brick
                4 if edge => [80, 80, 96, 255],
                4 => [136, 136, 152, 255], // stone
                5 if y == 1 => [168, 216, 248, 255],
                5 => [48, 104, 200, 255], // water
                6 if (1..7).contains(&tx) && (2..6).contains(&y) => [240, 240, 248, 255], // cloud
                7 if tx.abs_diff(4) + y.abs_diff(4) < 3 => [248, 216, 64, 255], // gem
                _ => [0, 0, 0, 0],
            };
            pixels.extend_from_slice(&color);
        }
    }
    write_png(&dir.join("assets/tiles.png"), TILES * 8, 8, &pixels)?;

    // Paths inside Tiled files are relative to the file: the tileset sits beside the map.
    fs::write(
        dir.join("tiles.tsj"),
        json(serde_json::json!({
            "type": "tileset", "version": "1.10", "tiledversion": "1.11.0", "name": "tiles",
            "image": "assets/tiles.png", "imagewidth": TILES * 8, "imageheight": 8,
            "tilewidth": 8, "tileheight": 8, "columns": TILES, "tilecount": TILES,
            "margin": 0, "spacing": 0,
            "tiles": ([1, 2, 3, 4].map(|id| serde_json::json!({"id": id, "type": "solid"}))),
        }))?,
    )?;

    // 64x28 cells: two screens wide, one high. Tile ids are gids, so tileset tile n is n + 1.
    let (w, h) = (64usize, 28usize);
    let mut data = vec![0u32; w * h];
    let mut put = |x: usize, y: usize, tile: u32| data[y * w + x] = tile + 1;
    for x in 0..w {
        let water = (40..45).contains(&x);
        put(x, 24, if water { 5 } else { 1 });
        for y in 25..h {
            put(x, y, if water { 5 } else { 2 });
        }
    }
    (10..16).for_each(|x| put(x, 18, 3));
    (20..22).for_each(|x| (20..24).for_each(|y| put(x, y, 4)));
    (5..8)
        .chain(28..31)
        .chain(50..53)
        .for_each(|x| put(x, 4, 6));
    put(12, 16, 7);
    put(42, 20, 7);
    fs::write(
        dir.join("level1.tmj"),
        json(serde_json::json!({
            "type": "map", "version": "1.10", "tiledversion": "1.11.0",
            "orientation": "orthogonal", "renderorder": "right-down", "infinite": false,
            "width": w, "height": h, "tilewidth": 8, "tileheight": 8,
            "nextlayerid": 2, "nextobjectid": 1,
            "tilesets": [{"firstgid": 1, "source": "tiles.tsj"}],
            "layers": [{"id": 1, "name": "ground", "type": "tilelayer", "x": 0, "y": 0,
                "width": w, "height": h, "opacity": 1, "visible": true, "data": data}],
        }))?,
    )?;

    write_tiled_project(dir)
}

/// The `.gitignore` of a new or pulled project, when it has none: `ppu connect` keeps the
/// folder's copies the site's replaced in `.ppu-backup/`, which is no part of the project.
pub(crate) fn write_ignore(dir: &Path) -> Result<()> {
    let path = dir.join(".gitignore");
    if !path.exists() {
        fs::write(&path, format!("{}/\n", crate::BACKUP_DIR))
            .with_context(|| format!("cannot write {}", path.display()))?;
    }
    Ok(())
}

/// The Tiled project file `ppu new --world` writes: opening it in Tiled shows the folder's maps
/// and tilesets in its project view.
pub(crate) fn write_tiled_project(dir: &Path) -> Result<()> {
    let project = serde_json::json!({
        "automappingRulesFile": "", "commands": [], "compatibilityVersion": 1100,
        "extensionsPath": "extensions", "folders": ["."], "properties": [], "propertyTypes": [],
    });
    fs::write(
        dir.join("project.tiled-project"),
        format!("{}\n", serde_json::to_string_pretty(&project)?),
    )?;
    Ok(())
}

fn lua_error(e: LuaError) -> anyhow::Error {
    anyhow!(
        "{}:{}: {}",
        e.file.as_deref().unwrap_or("Lua"),
        e.line.map(|n| n.to_string()).unwrap_or_else(|| "?".into()),
        e.message
    )
}

fn load_body(path: &Path) -> Result<FileBody> {
    let text = if path.is_dir() {
        pack(path)?
    } else {
        fs::read_to_string(path)?
    };
    let body: FileBody = serde_json::from_str(&text).context("invalid packed toy")?;
    ensure!(
        body.version == VERSION,
        "unknown file version {:?}",
        body.version
    );
    ensure!(
        body.files
            .iter()
            .any(|f| f.name == "main.lua" || f.name == "main.luau"),
        "toy must include main.lua or main.luau"
    );
    let mut names = HashSet::new();
    for file in &body.files {
        ensure!(
            valid_file_name(&file.name),
            "unsafe file name {:?}",
            file.name
        );
        ensure!(names.insert(&file.name), "duplicate file {:?}", file.name);
    }
    names.clear();
    for source in &body.sources {
        ensure!(
            !source.name.is_empty() && names.insert(&source.name),
            "empty or duplicate source name {:?}",
            source.name
        );
    }
    Ok(body)
}

fn engine(body: &FileBody) -> Result<LuaEngine> {
    engine_on(body, None)
}

/// The engine with its save slot (`sram`) bound to `slot` JSON before the program loads.
fn engine_on(body: &FileBody, slot: Option<&str>) -> Result<LuaEngine> {
    let mut engine = LuaEngine::new();
    if let Some(slot) = slot {
        engine.set_sram(slot);
    }
    for source in &body.sources {
        let bytes = BASE64
            .decode(&source.payload)
            .with_context(|| format!("invalid payload for {:?}", source.name))?;
        engine
            .add_source(&source.name, &bytes)
            .map_err(|e| anyhow!("source {:?}: {e:?}", source.name))?;
        // A sheet's per-char palettes live in its meta, not its payload; a Tiled map needs them.
        if source.kind == "sheet" {
            if let Some(cells) = source.meta.get("cells").and_then(|c| c.as_array()) {
                let pals: Vec<u8> = cells
                    .iter()
                    .map(|c| c.get("pal").and_then(|p| p.as_u64()).unwrap_or(0) as u8)
                    .collect();
                engine.set_sheet_palettes(&source.name, &pals);
            }
        }
    }
    let files: Vec<_> = body
        .files
        .iter()
        .map(|f| (f.name.as_str(), f.source.as_str()))
        .collect();
    engine.set_sources(&files).map_err(lua_error)?;
    Ok(engine)
}

/// Buttons held on frames `[from, to)`: `r@450-460` or `a+l@600-610`, comma-separated. A `2.` prefix names a
/// button on the second controller (`2.a+2.left@600-610`); in the mask, pad 2 is the high 16 bits.
pub fn parse_holds(value: &str) -> Result<Vec<(u32, u32, u32)>> {
    value
        .split(',')
        .map(|hold| {
            let (buttons, frames) = hold.split_once('@').context("hold needs buttons@from-to")?;
            let (from, to) = frames
                .split_once('-')
                .context("hold needs buttons@from-to")?;
            let mut mask = 0;
            for button in buttons.split('+') {
                let (name, shift) = match button.strip_prefix("2.") {
                    Some(name) => (name, 16),
                    None => (button, 0),
                };
                let bit = ppu_core::PAD_NAMES
                    .iter()
                    .position(|n| *n == name)
                    .with_context(|| format!("unknown button {button:?}"))?;
                mask |= 1 << (bit + shift);
            }
            Ok((mask, from.parse()?, to.parse()?))
        })
        .collect()
}

fn draw(
    engine: &mut LuaEngine,
    frame: u32,
    holds: &[(u32, u32, u32)],
) -> Result<(Vec<u8>, ObjOverflow)> {
    let lines = step(engine, frame, holds)?;
    Ok(render_frame_stats(&lines, engine.memory()))
}

/// One frame with the held buttons, without composing a picture; `engine.audio()` is its sound.
fn step(
    engine: &mut LuaEngine,
    frame: u32,
    holds: &[(u32, u32, u32)],
) -> Result<ppu_core::LineTable> {
    let held = holds
        .iter()
        .filter(|(_, from, to)| (*from..*to).contains(&frame));
    let mask = held.fold(0, |mask, (buttons, _, _)| mask | buttons);
    engine.set_pad(mask as u16);
    engine.set_pad2((mask >> 16) as u16);
    let time = f64::from(frame) / 60.0;
    engine
        .frame(time, frame)
        .map_err(lua_error)
        .with_context(|| format!("frame {frame} at {time:.6}s"))
}

/// Times are sampled on the same 60 Hz grid as continuous playback.
pub fn frame_number(seconds: f64) -> Result<u32> {
    ensure!(
        seconds.is_finite() && seconds >= 0.0 && seconds * 60.0 < f64::from(u32::MAX),
        "time must be finite, nonnegative, and fit a 32-bit frame number"
    );
    Ok((seconds * 60.0).round() as u32)
}

pub fn parse_times(value: &str) -> Result<Vec<u32>> {
    value
        .split(',')
        .map(|t| frame_number(t.parse().context("expected comma-separated seconds")?))
        .collect()
}

/// Render sequentially from zero, including intermediate frames for stateful toys.
/// `srm`: a saveData record's battery RAM image, loaded first when the file exists and written back after the
/// last frame (the cartridge's layout, so the file moves to and from emulators and flash carts).
pub fn render(
    path: &Path,
    frames: &[u32],
    out: &Path,
    holds: &[(u32, u32, u32)],
    srm: Option<&Path>,
) -> Result<()> {
    ensure!(!frames.is_empty(), "at least one sample time is required");
    let body = load_body(path)?;
    let mut slot = match srm.filter(|p| p.exists()) {
        Some(p) => format!(
            "{{{}}}",
            ppu_core::save::slot_members(
                &fs::read(p).with_context(|| format!("cannot read {}", p.display()))?
            )
        ),
        None => "{}".into(),
    };
    let mut engine = engine_on(&body, Some(&slot))?;
    let wanted: std::collections::BTreeSet<_> = frames.iter().copied().collect();
    fs::create_dir_all(out)?;
    for f in 0..=*wanted.last().unwrap() {
        let (pixels, overflow) = draw(&mut engine, f, holds)?;
        if let Some(json) = engine.take_sram() {
            slot = json;
        }
        if wanted.contains(&f) {
            let output = out.join(format!("frame-{f:08}.png"));
            write_png(&output, WIDTH as u32, HEIGHT as u32, &pixels)?;
            println!("{} ({:.6}s)", output.display(), f64::from(f) / 60.0);
            if overflow.range_over || overflow.time_over {
                eprintln!("warning: sprite overflow at frame {f}");
            }
        }
    }
    if let Some(p) = srm {
        let image = ppu_core::save::srm_from_slot(&slot)
            .context("--srm: the program keeps no saveData record")?;
        fs::write(p, image).with_context(|| format!("cannot write {}", p.display()))?;
    }
    Ok(())
}

/// Play frames [0, `frames`) headless and write their stereo S-DSP output as a WAV: the samples
/// the Studio plays for those frames. Prints the file, its length and peak/RMS in dBFS.
pub fn render_wav(path: &Path, frames: u32, out: &Path, holds: &[(u32, u32, u32)]) -> Result<()> {
    ensure!(frames > 0, "duration must be at least one frame");
    let mut engine = engine(&load_body(path)?)?;
    let mut samples = Vec::new();
    for f in 0..frames {
        step(&mut engine, f, holds)?;
        samples.extend_from_slice(engine.audio());
    }
    let bytes =
        ppu_core::wav::write(&samples, 2, ppu_core::wav::DSP_RATE).map_err(|e| anyhow!(e))?;
    fs::write(out, bytes).with_context(|| format!("cannot write {}", out.display()))?;
    let (peak, sum) = samples.iter().fold((0f64, 0f64), |(p, s), &v| {
        let v = f64::from(v) / 32768.0;
        (p.max(v.abs()), s + v * v)
    });
    let rms = (sum / samples.len().max(1) as f64).sqrt();
    let db = |v: f64| {
        if v > 0.0 {
            format!("{:.1} dBFS", 20.0 * v.log10())
        } else {
            "silent".into()
        }
    };
    println!(
        "{} ({:.3}s stereo {} Hz): peak {}, RMS {}",
        out.display(),
        (samples.len() / 2) as f64 / f64::from(ppu_core::wav::DSP_RATE),
        ppu_core::wav::DSP_RATE,
        db(peak),
        db(rms)
    );
    Ok(())
}

/// Check continuous playback; optional sample comparisons prove deterministic seeking/looping.
pub fn check(
    path: &Path,
    duration: f64,
    seeks: &[u32],
    loop_seconds: Option<f64>,
    allow_overflow: bool,
    holds: &[(u32, u32, u32)],
) -> Result<()> {
    let end = frame_number(duration)?;
    ensure!(end > 0, "duration must be at least one frame");
    let period = loop_seconds.map(frame_number).transpose()?;
    if let Some(period) = period {
        ensure!(
            period > 0 && period <= end,
            "loop period must be positive and no longer than duration"
        );
    }
    let mut samples = seeks.to_vec();
    if let Some(period) = period {
        samples.extend([0, period / 4, period / 2, period / 4 * 3, period - 1]);
    }
    ensure!(
        samples.iter().all(|f| *f <= end),
        "seek samples must be within duration"
    );
    let body = load_body(path)?;
    for source in &body.sources {
        println!("source {}: {}", source.name, source.meta);
    }
    let mut runner = engine(&body)?;
    let mut references = BTreeMap::new();
    let (mut max_sprites, mut max_tiles, mut overflow_frames) = (0, 0, 0);
    let mut sustained = BTreeMap::new();
    // Include the endpoint so exact cuts and loop boundaries are exercised.
    for f in 0..=end {
        let (pixels, overflow) = draw(&mut runner, f, holds)?;
        for (n, v) in runner.dsp_view().voices.iter().enumerate() {
            if let (Some(secs), true) = (v.sustains, v.envx > 0) {
                sustained.insert(n, secs);
            }
        }
        max_sprites = max_sprites.max(overflow.max_sprites);
        max_tiles = max_tiles.max(overflow.max_tiles);
        if overflow.range_over || overflow.time_over {
            overflow_frames += 1;
            ensure!(allow_overflow, "sprite overflow at frame {f} ({:.6}s): range={}, tiles={} (use --allow-overflow for intentional overflow)", f64::from(f) / 60.0, overflow.range_over, overflow.time_over);
        }
        if samples.contains(&f) {
            references.insert(f, pixels);
        }
    }
    // The Studio's contract violations, from the same engine record: a cartridge stops on the first.
    let broken = runner.violations().to_vec();
    for v in &broken {
        let when = v
            .frame
            .map_or("at load".to_string(), |f| format!("frame {f}"));
        eprintln!(
            "{}:{}: error[contract]: {} = {} outside {}..{} ({}, {when}, {}×)",
            v.file, v.line, v.store, v.value, v.min, v.max, v.scene, v.count
        );
    }
    ensure!(
        broken.is_empty(),
        "{} contract violation(s) above: the preview runs on, a cartridge stops at the first",
        broken.len()
    );
    for (n, secs) in sustained {
        let how = if secs.is_finite() {
            format!("~{secs:.1} s")
        } else {
            "forever".into()
        };
        eprintln!("warning: voice {n} sustains {how}: a looping sample keyed on with kon() and no koff(); play it with sfx() (it stops after its len, 0.15 s by default) or koff({n})");
    }
    for (&f, expected) in references.iter().rev() {
        ensure!(
            draw(&mut runner, f, holds)?.0 == *expected,
            "backward seek differs at frame {f}"
        );
        let mut fresh = engine(&body)?;
        ensure!(
            draw(&mut fresh, f, holds)?.0 == *expected,
            "direct seek differs at frame {f}"
        );
        if let Some(period) = period {
            let shifted = f
                .checked_add(period)
                .context("loop sample exceeds frame range")?;
            ensure!(
                draw(&mut runner, shifted, holds)?.0 == *expected,
                "loop differs at frame {f} + {period}"
            );
        }
    }
    println!("PASS: {} frames at 60 fps; max {max_sprites} sprites / {max_tiles} slivers per line; {overflow_frames} overflow frames; {} direct/backward seek comparisons", u64::from(end) + 1, references.len());
    if let Some(period) = period {
        println!(
            "PASS: {} loop comparisons at {:.6}s ({period} frames)",
            references.len(),
            f64::from(period) / 60.0
        );
    }
    Ok(())
}
