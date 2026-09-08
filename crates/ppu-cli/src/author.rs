use super::*;
use anyhow::{anyhow, ensure};
use ppu_core::{render_frame_stats, LuaEngine, LuaError, ObjOverflow, HEIGHT, WIDTH};
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter};

pub(super) fn source_bytes(
    dir: &Path,
    source: &ManifestSource,
) -> Result<(Vec<u8>, serde_json::Value)> {
    let read_path = |name: &str| -> Result<std::path::PathBuf> {
        ensure!(safe_relative(name), "unsafe source path {name:?}");
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
            let (width, height, rgba) = read_png(&read_path(file)?)?;
            let priority = source
                .priority_file
                .as_ref()
                .map(|name| -> Result<Vec<u8>> {
                    let (w, h, pixels) = read_png(&read_path(name)?)?;
                    ensure!(
                        (w, h) == (width, height),
                        "priority PNG dimensions must match source"
                    );
                    Ok(pixels)
                })
                .transpose()?;
            let kind = source.kind.parse().map_err(|e: String| anyhow!(e))?;
            let options =
                serde_json::from_value(source.options.clone()).context("invalid source options")?;
            let (payload, meta) = ppu_core::convert_source_with_priority(
                kind,
                &options,
                &rgba,
                width,
                height,
                priority.as_deref(),
            )
            .map_err(|e| anyhow!(e))?;
            Ok((payload.encode(), serde_json::to_value(meta)?))
        }
        _ => bail!("specify exactly one of file (PNG) or payload (binary)"),
    }
}

fn read_png(path: &Path) -> Result<(u32, u32, Vec<u8>)> {
    let mut decoder = png::Decoder::new(BufReader::new(
        File::open(path).with_context(|| format!("cannot read {}", path.display()))?,
    ));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().context("invalid PNG")?;
    let mut bytes = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut bytes)?;
    let bytes = &bytes[..info.buffer_size()];
    let mut rgba = Vec::with_capacity(info.width as usize * info.height as usize * 4);
    for pixel in bytes.chunks_exact(info.color_type.samples()) {
        match info.color_type {
            png::ColorType::Rgba => rgba.extend_from_slice(pixel),
            png::ColorType::Rgb => rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]),
            png::ColorType::Grayscale => {
                rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], 255])
            }
            png::ColorType::GrayscaleAlpha => {
                rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]])
            }
            png::ColorType::Indexed => bail!("PNG palette was not expanded"),
        }
    }
    Ok((info.width, info.height, rgba))
}

fn write_png(path: &Path, width: u32, height: u32, pixels: &[u8]) -> Result<()> {
    let mut encoder = png::Encoder::new(BufWriter::new(File::create(path)?), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(pixels)?;
    Ok(())
}

/// Create a complete animated project, including an editable PNG source.
pub fn new_project(dir: &Path) -> Result<()> {
    // create_dir deliberately refuses existing directories: never clobber a project.
    fs::create_dir(dir).with_context(|| format!("cannot create new project {}", dir.display()))?;
    fs::create_dir(dir.join("assets"))?;
    let manifest = serde_json::json!({
        "title": dir.file_name().and_then(|n| n.to_str()).unwrap_or("My demo"),
        "description": "Mode 7 flight with per-scanline perspective and palette lighting.",
        "files": ["timeline.lua", "main.lua"],
        "sources": [{"name": "floor", "kind": "m7", "file": "assets/floor.png", "options": {}}]
    });
    fs::write(
        dir.join(MANIFEST),
        format!("{}\n", serde_json::to_string_pretty(&manifest)?),
    )?;
    fs::write(dir.join("timeline.lua"), "-- timeline: end=8 in=0 out=8 loop=true\n-- loop markers: in=start out=loop_end\nmarkers = { start = 0, loop_end = 8 }\n")?;
    fs::write(dir.join("main.lua"), include_str!("starter.lua"))?;
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
        body.files.iter().any(|f| f.name == "main.lua"),
        "toy must include main.lua"
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
    let mut engine = LuaEngine::new();
    for source in &body.sources {
        let bytes = BASE64
            .decode(&source.payload)
            .with_context(|| format!("invalid payload for {:?}", source.name))?;
        engine
            .add_source(&source.name, &bytes)
            .map_err(|e| anyhow!("source {:?}: {e:?}", source.name))?;
    }
    let mut files: Vec<_> = body
        .files
        .iter()
        .map(|f| (f.name.as_str(), f.source.as_str()))
        .collect();
    if !files.iter().any(|f| f.0 == "pokes.lua") {
        files.insert(0, ("pokes.lua", "function apply_pokes() end"));
    }
    engine.set_sources(&files).map_err(lua_error)?;
    Ok(engine)
}

fn draw(engine: &mut LuaEngine, frame: u32) -> Result<(Vec<u8>, ObjOverflow)> {
    let time = f64::from(frame) / 60.0;
    let lines = engine
        .frame(time, frame)
        .map_err(lua_error)
        .with_context(|| format!("frame {frame} at {time:.6}s"))?;
    Ok(render_frame_stats(&lines, engine.memory()))
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
pub fn render(path: &Path, frames: &[u32], out: &Path) -> Result<()> {
    ensure!(!frames.is_empty(), "at least one sample time is required");
    let body = load_body(path)?;
    let mut engine = engine(&body)?;
    let wanted: std::collections::BTreeSet<_> = frames.iter().copied().collect();
    fs::create_dir_all(out)?;
    for f in 0..=*wanted.last().unwrap() {
        let (pixels, overflow) = draw(&mut engine, f)?;
        if wanted.contains(&f) {
            let output = out.join(format!("frame-{f:08}.png"));
            write_png(&output, WIDTH as u32, HEIGHT as u32, &pixels)?;
            println!("{} ({:.6}s)", output.display(), f64::from(f) / 60.0);
            if overflow.range_over || overflow.time_over {
                eprintln!("warning: sprite overflow at frame {f}");
            }
        }
    }
    Ok(())
}

/// Check continuous playback; optional sample comparisons prove deterministic seeking/looping.
pub fn check(
    path: &Path,
    duration: f64,
    seeks: &[u32],
    loop_seconds: Option<f64>,
    allow_overflow: bool,
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
    // Include the endpoint so exact cuts and loop boundaries are exercised.
    for f in 0..=end {
        let (pixels, overflow) = draw(&mut runner, f)?;
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
    for (&f, expected) in references.iter().rev() {
        ensure!(
            draw(&mut runner, f)?.0 == *expected,
            "backward seek differs at frame {f}"
        );
        let mut fresh = engine(&body)?;
        ensure!(
            draw(&mut fresh, f)?.0 == *expected,
            "direct seek differs at frame {f}"
        );
        if let Some(period) = period {
            let shifted = f
                .checked_add(period)
                .context("loop sample exceeds frame range")?;
            ensure!(
                draw(&mut runner, shifted)?.0 == *expected,
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
