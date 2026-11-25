use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[cfg(not(target_arch = "wasm32"))]
pub mod art;
#[cfg(not(target_arch = "wasm32"))]
mod art_job;
mod author;
pub use author::*;
#[cfg(not(target_arch = "wasm32"))]
pub mod cart;
#[cfg(all(not(target_arch = "wasm32"), feature = "cart-check"))]
pub mod cart_check;
mod docs;
pub use docs::docs;
mod editor;
mod skills;
pub use editor::write_editor_files;
pub use skills::{write_skills, Skills};
#[cfg(not(target_arch = "wasm32"))]
pub mod connect;
#[cfg(not(target_arch = "wasm32"))]
mod folder;
#[cfg(not(target_arch = "wasm32"))]
pub mod link;
#[cfg(not(target_arch = "wasm32"))]
pub mod login;
#[cfg(not(target_arch = "wasm32"))]
pub mod news;
#[cfg(not(target_arch = "wasm32"))]
pub mod setup;
#[cfg(not(target_arch = "wasm32"))]
pub mod song;

/// FNV-1a: stable across Rust versions and platforms, unlike std's hasher.
pub(crate) fn fnv(parts: &[&[u8]]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        for b in part.iter().chain(&[0u8]) {
            h = (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{h:016x}")
}

const MANIFEST: &str = "ppu.json";
/// Where `ppu connect` keeps a folder copy that "Take site's" replaced, one directory per site
/// revision; `ppu new` and a pull list it in `.gitignore`.
pub const BACKUP_DIR: &str = ".ppu-backup";
const VERSION: &str = "ppu.toys/1";
/// The Luau declarations written beside a Luau toy for editors; never one of its files.
pub const LUAU_DECLARATIONS_FILE: &str = "ppu.d.luau";

/// A toy file name: a relative `/`-separated path (`levels/w1-1.luau`) that stays inside the
/// project, never the manifest, with no empty, `.` or `..` segment and no backslash.
fn valid_file_name(name: &str) -> bool {
    name != MANIFEST
        && !name.contains('\\')
        && safe_relative(name)
        && name.split('/').all(|part| !part.is_empty() && part != ".")
}

/// The first name equal to an earlier one ignoring case: one file on macOS and Windows. Pack,
/// unpack and connect all refuse such a pair, so a folder never saves what it cannot bring back.
pub(crate) fn case_twin<'a>(names: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let mut seen = std::collections::HashSet::new();
    names.into_iter().find(|n| !seen.insert(n.to_lowercase()))
}

/// A name the site sends: a valid toy file name that also stays out of dot directories and
/// files (`.git/`, `.vscode/`, `.ppu/`), is not the editor declarations file, and holds no
/// line break (the link is a line-per-entry file). It must hold on macOS and Windows too: never
/// the manifest or declarations file in another case, nor a Windows device name (`CON`,
/// `aux.luau`, `LPT1`), a character Windows refuses (`: < > " | ? *`, control characters) or a
/// segment ending in a dot or space (Windows drops them: `ppu.json.` IS `ppu.json`). The Studio's
/// `fileNameRefusal` (web/src/studio/sketches/openSketch.ts) holds the same rule: change both.
pub(crate) fn valid_site_name(name: &str) -> bool {
    let is = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
    valid_file_name(name)
        && !is(name, MANIFEST)
        && !is(name, LUAU_DECLARATIONS_FILE)
        && !name.contains(|c: char| c < ' ' || ":<>\"|?*".contains(c))
        && !name
            .split('/')
            .any(|part| part.starts_with('.') || part.ends_with(['.', ' ']) || windows_device(part))
}

/// A Windows device name: `CON`, `PRN`, `AUX`, `NUL`, `COM1`-`COM9`, `LPT1`-`LPT9`, with or
/// without an extension (`NUL.tar.luau` is the device too).
fn windows_device(part: &str) -> bool {
    let stem = part
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|p| {
            stem.strip_prefix(p)
                .is_some_and(|n| matches!(n.as_bytes(), [b'1'..=b'9']))
        })
}

fn safe_relative(name: &str) -> bool {
    !name.is_empty()
        && Path::new(name).components().all(|part| {
            matches!(
                part,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Origin {
    id: String,
    revision: i64,
    author_id: String,
}

/// `ppu.json` on disk: a project directory's manifest.
#[derive(Serialize, Deserialize)]
struct Manifest {
    title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    description: String,
    files: Vec<String>,
    #[serde(default)]
    sources: Vec<ManifestSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    origin: Option<Origin>,
    /// Build setting for cartridge exports: video region and update rate in one number
    /// (60 or 30 NTSC, 50 or 25 PAL). Absent leaves it to the exporter's default.
    #[serde(default, rename = "targetHz", skip_serializing_if = "Option::is_none")]
    target_hz: Option<u8>,
    /// Build setting for cartridge exports: the ROM size, `"auto"` or a size in KiB; carried
    /// as written, for the exporter to read. Absent is auto.
    #[serde(default, rename = "romSize", skip_serializing_if = "Option::is_none")]
    rom_size: Option<serde_json::Value>,
    /// Fields this tool does not model (`tags`, ...): kept as written, so editing `ppu.json`
    /// never drops what the user or another tool put there.
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Clone, Serialize, Deserialize)]
struct ManifestSource {
    name: String,
    kind: String,
    #[serde(default = "empty_options", skip_serializing_if = "is_empty_options")]
    options: serde_json::Value,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    meta: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    payload: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    priority_file: Option<String>,
    /// Source fields this tool does not model, kept as written.
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,
}

/// Meta keys the author owns rather than the converter: a file source's meta is rebuilt from the
/// file on every load, and these are carried over from ppu.json. `icon` is the catalogue icon the
/// Studio's Staff view draws for an uploaded sample.
const AUTHOR_META: &[&str] = &["icon"];

fn keep_author_meta(built: &mut serde_json::Value, written: &serde_json::Value) {
    if let Some(built) = built.as_object_mut() {
        for key in AUTHOR_META {
            if let Some(v) = written.get(key) {
                built.insert((*key).into(), v.clone());
            }
        }
    }
}

fn is_empty_options(v: &serde_json::Value) -> bool {
    v.as_object().is_some_and(|o| o.is_empty())
}

fn empty_options() -> serde_json::Value {
    serde_json::json!({})
}

/// A file entry inside a `ppu.toys/1` body.
#[derive(Serialize, Deserialize)]
struct FileEntry {
    name: String,
    source: String,
}

/// A source entry inside a `ppu.toys/1` body: payload is base64.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceEntry {
    name: String,
    kind: String,
    builtin_id: Option<String>,
    options: serde_json::Value,
    meta: serde_json::Value,
    payload: String,
}

/// The `ppu.toys/1` file body (what Studio's Open... accepts).
#[derive(Serialize, Deserialize)]
struct FileBody {
    version: String,
    title: String,
    description: String,
    files: Vec<FileEntry>,
    sources: Vec<SourceEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    origin: Option<Origin>,
    /// Build setting for cartridge exports: video region and update rate in one number
    /// (60 or 30 NTSC, 50 or 25 PAL). Absent leaves it to the exporter's default.
    #[serde(default, rename = "targetHz", skip_serializing_if = "Option::is_none")]
    target_hz: Option<u8>,
    /// Build setting for cartridge exports: the ROM size, `"auto"` or a size in KiB; carried
    /// as written, for the exporter to read. Absent is auto.
    #[serde(default, rename = "romSize", skip_serializing_if = "Option::is_none")]
    rom_size: Option<serde_json::Value>,
}

/// `bytes` in a file under `.ppu/` named for this process, so two connects in one folder never
/// share it (one process writes a folder from one loop, in turn), on disk before this returns:
/// renamed over its target, a crash leaves the old file or the new one, never a renamed empty one.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn write_temp(dir: &Path, name: &str, bytes: &[u8]) -> Result<std::path::PathBuf> {
    use std::io::Write;
    let temp = (dir.join(".ppu")).join(format!("{name}.{}.tmp", std::process::id()));
    std::fs::create_dir_all(dir.join(".ppu"))
        .and_then(|()| std::fs::File::create(&temp))
        .and_then(|mut f| {
            f.write_all(bytes)?;
            f.sync_all()
        })
        .map_err(|e| {
            let _ = std::fs::remove_file(&temp);
            anyhow::Error::new(e).context(format!("cannot write {}", temp.display()))
        })?;
    Ok(temp)
}

/// `ppu.json` written by temp+rename: a crash leaves the old manifest or the new one.
#[cfg(not(target_arch = "wasm32"))]
fn write_manifest(dir: &Path, manifest: &impl Serialize) -> Result<()> {
    let temp = write_temp(
        dir,
        MANIFEST,
        format!("{}\n", serde_json::to_string_pretty(manifest)?).as_bytes(),
    )?;
    std::fs::rename(&temp, dir.join(MANIFEST)).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        anyhow::Error::new(e).context("cannot write ppu.json")
    })
}

fn read_manifest(dir: &Path) -> Result<Manifest> {
    let text =
        std::fs::read_to_string(dir.join(MANIFEST)).context("no ppu.json in this directory")?;
    serde_json::from_str(&text).context("invalid ppu.json")
}

/// Read a project directory (`ppu.json` manifest + its files/payloads) and
/// serialize it to `ppu.toys/1` file text. The result parses identically in
/// Studio's Open... dialog; key order may differ from what Studio writes
/// (serde_json sorts object keys, Studio preserves insertion order).
pub fn pack(dir: &Path) -> Result<String> {
    let manifest = read_manifest(dir)?;

    if !manifest
        .files
        .iter()
        .any(|name| name == "main.lua" || name == "main.luau")
    {
        bail!(
            "{} must list main.lua or main.luau in \"files\" (Studio requires it)",
            dir.join(MANIFEST).display()
        );
    }

    if manifest
        .files
        .iter()
        .any(|name| name == LUAU_DECLARATIONS_FILE)
    {
        bail!(
            "{} lists {LUAU_DECLARATIONS_FILE} in \"files\": it declares the API for editors and is not toy code",
            dir.join(MANIFEST).display()
        );
    }

    let files = manifest
        .files
        .iter()
        .map(|name| {
            // A ppu.json written on Windows may use `\`; the body's names are always `/`.
            let name = name.replace('\\', "/");
            // The names connect brings in and unpack writes: a file it would refuse cannot be saved.
            if !valid_site_name(&name) {
                bail!("unsafe file name {name:?}");
            }
            Ok(FileEntry {
                source: std::fs::read_to_string(dir.join(&name))
                    .with_context(|| format!("cannot read {name}"))?,
                name,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    if let Some(name) = case_twin(files.iter().map(|f| f.name.as_str())) {
        bail!("{name:?} differs from another file's name only in case (one file on macOS and Windows)");
    }

    let sources = manifest
        .sources
        .iter()
        .map(|source| {
            let (bytes, mut meta) = author::source_bytes(dir, source)
                .map_err(|e| anyhow::anyhow!("source {:?}: {e:#}", source.name))?;
            keep_author_meta(&mut meta, &source.meta);
            Ok(SourceEntry {
                name: source.name.clone(),
                kind: source.kind.clone(),
                builtin_id: None,
                options: source.options.clone(),
                meta,
                payload: BASE64.encode(bytes),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let body = FileBody {
        version: VERSION.into(),
        title: manifest.title,
        description: manifest.description,
        files,
        sources,
        origin: manifest.origin,
        target_hz: manifest.target_hz,
        rom_size: manifest.rom_size,
    };
    Ok(serde_json::to_string_pretty(&body)?)
}

/// Parse `ppu.toys/1` file text and write it out as a project directory:
/// manifest `ppu.json`, Lua files in order, `.bin` payloads under
/// `.ppu/sources/`, and for a Luau toy the editor files `ppu new` writes (`ppu.d.luau`, `.luaurc`, `.vscode/`, `.ppu/lib/`). Validates every file name and decodes every payload
/// before writing anything.
pub fn unpack(text: &str, dir: &Path) -> Result<()> {
    let value: serde_json::Value = serde_json::from_str(text).context("not valid JSON")?;
    match value.get("version").and_then(|v| v.as_str()) {
        Some(VERSION) => {}
        Some(other) => bail!("unknown file version {other:?} (expected {VERSION})"),
        None => bail!("not a ppu.toys file (no version field)"),
    }
    let body: FileBody = serde_json::from_value(value).context("invalid ppu.toys file")?;

    for file in &body.files {
        if !valid_site_name(&file.name) {
            bail!("unsafe file name {:?}", file.name);
        }
    }
    if let Some(name) = case_twin(body.files.iter().map(|f| f.name.as_str())) {
        bail!("{name:?} differs from another file's name only in case (one file on macOS and Windows)");
    }

    let payloads = body
        .sources
        .iter()
        .map(|source| {
            BASE64
                .decode(&source.payload)
                .with_context(|| format!("source {:?} has an invalid base64 payload", source.name))
        })
        .collect::<Result<Vec<Vec<u8>>, _>>()?;

    std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    for file in &body.files {
        let path = dir.join(&file.name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        std::fs::write(path, &file.source)
            .with_context(|| format!("cannot write {}", file.name))?;
    }
    let source_dir = dir.join(".ppu/sources");
    std::fs::create_dir_all(&source_dir)
        .with_context(|| format!("cannot create {}", source_dir.display()))?;
    let mut sources = Vec::with_capacity(body.sources.len());
    for (i, (source, bytes)) in body.sources.iter().zip(payloads).enumerate() {
        let payload_path = format!(".ppu/sources/{i}.bin");
        std::fs::write(dir.join(&payload_path), bytes)
            .with_context(|| format!("cannot write {payload_path}"))?;
        sources.push(ManifestSource {
            name: source.name.clone(),
            kind: source.kind.clone(),
            options: source.options.clone(),
            meta: source.meta.clone(),
            payload: Some(payload_path),
            file: None,
            priority_file: None,
            extra: Default::default(),
        });
    }

    let manifest = Manifest {
        title: body.title,
        description: body.description,
        files: body.files.iter().map(|f| f.name.clone()).collect(),
        sources,
        origin: body.origin,
        target_hz: body.target_hz,
        rom_size: body.rom_size,
        extra: Default::default(),
    };
    std::fs::write(
        dir.join(MANIFEST),
        format!("{}\n", serde_json::to_string_pretty(&manifest)?),
    )?;
    if manifest.files.iter().any(|f| f.ends_with(".luau")) {
        write_editor_files(dir)?;
    }
    Ok(())
}

/// Convert a Lua toy (a project directory or a `ppu.toys/1` file) to Luau and write it to `output`:
/// a `.json` path gets a `ppu.toys/1` file, anything else a project directory.
pub fn convert(input: &Path, output: &Path) -> Result<ppu_core::convert::Conversion> {
    let text = if input.is_dir() {
        pack(input)?
    } else {
        std::fs::read_to_string(input)
            .with_context(|| format!("cannot read {}", input.display()))?
    };
    let mut body: serde_json::Value = serde_json::from_str(&text).context("not valid JSON")?;
    let files: Vec<FileEntry> =
        serde_json::from_value(body.get("files").cloned().unwrap_or_default())
            .context("invalid ppu.toys file")?;
    let pairs: Vec<_> = files
        .iter()
        .map(|f| (f.name.as_str(), f.source.as_str()))
        .collect();
    let conversion = ppu_core::convert::convert(&pairs);
    body["files"] = serde_json::to_value(&conversion.files)?;
    let text = serde_json::to_string_pretty(&body)?;
    if output.extension().is_some_and(|e| e == "json") {
        std::fs::write(output, text)
            .with_context(|| format!("cannot write {}", output.display()))?;
    } else {
        unpack(&text, output)?;
    }
    Ok(conversion)
}

#[cfg(test)]
mod tests {
    use super::valid_site_name;

    #[test]
    fn device_names_are_refused_and_lookalikes_are_not() {
        for name in [
            "CON",
            "nul.luau",
            "Com9.x",
            "LPT1.luau",
            "a/PRN.tmj",
            "PPU.json",
            "ppu.json.",
            "ppu.json ",
            "levels./a.luau",
            "a:b.luau",
            "a<b.luau",
            "a>b.luau",
            "a\"b.luau",
            "a|b.luau",
            "a?b.luau",
            "a*b.luau",
            "a\tb.luau",
            "a\u{1}b.luau",
        ] {
            assert!(!valid_site_name(name), "{name}");
        }
        for name in [
            "COM10.luau",
            "console.luau",
            "com0.luau",
            "a/main.luau",
            "a b.luau",
            "lévels/Ünï.luau",
        ] {
            assert!(valid_site_name(name), "{name}");
        }
    }
}
