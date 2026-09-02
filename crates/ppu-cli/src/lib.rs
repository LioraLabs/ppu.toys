use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::{Deserialize, Serialize};
use std::path::Path;

const MANIFEST: &str = "ppu.json";
const VERSION: &str = "ppu.toys/1";

fn valid_file_name(name: &str) -> bool {
    !name.is_empty()
        && name != MANIFEST
        && Path::new(name).components().count() == 1
        && !matches!(name, "." | "..")
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
    #[serde(default)]
    description: String,
    files: Vec<String>,
    #[serde(default)]
    sources: Vec<ManifestSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    origin: Option<Origin>,
}

#[derive(Serialize, Deserialize)]
struct ManifestSource {
    name: String,
    kind: String,
    options: serde_json::Value,
    meta: serde_json::Value,
    payload: String,
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

    if !manifest.files.iter().any(|name| name == "main.lua") {
        bail!(
            "{} must list main.lua in \"files\" (Studio requires it)",
            dir.join(MANIFEST).display()
        );
    }

    let files = manifest
        .files
        .iter()
        .map(|name| {
            if !valid_file_name(name) {
                bail!("unsafe file name {name:?}");
            }
            Ok(FileEntry {
                name: name.clone(),
                source: std::fs::read_to_string(dir.join(name))
                    .with_context(|| format!("cannot read {name}"))?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let sources = manifest
        .sources
        .iter()
        .map(|source| {
            if !safe_relative(&source.payload) {
                bail!("unsafe source payload path {:?}", source.payload);
            }
            let bytes = std::fs::read(dir.join(&source.payload))
                .with_context(|| format!("cannot read source payload {}", source.payload))?;
            Ok(SourceEntry {
                name: source.name.clone(),
                kind: source.kind.clone(),
                builtin_id: None,
                options: source.options.clone(),
                meta: source.meta.clone(),
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
    };
    Ok(serde_json::to_string_pretty(&body)?)
}

/// Parse `ppu.toys/1` file text and write it out as a project directory:
/// manifest `ppu.json`, Lua files in order, and `.bin` payloads under
/// `.ppu/sources/`. Validates every file name and decodes every payload
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
        if !valid_file_name(&file.name) {
            bail!("unsafe file name {:?}", file.name);
        }
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
        std::fs::write(dir.join(&file.name), &file.source)
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
            payload: payload_path,
        });
    }

    let manifest = Manifest {
        title: body.title,
        description: body.description,
        files: body.files.iter().map(|f| f.name.clone()).collect(),
        sources,
        origin: body.origin,
    };
    std::fs::write(
        dir.join(MANIFEST),
        format!("{}\n", serde_json::to_string_pretty(&manifest)?),
    )?;
    Ok(())
}
