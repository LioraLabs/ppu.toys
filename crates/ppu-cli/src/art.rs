//! `ppu art check` and `ppu art palette sync`: the one validator and palette tool editors call.
//! Thin over ppu-core: the rules are `check_indexed` (what import refuses) and `replace_palette`.

use super::{
    folder, read_manifest, safe_relative, write_manifest, Manifest, ManifestSource, BACKUP_DIR,
    MANIFEST,
};
use anyhow::{anyhow, bail, ensure, Context, Result};
use ppu_core::import::keep::is_snes_art;
use ppu_core::import::Overflow;
use ppu_core::render::{
    alpha_of, decode_png, encode_png, is_transparent_entry, own_entries, plte_of, render_png,
    replace_palette, DecodedPng, IndexedImage, IndexedPng,
};
use ppu_core::rows::{self, Art};
use ppu_core::{
    check_indexed, convert_png_source, gpl, rgb15, row_size_for, unpack_rgb15, Budgets,
    ConvertOptions, Finding, PaletteSource, Rule, SourceKind, SourceReport, UnitSize,
};
use serde::Serialize;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// The machine contract's version: every `--json` answer carries it as `protocol`. Raised only by
/// a change an older editor plugin would misread.
pub const PROTOCOL: u32 = 1;

/// A refusal with a `reason` code a caller branches on (the message is for people and may be
/// reworded). Without one, `run` reports `refused` for an importer refusal and `error` otherwise.
#[derive(Debug)]
struct Coded(&'static str, String);

impl std::fmt::Display for Coded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.1)
    }
}

impl std::error::Error for Coded {}

fn coded(reason: &'static str, message: String) -> anyhow::Error {
    anyhow::Error::new(Coded(reason, message))
}

/// A command line `ppu art` does not take, said in `message`.
fn bad_usage(message: impl Into<String>) -> anyhow::Error {
    coded("usage", message.into())
}

/// A flag's value as a number, or a usage error naming the flag.
fn number<T: std::str::FromStr>(value: &str, flag: &str) -> Result<T> {
    value
        .parse()
        .map_err(|_| bad_usage(format!("invalid {flag} {value:?}")))
}

fn usage() -> anyhow::Error {
    coded("usage", USAGE.into())
}

/// A project PNG's source entry: what the manifest says about the file.
pub struct ArtEntry {
    pub dir: PathBuf,
    pub name: String,
    /// The PNG's path as the manifest writes it.
    pub file: String,
    pub kind: SourceKind,
    pub options: ConvertOptions,
    pub extbg: bool,
    /// `options.palette`, read beside [`ConvertOptions`] (which is palette-blind).
    pub palette_name: Option<String>,
    /// That palette entry's parsed `.gpl`.
    pub palette: Option<PaletteSource>,
    /// Where that `.gpl` lives (absolute).
    pub palette_path: Option<PathBuf>,
    pub priority_file: Option<String>,
}

/// The same file, compared by its resolved directory (the file itself may not exist yet).
fn same_file(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        let parent = p
            .parent()
            .filter(|d| !d.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        parent
            .canonicalize()
            .ok()
            .zip(p.file_name().map(|n| n.to_owned()))
    };
    norm(a).is_some() && norm(a) == norm(b)
}

/// The directory of the nearest `ppu.json` at or above `file`.
fn project_dir(file: &Path) -> Result<PathBuf> {
    let abs = std::path::absolute(file)?;
    abs.ancestors()
        .skip(1)
        .find(|d| d.join(MANIFEST).is_file())
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            coded(
                "not_project",
                format!("no {MANIFEST} in any directory above {}", file.display()),
            )
        })
}

fn source_path(dir: &Path, name: &str) -> Result<PathBuf> {
    let name = name.replace('\\', "/");
    ensure!(safe_relative(&name), "unsafe source path {name:?}");
    Ok(dir.join(name))
}

pub(crate) fn palette_option(s: &ManifestSource) -> Result<Option<String>> {
    match s.options.get("palette") {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(n)) => Ok(Some(n.clone())),
        Some(v) => bail!(
            "source {:?}: options.palette must be a palette source's name, got {v}",
            s.name
        ),
    }
}

/// The `.gpl` of the manifest's palette source `name`.
pub(crate) fn load_palette(
    dir: &Path,
    sources: &[ManifestSource],
    name: &str,
) -> Result<(PaletteSource, PathBuf)> {
    let entry = sources.iter().find(|s| s.name == name).with_context(|| {
        format!("options.palette names {name:?}, but {MANIFEST} has no source of that name")
    })?;
    ensure!(
        entry.kind == "palette",
        "options.palette names {name:?}, which is a {} source, not a palette",
        entry.kind
    );
    let file = entry
        .file
        .as_deref()
        .with_context(|| format!("palette source {name:?} has no .gpl file"))?;
    let path = source_path(dir, file)?;
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read {}", path.display()))?;
    let pal = gpl::parse(&text).map_err(|e| anyhow!("{}: {e}", path.display()))?;
    Ok((pal, path))
}

/// What `ppu art check` finds wrong with `png`, one message per finding (empty: import accepts it).
pub fn findings(png: &Path) -> Result<Vec<String>> {
    let entry = find_entry(png)?;
    Ok(report_lines(&check_entry(png, entry, None)?))
}

/// What `ppu art check` finds wrong with the art `source` lists, one rule and message per finding
/// (empty: import accepts it). The rules a sync holds a folder's art to before it sends anything.
pub(crate) fn findings_of(
    dir: &Path,
    manifest: &Manifest,
    source: &ManifestSource,
) -> Result<Vec<(Rule, String)>> {
    let entry = entry_of(dir, manifest, source)?;
    let png = source_path(dir, &entry.file)?;
    let report = check_entry(&png, entry, None)?;
    let rules = report.findings.iter().map(|f| f.rule);
    Ok(rules.zip(report_lines(&report)).collect())
}

/// One `file: rule at x,y: message` line per finding (what `human` prints too).
fn report_lines(report: &Report) -> Vec<String> {
    report
        .findings
        .iter()
        .map(|f| {
            let at =
                f.x.zip(f.y)
                    .map(|(x, y)| format!(" at {x},{y}"))
                    .unwrap_or_default();
            format!("{}: {}{at}: {}", report.file, rule_name(f.rule), f.message)
        })
        .collect()
}

/// Find `png` in the nearest `ppu.json`: its kind, options and palette. Refuses a PNG no source
/// lists, a PNG two sources list (one PNG, one source: an editor needs one answer), an unknown
/// kind, and a palette name the manifest does not hold.
pub fn find_entry(png: &Path) -> Result<ArtEntry> {
    let dir = project_dir(png)?;
    let manifest = read_manifest(&dir)?;
    let source = listed_source(&dir, &manifest, png)?.ok_or_else(|| {
        coded(
            "not_listed",
            format!(
                "{} is not the file of any source in {}",
                png.display(),
                dir.join(MANIFEST).display()
            ),
        )
    })?;
    entry_of(&dir, &manifest, source)
}

/// The source that lists `png` as its file, if any; refuses two (one PNG, one source).
fn listed_source<'a>(
    dir: &Path,
    manifest: &'a Manifest,
    png: &Path,
) -> Result<Option<&'a ManifestSource>> {
    let mut listing = manifest.sources.iter().filter(|s| {
        s.file
            .as_deref()
            .is_some_and(|f| source_path(dir, f).is_ok_and(|p| same_file(&p, png)))
    });
    let source = listing.next();
    if let (Some(source), Some(other)) = (source, listing.next()) {
        return Err(coded(
            "listed_twice",
            format!(
                "{} is listed by sources {:?} and {:?}: one PNG, one source",
                png.display(),
                source.name,
                other.name
            ),
        ));
    }
    Ok(source)
}

/// The entry of one source: its kind, options and palette. Refuses an unknown kind, a kind that
/// is not indexed art, and a palette name the manifest does not hold.
pub(crate) fn entry_of(
    dir: &Path,
    manifest: &Manifest,
    source: &ManifestSource,
) -> Result<ArtEntry> {
    let kind: SourceKind = source
        .kind
        .parse()
        .map_err(|e: String| anyhow!("source {:?}: {e}", source.name))?;
    ensure!(
        row_size_for(kind, 4).is_some(),
        "source {:?} is a {} source, not indexed art",
        source.name,
        source.kind
    );
    let options: ConvertOptions = serde_json::from_value(source.options.clone())
        .with_context(|| format!("source {:?}: invalid options", source.name))?;
    let palette_name = palette_option(source)?;
    let (palette, palette_path) = palette_name
        .as_deref()
        .map(|n| load_palette(dir, &manifest.sources, n))
        .transpose()?
        .unzip();
    Ok(ArtEntry {
        file: source.file.clone().unwrap_or_default(),
        name: source.name.clone(),
        extbg: kind == SourceKind::M7 && options.extbg == Some(true),
        kind,
        options,
        palette_name,
        palette,
        palette_path,
        priority_file: source.priority_file.clone(),
        dir: dir.to_path_buf(),
    })
}

#[derive(Serialize)]
struct Report {
    ok: bool,
    protocol: u32,
    source: String,
    file: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    palette: Option<String>,
    /// The named palette's `.gpl`, absolute.
    #[serde(skip_serializing_if = "Option::is_none")]
    palette_file: Option<PathBuf>,
    /// The `.gpl`'s colour-line count: exactly what `palette write` expects on stdin.
    #[serde(skip_serializing_if = "Option::is_none")]
    palette_colours: Option<usize>,
    /// The PLTE the art means with its named palette, as 8-bit `[r, g, b]`: what an editor loads,
    /// not the raw `.gpl` (Mode 7 EXTBG mirrors 128 colours to 256; the rule is `plte_of`).
    #[serde(skip_serializing_if = "Option::is_none")]
    plte: Option<Vec<[u8; 3]>>,
    /// How many leading `plte` entries are the `.gpl`'s own colours (`own_entries`): what an
    /// editor saves back, keeping the `.gpl`'s lines past them; the rest of `plte` mirrors them.
    #[serde(skip_serializing_if = "Option::is_none")]
    plte_own: Option<usize>,
    /// Palette row size in entries for the art's kind and bit depth (keep path only).
    #[serde(skip_serializing_if = "Option::is_none")]
    row: Option<u16>,
    /// The PLTE indices that are transparent for this art (keep path only): what an editor sets
    /// to alpha 0 and guards. The rule is `is_transparent_entry`, not restated here.
    #[serde(skip_serializing_if = "Option::is_none")]
    transparent: Option<Vec<usize>>,
    /// The source's stored conversion options (absent ones stay absent): what an editor prefills
    /// its convert dialog from.
    options: ConvertOptions,
    path: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    unit: Option<UnitSize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    budgets: Option<Budgets>,
    findings: Vec<Finding>,
}

/// What `png` would be refused for. With `stdin`, raw width x height index bytes stand in for its
/// pixels (what an editor holds), as SNES art; without, the file is read and, if it is not SNES
/// art, run through the quantizing import (which has no rules to list, only an error).
fn check(png: &Path, stdin: Option<(u32, u32, Vec<u8>)>) -> Result<Report> {
    check_entry(png, find_entry(png)?, stdin)
}

/// The index bytes an editor holds, as the PNG they would make: the named palette's colours,
/// else the file's own.
fn stdin_png(
    png: &Path,
    entry: &ArtEntry,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
) -> Result<IndexedPng> {
    ensure!(
        pixels.len() == width as usize * height as usize,
        "stdin holds {} bytes, but {width}x{height} needs {}",
        pixels.len(),
        width as usize * height as usize
    );
    let palette = match &entry.palette {
        Some(p) => plte_of(p, entry.extbg),
        None => match decode_png(
            &std::fs::read(png).with_context(|| format!("cannot read {}", png.display()))?,
        )
        .map_err(|e| anyhow!("{}: {e}", png.display()))?
        {
            DecodedPng::Indexed(ip) => ip.image.palette,
            DecodedPng::Rgba { .. } => bail!(
                "{} has no PLTE and no palette is named: nothing to check the indices against",
                png.display()
            ),
        },
    };
    let rs = row_size_for(entry.kind, entry.options.bit_depth.unwrap_or(4))
        .expect("entry_of checked the row size");
    let alpha = alpha_of(palette.len(), rs as usize, entry.extbg);
    let image = IndexedImage {
        width,
        height,
        pixels,
        palette,
        row_size: rs,
        extbg: entry.extbg,
    };
    Ok(IndexedPng { image, alpha })
}

fn check_entry(png: &Path, entry: ArtEntry, stdin: Option<(u32, u32, Vec<u8>)>) -> Result<Report> {
    let on_disk = || std::fs::read(png).with_context(|| format!("cannot read {}", png.display()));
    let from_file = stdin.is_none();
    let mut plte_len = 0;
    let (path, art) = match stdin {
        Some((width, height, pixels)) => {
            let ip = stdin_png(png, &entry, width, height, pixels)?;
            plte_len = ip.image.palette.len();
            (
                "keep",
                Some(check_indexed(entry.kind, &entry.options, &ip, None).map_err(|e| anyhow!(e))?),
            )
        }
        None => match decode_png(&on_disk()?).map_err(|e| anyhow!("{}: {e}", png.display()))? {
            // A PNG that names a palette is meant to be SNES art: check it as such, so a lost
            // tRNS is reported with the rules the check can still see, not quantized away.
            DecodedPng::Indexed(ip)
                if entry.palette.is_some() || is_snes_art(entry.kind, &entry.options, &ip) =>
            {
                plte_len = entry
                    .palette
                    .as_ref()
                    .map_or(ip.image.palette.len(), |p| plte_of(p, entry.extbg).len());
                (
                    "keep",
                    Some(
                        check_indexed(entry.kind, &entry.options, &ip, entry.palette.as_ref())
                            .map_err(|e| anyhow!(e))?,
                    ),
                )
            }
            _ => ("quantize", None),
        },
    };
    let mut findings = art.as_ref().map(|c| c.findings.clone()).unwrap_or_default();
    if let (None, Some(name)) = (&art, &entry.palette_name) {
        findings.push(Finding::new(
            Rule::NotSnesArt,
            format!("not SNES art: a full-colour PNG cannot use palette {name:?}; it needs an indexed PNG"),
        ));
    }
    // Import also reads a priority mask, and the quantizer has its own errors: when check has
    // nothing to say, run the import itself so "ok" stays "import accepts".
    if findings.is_empty() && from_file && (art.is_none() || entry.priority_file.is_some()) {
        let mask = entry
            .priority_file
            .as_deref()
            .map(|n| -> Result<Vec<u8>> {
                let p = source_path(&entry.dir, n)?;
                std::fs::read(&p).with_context(|| format!("cannot read {}", p.display()))
            })
            .transpose()?;
        if let Err(e) = convert_png_source(entry.kind, &entry.options, &on_disk()?, mask.as_deref())
        {
            findings.push(Finding::new(Rule::Import, e));
        }
    }
    let palette_colours = entry
        .palette_path
        .as_ref()
        .map(|p| {
            std::fs::read_to_string(p)
                .map(|t| gpl::colour_count(&t))
                .with_context(|| format!("cannot read {}", p.display()))
        })
        .transpose()?;
    let plte = entry.palette.as_ref().map(|p| {
        plte_of(p, entry.extbg)
            .into_iter()
            .map(|c| {
                let [r, g, b, _] = unpack_rgb15(c);
                [r, g, b]
            })
            .collect()
    });
    Ok(Report {
        ok: findings.is_empty(),
        protocol: PROTOCOL,
        source: entry.name,
        file: entry.file,
        kind: entry.kind.name().into(),
        palette: entry.palette_name,
        palette_file: entry.palette_path,
        palette_colours,
        plte,
        plte_own: palette_colours.map(|n| own_entries(n, entry.extbg)),
        row: art.as_ref().and(row_size_for(
            entry.kind,
            entry.options.bit_depth.unwrap_or(4),
        )),
        transparent: art.as_ref().map(|_| {
            let rs = row_size_for(entry.kind, entry.options.bit_depth.unwrap_or(4))
                .expect("entry_of checked the row size");
            (0..plte_len)
                .filter(|&k| is_transparent_entry(rs as usize, entry.extbg, k))
                .collect()
        }),
        options: entry.options.clone(),
        path,
        unit: art.as_ref().map(|c| c.unit),
        budgets: art.as_ref().map(|c| c.budgets),
        findings,
    })
}

fn rule_name(r: Rule) -> String {
    serde_json::to_value(r)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default()
}

fn human(r: &Report) -> String {
    let mut out = String::new();
    for line in report_lines(r) {
        out += &line;
        out += "\n";
    }
    if r.ok {
        out += &format!("{}: ok ({})", r.file, r.path);
        if let Some(b) = r.budgets {
            out += &format!(
                ", tiles {}/{}, palette rows {}/{}",
                b.tiles.used, b.tiles.max, b.palettes.used, b.palettes.max
            );
        }
        out += "\n";
    }
    out
}

/// Rewrite the PLTE/tRNS of every PNG whose source names the palette `gpl_path` holds. Every PNG
/// is rewritten in memory first, so a refusal changes nothing; each file then lands by rename.
pub fn palette_sync(gpl_path: &Path) -> Result<Vec<PathBuf>> {
    palette_sync_with(gpl_path, &mut |_| Ok(()))
}

/// [`palette_sync`], calling `before` with each PNG just before its rewrite lands (a caller keeps
/// a copy there); an error from it stops the sync before that file is touched.
pub fn palette_sync_with(
    gpl_path: &Path,
    before: &mut dyn FnMut(&Path) -> Result<()>,
) -> Result<Vec<PathBuf>> {
    let text = std::fs::read_to_string(gpl_path)
        .with_context(|| format!("cannot read {}", gpl_path.display()))?;
    let rewrites = sync_plan(gpl_path, &text)?;
    land(rewrites, before)
}

/// Every PNG rewrite that `text` as the `.gpl`'s content calls for, in memory: a refusal here
/// changes nothing.
fn sync_plan(gpl_path: &Path, text: &str) -> Result<Vec<(PathBuf, Vec<u8>)>> {
    let dir = project_dir(gpl_path)?;
    let manifest = read_manifest(&dir)?;
    let name = &manifest
        .sources
        .iter()
        .find(|s| {
            s.kind == "palette"
                && s.file
                    .as_deref()
                    .is_some_and(|f| source_path(&dir, f).is_ok_and(|p| same_file(&p, gpl_path)))
        })
        .ok_or_else(|| {
            coded(
                "not_listed",
                format!(
                    "{} is not the file of any palette source in {}",
                    gpl_path.display(),
                    dir.join(MANIFEST).display()
                ),
            )
        })?
        .name;
    let pal = gpl::parse(text).map_err(|e| anyhow!("{}: {e}", gpl_path.display()))?;
    let mut rewrites = vec![];
    for s in &manifest.sources {
        if palette_option(s)?.as_deref() != Some(name) {
            continue;
        }
        let kind: SourceKind = s
            .kind
            .parse()
            .map_err(|e: String| anyhow!("source {:?}: {e}", s.name))?;
        let opts: ConvertOptions = serde_json::from_value(s.options.clone())
            .with_context(|| format!("source {:?}: invalid options", s.name))?;
        let file = s.file.as_deref().with_context(|| {
            format!(
                "source {:?} names palette {name:?} but has no PNG file",
                s.name
            )
        })?;
        match row_size_for(kind, opts.bit_depth.unwrap_or(4)) {
            Some(rs) if rs == pal.row_size => {}
            Some(rs) => bail!(
                "source {:?}: its rows are {rs} colours, but palette {name:?} has {}",
                s.name,
                pal.row_size
            ),
            None => bail!(
                "source {:?} (kind {:?}) has no palette rows, but it names palette {name:?}",
                s.name,
                s.kind
            ),
        }
        let path = source_path(&dir, file)?;
        let png =
            std::fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?;
        let out = replace_palette(
            &png,
            &pal,
            kind == SourceKind::M7 && opts.extbg == Some(true),
        )
        .map_err(|e| anyhow!("source {:?} ({file}): {e}", s.name))?;
        // An editor watching the file would reload it for nothing.
        if out != png {
            rewrites.push((path, out));
        }
    }
    Ok(rewrites)
}

/// Each rewrite lands by rename, `before` called with the path first.
fn land(
    rewrites: Vec<(PathBuf, Vec<u8>)>,
    before: &mut dyn FnMut(&Path) -> Result<()>,
) -> Result<Vec<PathBuf>> {
    for (path, bytes) in &rewrites {
        before(path)?;
        replace_file(path, bytes)?;
    }
    Ok(rewrites.into_iter().map(|(p, _)| p).collect())
}

/// `bytes` over `path` by rename: a crash leaves the old file or the new one.
fn replace_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension().and_then(|e| e.to_str()).unwrap_or("")
    ));
    std::fs::write(&tmp, bytes)
        .and_then(|_| std::fs::rename(&tmp, path))
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })
        .with_context(|| format!("cannot write {}", path.display()))
}

/// `ppu art palette write`: `colours` (8-bit, one per colour line of the `.gpl`) snapped to RGB555
/// into the file, then the sync. Everything is planned in memory first, so a refusal changes
/// nothing; the `.gpl` text is left alone when nothing changes.
pub fn palette_write(gpl_path: &Path, colours: &[[u8; 3]]) -> Result<Vec<PathBuf>> {
    let text = std::fs::read_to_string(gpl_path)
        .with_context(|| format!("cannot read {}", gpl_path.display()))?;
    let next = gpl::recolour(&text, colours).map_err(|e| anyhow!("{}: {e}", gpl_path.display()))?;
    let rewrites = sync_plan(gpl_path, &next)?;
    if next != text {
        replace_file(gpl_path, next.as_bytes())?;
    }
    land(rewrites, &mut |_| Ok(()))
}

/// The tile/cell geometry of `png`'s art with `pixels` (the check's `unit`, so rows and the check
/// agree), and the art as an editor holds it.
fn rows_art(
    png: &Path,
    entry: &ArtEntry,
    (width, height, pixels): (u32, u32, Vec<u8>),
) -> Result<(IndexedPng, (usize, usize))> {
    let ip = stdin_png(png, entry, width, height, pixels)?;
    let unit = check_indexed(entry.kind, &entry.options, &ip, None)
        .map_err(|e| anyhow!(e))?
        .unit;
    Ok((ip, (unit.width as usize, unit.height as usize)))
}

/// `ppu art rows`: the art's index bytes with `op` applied, and what it did in words.
fn rows_op(png: &Path, op: &RowsOp, stdin: (u32, u32, Vec<u8>)) -> Result<(Vec<u8>, Vec<String>)> {
    let entry = find_entry(png)?;
    let (ip, unit) = rows_art(png, &entry, stdin)?;
    let img = &ip.image;
    let art = Art {
        pixels: &img.pixels,
        width: img.width as usize,
        height: img.height as usize,
        unit,
        rs: img.row_size as usize,
    };
    match *op {
        RowsOp::Set { row, rect } => {
            let out = rows::set_row(&art, img.palette.len().div_ceil(art.rs), row, rect)
                .map_err(|e| anyhow!(e))?;
            let note = format!(
                "set row {row} on the {}s the rectangle touches",
                unit_name(unit)
            );
            Ok((out, vec![note]))
        }
        RowsOp::Fix => {
            let f = rows::fix_rows(&art, &img.palette).map_err(|e| anyhow!(e))?;
            let mut lines = vec![format!(
                "moved {} pixels into their {}'s row, {} {}s keep strays",
                f.moved,
                unit_name(unit),
                f.left.len(),
                unit_name(unit)
            )];
            lines.extend(f.left);
            Ok((f.pixels, lines))
        }
    }
}

fn unit_name(unit: (usize, usize)) -> &'static str {
    if unit == (8, 8) {
        "tile"
    } else {
        "cell"
    }
}

enum RowsOp {
    Set {
        row: usize,
        rect: (usize, usize, usize, usize),
    },
    Fix,
}

/// The name of the palette source for the `.gpl` at `gpl_path` (its `text`): the one that lists it,
/// else a new one, pushed on `sources`, named by the file's stem. Refuses a file outside the
/// project, an unsafe path, and a stem some source already has.
fn list_palette(
    dir: &Path,
    sources: &mut Vec<ManifestSource>,
    gpl_path: &Path,
    text: &str,
) -> Result<String> {
    let gpl_abs = std::path::absolute(gpl_path)?;
    let listed = sources.iter().find(|s| {
        s.kind == "palette"
            && s.file
                .as_deref()
                .is_some_and(|f| source_path(dir, f).is_ok_and(|p| same_file(&p, &gpl_abs)))
    });
    if let Some(s) = listed {
        return Ok(s.name.clone());
    }
    let rel = rel_path(dir, &gpl_abs)?;
    let stem = gpl_abs
        .file_stem()
        .and_then(|n| n.to_str())
        .with_context(|| format!("{} has no usable name", gpl_path.display()))?;
    ensure!(
        sources.iter().all(|s| s.name != stem),
        "{MANIFEST} already has a source named {stem:?}: rename the palette file"
    );
    let (_, meta) = gpl::convert_palette(text).map_err(|e| anyhow!(e))?;
    sources.push(ManifestSource {
        name: stem.into(),
        kind: "palette".into(),
        options: serde_json::json!({}),
        meta: serde_json::to_value(meta)?,
        payload: None,
        file: Some(rel),
        priority_file: None,
        extra: Default::default(),
    });
    Ok(stem.to_string())
}

/// `abs` as a `/`-separated path under the project `dir`.
fn rel_path(dir: &Path, abs: &Path) -> Result<String> {
    let rel = abs
        .strip_prefix(dir)
        .with_context(|| format!("{} is outside the project", abs.display()))?
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    ensure!(safe_relative(&rel), "unsafe path {rel:?}");
    Ok(rel)
}

/// What `ppu art palette set` found.
enum Switched {
    Pixels(Vec<u8>),
    /// Why the palette does not fit: one line per colour it lacks (or unit it splits).
    Unmatched(Vec<String>),
}

/// `ppu art palette set`: the art's pixels repainted for the `.gpl` at `gpl_path`, and the
/// manifest pointing the PNG's source at it. A palette that lacks a colour changes nothing.
fn palette_set(png: &Path, gpl_path: &Path, stdin: (u32, u32, Vec<u8>)) -> Result<Switched> {
    let entry = find_entry(png)?;
    ensure!(
        !entry.extbg,
        "Mode 7 EXTBG art cannot switch palette: entry 128 is its priority"
    );
    let text = std::fs::read_to_string(gpl_path)
        .with_context(|| format!("cannot read {}", gpl_path.display()))?;
    let pal = gpl::parse(&text).map_err(|e| anyhow!("{}: {e}", gpl_path.display()))?;
    let rs = row_size_for(entry.kind, entry.options.bit_depth.unwrap_or(4))
        .expect("entry_of checked the row size");
    ensure!(
        pal.row_size == rs,
        "{}: its rows are {} colours, but this art's are {rs}",
        gpl_path.display(),
        pal.row_size
    );
    let (ip, unit) = rows_art(png, &entry, stdin)?;
    let img = &ip.image;
    let art = Art {
        pixels: &img.pixels,
        width: img.width as usize,
        height: img.height as usize,
        unit,
        rs: rs as usize,
    };
    let pixels = match rows::switch_palette(&art, &img.palette, &plte_of(&pal, false)) {
        Ok(p) => p,
        Err(lines) => return Ok(Switched::Unmatched(lines)),
    };
    let mut manifest = read_manifest(&entry.dir)?;
    let listed_before = manifest.sources.len();
    let name = list_palette(&entry.dir, &mut manifest.sources, gpl_path, &text)?;
    let at = manifest
        .sources
        .iter()
        .position(|s| s.name == entry.name)
        .expect("find_entry found it");
    let source = &manifest.sources[at];
    ensure!(
        source.options.is_null() || source.options.is_object(),
        "source {:?}: options must be an object, got {}",
        source.name,
        source.options
    );
    // The PNG first, then the manifest: disk is never half-switched (a manifest naming the new
    // palette over pixels of the old one would let the next palette sync recolour the art).
    let switched = IndexedImage {
        width: img.width,
        height: img.height,
        pixels: pixels.clone(),
        palette: plte_of(&pal, false),
        row_size: rs,
        extbg: false,
    };
    replace_file(png, &encode_png(&switched))?;
    // Edited as plain JSON, not through the struct: every field stays (keys come back sorted).
    let mut raw: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(entry.dir.join(MANIFEST)).context("cannot read ppu.json")?,
    )
    .context("invalid ppu.json")?;
    raw["sources"][at]["options"]["palette"] = name.into();
    if manifest.sources.len() > listed_before {
        raw["sources"]
            .as_array_mut()
            .expect("read_manifest parsed sources")
            .push(serde_json::to_value(&manifest.sources[listed_before])?);
    }
    write_manifest(&entry.dir, &raw)?;
    Ok(Switched::Pixels(pixels))
}

/// A name that is safe as a source name and a file stem.
fn safe_name(what: &str, name: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
        "unsafe {what} name {name:?}: use letters, digits, '_' and '-'"
    );
    Ok(())
}

/// Where `ppu art new` gets its palette.
enum NewPalette {
    /// A `.gpl` file, listed in `ppu.json` or added to it.
    Existing(PathBuf),
    /// A new `palettes/<name>.gpl`: entry 0 and a grey ramp.
    Named(String),
}

/// What `ppu art new` was asked for.
struct NewArt {
    png: PathBuf,
    kind: SourceKind,
    size: (u32, u32),
    bpp: Option<u8>,
    cell: Option<u8>,
    palette: NewPalette,
}

/// The kind, `--bpp` and `--cell` an `art new` or `art convert` was asked for, checked: the
/// bit depth it means and the `options` (`bit_depth`, `cell_size`) its source entry carries.
fn art_options(
    kind: SourceKind,
    bpp: Option<u8>,
    cell: Option<u8>,
) -> Result<(u8, serde_json::Value)> {
    let (legal, default): (&[u8], u8) = match kind {
        SourceKind::Bg | SourceKind::Sheet => (&[2, 4, 8], 4),
        SourceKind::Obj => (&[4], 4),
        SourceKind::M7 => (&[8], 8),
        k => bail!("--kind must be bg, sheet, obj or m7, not {}", k.name()),
    };
    let bpp = bpp.unwrap_or(default);
    ensure!(
        legal.contains(&bpp),
        "--bpp {bpp} is not legal for a {} source: use {}",
        kind.name(),
        legal
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    );
    ensure!(
        cell.is_none() || kind == SourceKind::Obj,
        "--cell only goes with --kind obj"
    );
    if let Some(c) = cell {
        ensure!(
            c % 8 == 0 && (8..=128).contains(&c),
            "--cell must be a multiple of 8 from 8 to 128 (got {c})"
        );
    }
    let mut options = serde_json::json!({"bit_depth": bpp});
    if let Some(c) = cell {
        options["cell_size"] = c.into();
    }
    Ok((bpp, options))
}

/// `ppu art new`: an empty (every pixel index 0) indexed PNG of the asked size and its source in
/// `ppu.json`, with the palette's PLTE. Everything is planned in memory first, and the planned
/// art is run through the check, so a refusal writes nothing and what lands passes `art check`;
/// then the new files land (PNG, `.gpl`) and `ppu.json` last. Returns the files written.
fn art_new(a: NewArt) -> Result<Vec<PathBuf>> {
    let NewArt {
        png,
        kind,
        size,
        bpp,
        cell,
        palette,
    } = a;
    let (bpp, mut options) = art_options(kind, bpp, cell)?;
    let tile = u32::from(cell.unwrap_or(8));
    // Mode 7's 1024 px is the widest any kind holds; refuse before the pixel buffer is allocated.
    ensure!(
        size.0 <= MAX_SIDE && size.1 <= MAX_SIDE,
        "--size {}x{}: no art is larger than {MAX_SIDE} px on a side",
        size.0,
        size.1
    );
    ensure!(
        size.0 > 0 && size.1 > 0 && size.0 % tile == 0 && size.1 % tile == 0,
        "--size {}x{} must be a positive multiple of {tile} in each direction",
        size.0,
        size.1
    );
    ensure!(
        png.extension().and_then(|e| e.to_str()) == Some("png"),
        "{} is not a .png path",
        png.display()
    );
    let stem = png
        .file_stem()
        .and_then(|n| n.to_str())
        .with_context(|| format!("{} has no usable name", png.display()))?;
    safe_name("source", stem)?;
    let dir = project_dir(&png)?;
    let png_abs = std::path::absolute(&png)?;
    let rel = rel_path(&dir, &png_abs)?;
    ensure!(
        std::fs::symlink_metadata(&png_abs).is_err(),
        "{} already exists",
        png.display()
    );
    let mut manifest = read_manifest(&dir)?;
    let listed_before = manifest.sources.len();
    let rs = row_size_for(kind, bpp).expect("the kind is indexed art");
    let mut files = vec![];
    let (pal, pal_name) = match palette {
        NewPalette::Existing(path) => {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("cannot read {}", path.display()))?;
            let pal = gpl::parse(&text).map_err(|e| anyhow!("{}: {e}", path.display()))?;
            ensure!(
                pal.row_size == rs,
                "{}: its rows are {} colours, but this art's are {rs}",
                path.display(),
                pal.row_size
            );
            let name = list_palette(&dir, &mut manifest.sources, &path, &text)?;
            (pal, name)
        }
        NewPalette::Named(name) => {
            safe_name("palette", &name)?;
            // A 256-colour block holds as few as it needs: 16 is plenty to start.
            let n = usize::from(rs.min(16));
            let ramp = (0..n)
                .map(|i| match i {
                    0 => 0,
                    _ => {
                        let v = (i * 255 / (n - 1)) as u8;
                        rgb15(v, v, v)
                    }
                })
                .collect();
            let pal = PaletteSource {
                row_size: rs,
                rows: vec![ramp],
            };
            let path = dir.join("palettes").join(format!("{name}.gpl"));
            ensure!(
                std::fs::symlink_metadata(&path).is_err(),
                "{} already exists",
                path.display()
            );
            let text = gpl::write(&pal, &name);
            list_palette(&dir, &mut manifest.sources, &path, &text)?;
            files.push((path, text.into_bytes()));
            (pal, name)
        }
    };
    ensure!(
        pal_name != stem,
        "the new palette's name must differ from the art's: both are {stem:?}"
    );
    ensure!(
        manifest.sources.iter().all(|s| s.name != stem),
        "{MANIFEST} already has a source named {stem:?}: pick another file name"
    );
    options["palette"] = pal_name.clone().into();
    let image = IndexedImage {
        width: size.0,
        height: size.1,
        pixels: vec![0; size.0 as usize * size.1 as usize],
        palette: plte_of(&pal, false),
        row_size: rs,
        extbg: false,
    };
    let planned = IndexedPng {
        alpha: alpha_of(image.palette.len(), rs as usize, false),
        image,
    };
    let opts: ConvertOptions = serde_json::from_value(options.clone())?;
    let found = check_indexed(kind, &opts, &planned, Some(&pal)).map_err(|e| anyhow!(e))?;
    if let Some(f) = found.findings.first() {
        bail!(
            "an empty {}x{} {} would fail the check: {}",
            size.0,
            size.1,
            kind.name(),
            f.message
        );
    }
    manifest.sources.push(ManifestSource {
        name: stem.into(),
        kind: kind.name().into(),
        options,
        meta: serde_json::Value::Null,
        payload: None,
        file: Some(rel),
        priority_file: None,
        extra: Default::default(),
    });
    // Edited as plain JSON, not through the struct: every field stays (keys come back sorted).
    let mut raw: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join(MANIFEST)).context("cannot read ppu.json")?,
    )
    .context("invalid ppu.json")?;
    let listed = raw["sources"]
        .as_array_mut()
        .expect("read_manifest parsed sources");
    for s in &manifest.sources[listed_before..] {
        listed.push(serde_json::to_value(s)?);
    }
    // New files first, the manifest last: a failure leaves files nothing names, never a source
    // without its file.
    files.push((png_abs, encode_png(&planned.image)));
    for (path, bytes) in &files {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        replace_file(path, bytes)?;
    }
    write_manifest(&dir, &raw)?;
    let mut written: Vec<PathBuf> = files.into_iter().map(|(p, _)| p).collect();
    written.push(dir.join(MANIFEST));
    Ok(written)
}

/// An import the importer refused: exit 1 and its message, nothing written.
#[derive(Debug)]
struct Refused(String);

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Refused {}

/// What `ppu art convert` was asked for.
struct Convert {
    png: PathBuf,
    /// Needed for a PNG `ppu.json` does not list; a listed one has its entry's.
    kind: Option<SourceKind>,
    bpp: Option<u8>,
    cell: Option<u8>,
    /// The other converter options (`--cell-height`, `--dither`, ...), as `ppu.json` options.
    more: Vec<(&'static str, serde_json::Value)>,
    /// The new `.gpl`'s name; default `<stem>_pal`.
    new_palette: Option<String>,
}

/// The `ConvertOptions` fields (and their flags) each kind's importer reads; any other flag given
/// for the kind is refused. Read off `convert_source_with_priority`: bg takes the tile size, sheet
/// is fixed 8x8, obj has cells instead of a bit depth, and m7 reads none (its `extbg` needs a
/// priority mask, which `art convert` has no way to take).
fn honoured(kind: SourceKind) -> &'static [&'static str] {
    const REMAP: [&str; 3] = ["dither", "dither_strength", "alpha_threshold"];
    match kind {
        SourceKind::Bg => &["tile_size", "max_palettes", REMAP[0], REMAP[1], REMAP[2]],
        SourceKind::Sheet => &["max_palettes", REMAP[0], REMAP[1], REMAP[2]],
        SourceKind::Obj => &["cell_height", REMAP[0], REMAP[1], REMAP[2]],
        _ => &[],
    }
}

/// `--cell-height`, `--tile-size`, `--extbg`, `--dither`, `--dither-strength`,
/// `--alpha-threshold` and `--max-palettes` of `art convert`, each checked against its range, as
/// the `ppu.json` option it sets. Which kinds read them is checked later, once the kind is known.
fn take_convert_flags(args: &mut Vec<String>) -> Result<Vec<(&'static str, serde_json::Value)>> {
    let mut more = vec![];
    let mut num =
        |flag: &str, key: &'static str, ok: &dyn Fn(u8) -> bool, want: &str| -> Result<()> {
            if let Some(v) = take_option(args, flag)? {
                let n: u8 = v
                    .parse()
                    .ok()
                    .filter(|n| ok(*n))
                    .with_context(|| format!("{flag} must be {want} (got {v:?})"))?;
                more.push((key, n.into()));
            }
            Ok(())
        };
    num(
        "--cell-height",
        "cell_height",
        &|n| n % 8 == 0 && (8..=128).contains(&n),
        "a multiple of 8 from 8 to 128",
    )?;
    num(
        "--tile-size",
        "tile_size",
        &|n| n == 8 || n == 16,
        "8 or 16",
    )?;
    num(
        "--dither-strength",
        "dither_strength",
        &|n| n <= 100,
        "0 to 100",
    )?;
    num(
        "--alpha-threshold",
        "alpha_threshold",
        &|_| true,
        "0 to 255",
    )?;
    num(
        "--max-palettes",
        "max_palettes",
        &|n| (1..=8).contains(&n),
        "1 to 8",
    )?;
    if let Some(d) = take_option(args, "--dither")? {
        ensure!(
            ["none", "bayer", "diffusion"].contains(&d.as_str()),
            "--dither must be none, bayer or diffusion (got {d:?})"
        );
        more.push(("dither", d.into()));
    }
    ensure!(
        !take_flag(args, "--extbg"),
        "--extbg needs a Mode 7 priority mask, which ppu art convert does not take"
    );
    Ok(more)
}

/// What `ppu art convert` made.
pub(crate) struct Converted {
    pub colours: usize,
    /// What the converter did that is not a refusal (colours merged, tiles remapped).
    pub notes: Vec<String>,
    tiles: ppu_core::Budget,
    palettes: ppu_core::Budget,
    files: Vec<PathBuf>,
    pub backup: PathBuf,
}

/// The converter's notes that are not refusals, as the site shows them; `rows` is the palette
/// row cap the run used.
fn notes(report: &SourceReport, rows: impl std::fmt::Display) -> Vec<String> {
    match report {
        SourceReport::Tile { report }
        | SourceReport::Sheet { report }
        | SourceReport::Obj { report } => report
            .overflows
            .iter()
            .filter_map(|o| match o {
                Overflow::Colors { unique, budget } => Some(format!(
                    "{unique} distinct colours, but {budget} fit: merged down"
                )),
                Overflow::Palettes {
                    needed,
                    remapped_tiles,
                } => Some(format!(
                    "needed {needed} palette rows, but {rows} fit: {remapped_tiles} tiles use their closest row"
                )),
                Overflow::TileSize16 => Some("imported as 8x8 tiles".into()),
                _ => None,
            })
            .collect(),
        _ => vec![],
    }
}

/// What an import left out (tiles past the budget, pixels past the map edge), if anything.
fn dropped(report: &SourceReport) -> Option<String> {
    match report {
        SourceReport::Tile { report }
        | SourceReport::Sheet { report }
        | SourceReport::Obj { report } => report.overflows.iter().find_map(|o| match o {
            Overflow::Tiles { unique, kept } => {
                Some(format!("{unique} distinct tiles, but {kept} fit"))
            }
            Overflow::Cropped { max_px } => {
                Some(format!("the image is larger than {max_px} px on a side"))
            }
            _ => None,
        }),
        SourceReport::Mode7 { report } if report.overflow_tiles > 0 => Some(format!(
            "{} distinct tiles, but {} fit",
            report.unique_tiles, report.tile_capacity
        )),
        _ => None,
    }
}

/// `ppu art convert`: the PNG through ppu-core's converter (`convert_png_source`, the one the site runs), written back as
/// the indexed PNG `render_png` makes of the payload, with its palette as a new `.gpl` the
/// source's `options.palette` names. A listed PNG converts with its entry's kind and options (a
/// `--bpp` or `--cell` overrides the entry's option, and `--kind`, if given, must match the
/// entry); an unlisted one needs `--kind` and is listed. Everything is planned in memory first and the written PNG is proved to import back to
/// the same payload, so a refusal writes nothing. Then the original is copied to
/// `.ppu-backup/convert-N/`, the `.gpl` and the PNG land, and `ppu.json` last: a failure leaves
/// files nothing names (and the original in the backup), never a source naming a missing file.
fn art_convert(c: Convert) -> Result<Converted> {
    let Convert {
        png,
        kind,
        bpp,
        cell,
        more,
        new_palette,
    } = c;
    ensure!(
        png.extension().and_then(|e| e.to_str()) == Some("png"),
        "{} is not a .png path",
        png.display()
    );
    let stem = png
        .file_stem()
        .and_then(|n| n.to_str())
        .with_context(|| format!("{} has no usable name", png.display()))?;
    let bytes = std::fs::read(&png).with_context(|| format!("cannot read {}", png.display()))?;
    let dir = project_dir(&png)?;
    let png_abs = std::path::absolute(&png)?;
    let rel = rel_path(&dir, &png_abs)?;
    let mut manifest = read_manifest(&dir)?;
    let listed = listed_source(&dir, &manifest, &png)?;
    let listed_name = listed.map(|s| s.name.clone());
    let old_palette = listed.and_then(|s| s.options["palette"].as_str().map(str::to_string));
    let (kind, mut options) = match listed {
        Some(s) => {
            ensure!(
                s.priority_file.is_none(),
                "source {:?} has a priority_file: a Mode 7 EXTBG mask does not go with indexed SNES art, so ppu art convert does not take it",
                s.name
            );
            let entry = entry_of(&dir, &manifest, s)?;
            if let Some(k) = kind {
                ensure!(
                    k == entry.kind,
                    "{} is listed as a {} source, not {}: --kind must match (edit {MANIFEST} to change it)",
                    png.display(),
                    entry.kind.name(),
                    k.name()
                );
            }
            let mut options = s.options.clone();
            if !options.is_object() {
                options = serde_json::json!({});
            }
            // The flags override the entry's options; art_options checks them for the kind.
            let (_, given) = art_options(entry.kind, bpp, cell)?;
            for (flag, key) in [(bpp, "bit_depth"), (cell, "cell_size")] {
                if flag.is_some() {
                    options[key] = given[key].clone();
                }
            }
            (entry.kind, options)
        }
        None => {
            let kind = kind.with_context(|| {
                format!(
                    "{} is not listed in {MANIFEST}: say what it is with --kind bg|sheet|obj|m7",
                    png.display()
                )
            })?;
            safe_name("source", stem)?;
            ensure!(
                manifest.sources.iter().all(|s| s.name != stem),
                "{MANIFEST} already has a source named {stem:?}: pick another file name"
            );
            (kind, art_options(kind, bpp, cell)?.1)
        }
    };
    for (key, v) in more {
        ensure!(
            honoured(kind).contains(&key),
            "--{} does nothing for a {} source: remove it",
            key.replace('_', "-"),
            kind.name()
        );
        ensure!(
            !(key == "tile_size" && v == 16),
            "16x16 tile import is not implemented: the converter imports 8x8"
        );
        options[key] = v;
    }
    let opts: ConvertOptions = serde_json::from_value(options.clone())
        .with_context(|| format!("invalid options for {}", png.display()))?;
    let size = match decode_png(&bytes).map_err(|e| anyhow!("{}: {e}", png.display()))? {
        DecodedPng::Indexed(ip) => {
            ensure!(
                !is_snes_art(kind, &opts, &ip),
                "{} is already SNES art: ppu art check is the command for it",
                png.display()
            );
            (ip.image.width, ip.image.height)
        }
        DecodedPng::Rgba { width, height, .. } => (width, height),
    };
    let pal_name = new_palette.unwrap_or_else(|| format!("{stem}_pal"));
    safe_name("palette", &pal_name)?;
    ensure!(
        pal_name != stem,
        "the new palette's name must differ from the art's: both are {stem:?}"
    );
    let gpl_path = dir.join("palettes").join(format!("{pal_name}.gpl"));
    ensure!(
        std::fs::symlink_metadata(&gpl_path).is_err(),
        "{} already exists: pick another --new-palette",
        gpl_path.display()
    );
    ensure!(
        manifest.sources.iter().all(|s| s.name != pal_name),
        "{MANIFEST} already has a source named {pal_name:?}: pick another --new-palette"
    );
    let refuse = |e: String| anyhow::Error::new(Refused(e));
    let (payload, meta) = convert_png_source(kind, &opts, &bytes, None).map_err(refuse)?;
    // The site applies an import that dropped art and shows it as a warning; a file rewrite that
    // loses art is refused instead (the original is still on disk, nothing is written).
    if let Some(lost) = dropped(&meta.report) {
        return Err(refuse(format!(
            "the importer would drop part of the art: {lost}"
        )));
    }
    let pal = folder::own_palette(&payload).map_err(refuse)?;
    let rendered = render_png(&payload, &meta, Some(&pal)).map_err(refuse)?;
    let DecodedPng::Indexed(ip) = decode_png(&rendered).map_err(|e| anyhow!(e))? else {
        bail!("render made a PNG that is not indexed")
    };
    // Any padding or cropping (an off-grid size, past the map edge) changes the size: the artist's
    // picture would not be the one written back.
    if (ip.image.width, ip.image.height) != size {
        return Err(refuse(format!(
            "the importer would change the picture's size: {}x{} in, {}x{} out (it crops past the map edge)",
            size.0, size.1, ip.image.width, ip.image.height
        )));
    }
    let (back, _) = convert_png_source(kind, &opts, &rendered, None)
        .map_err(|e| refuse(format!("the converted PNG would not import back: {e}")))?;
    if back.encode() != payload.encode() {
        return Err(refuse(
            "the converted PNG would not import back to the same art".into(),
        ));
    }
    let checked = check_indexed(kind, &opts, &ip, Some(&pal)).map_err(|e| anyhow!(e))?;
    if let Some(f) = checked.findings.first() {
        return Err(refuse(format!(
            "the converted PNG would fail ppu art check: {}",
            f.message
        )));
    }
    let mut notes = notes(&meta.report, checked.budgets.palettes.max);
    if let Some(old) = old_palette.filter(|old| *old != pal_name) {
        notes.push(format!(
            "{stem} now uses the palette {pal_name}, not {old}: code that loads it with palette = \"{old}\" must name \"{pal_name}\"."
        ));
    }
    let colours = match &meta.report {
        SourceReport::Tile { report }
        | SourceReport::Sheet { report }
        | SourceReport::Obj { report } => report.colors_used,
        SourceReport::Mode7 { report } => usize::from(report.colors),
        _ => bail!("the importer made a source that is not art"),
    };
    let text = gpl::write(&pal, &pal_name);
    let listed_before = manifest.sources.len();
    list_palette(&dir, &mut manifest.sources, &gpl_path, &text)?;
    // Edited as plain JSON, not through the struct: every field stays (keys come back sorted).
    let mut raw: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join(MANIFEST)).context("cannot read ppu.json")?,
    )
    .context("invalid ppu.json")?;
    let sources = raw["sources"]
        .as_array_mut()
        .expect("read_manifest parsed sources");
    match listed_name {
        Some(name) => {
            let mine = sources
                .iter_mut()
                .find(|r| r["name"] == name.as_str())
                .expect("read_manifest parsed this source");
            // `options` is the entry's own (unknown keys included) with the flags laid over it.
            mine["options"] = options;
            mine["options"]["palette"] = pal_name.clone().into();
        }
        None => {
            let mut options = options;
            options["palette"] = pal_name.clone().into();
            manifest.sources.push(ManifestSource {
                name: stem.into(),
                kind: kind.name().into(),
                options,
                meta: serde_json::Value::Null,
                payload: None,
                file: Some(rel.clone()),
                priority_file: None,
                extra: Default::default(),
            });
        }
    }
    for s in &manifest.sources[listed_before..] {
        sources.push(serde_json::to_value(s)?);
    }
    // The first backup folder nobody has used: an earlier backup is never overwritten.
    let backup = (1..)
        .map(|n| format!("{BACKUP_DIR}/convert-{n}"))
        .find(|b| std::fs::symlink_metadata(dir.join(b)).is_err())
        .expect("an unused backup folder");
    folder::back_up(&dir, &backup, &rel)?;
    std::fs::create_dir_all(gpl_path.parent().expect("a .gpl has a directory"))?;
    replace_file(&gpl_path, text.as_bytes())?;
    replace_file(&png_abs, &rendered)?;
    write_manifest(&dir, &raw)?;
    Ok(Converted {
        colours,
        notes,
        tiles: checked.budgets.tiles,
        palettes: checked.budgets.palettes,
        files: vec![gpl_path, png_abs, dir.join(MANIFEST)],
        backup: dir.join(backup).join(rel),
    })
}

/// `ppu art convert`'s arguments after `convert` (its flags and the PNG), checked and run: the
/// one path the CLI and `ppu connect`'s Convert button share.
pub(crate) fn convert_with(mut args: Vec<String>) -> Result<Converted> {
    let args = &mut args;
    let (kind, bpp, cell) = take_art_flags(args)?;
    let more = take_convert_flags(args)?;
    let new_palette = take_option(args, "--new-palette")?;
    ensure!(args.len() == 1, usage());
    art_convert(Convert {
        png: PathBuf::from(&args[0]),
        kind,
        bpp,
        cell,
        more,
        new_palette,
    })
}

/// An editor `ppu art install` knows: its files (name, content) and where its user folder puts them.
struct Editor {
    name: &'static str,
    /// `None`: named, not available yet.
    files: Option<&'static [(&'static str, &'static str)]>,
    /// `Some((archive, inside))`: the editor loads a `.zip`, so the files go into `archive` under the
    /// folder `inside` instead of loose into the destination.
    archive: Option<(&'static str, &'static str)>,
    /// The folder the files go in, given `--dir`.
    destination: fn(Option<PathBuf>) -> Result<PathBuf>,
    /// What to tell the author once the files are in.
    after: &'static str,
}

/// The longest side `ppu art new` makes (a Mode 7 track is 1024 px).
const MAX_SIDE: u32 = 1024;

const EDITORS: &[Editor] = &[
    Editor {
        name: "aseprite",
        files: Some(&[
            (
                "package.json",
                include_str!("../editors/aseprite/package.json"),
            ),
            (
                "snes-art.lua",
                include_str!("../editors/aseprite/snes-art.lua"),
            ),
        ]),
        archive: None,
        destination: aseprite_extension_folder,
        after: "Restart Aseprite. The first time it opens a project PNG, tick \"Give full trust to this script\" in the Security dialog and press Allow.",
    },
    Editor {
        name: "pixelorama",
        files: Some(&[
            (
                "extension.json",
                include_str!("../editors/pixelorama/SnesArt/extension.json"),
            ),
            (
                "main.gd",
                include_str!("../editors/pixelorama/SnesArt/main.gd"),
            ),
            (
                "main.tscn",
                include_str!("../editors/pixelorama/SnesArt/main.tscn"),
            ),
            (
                "overlay.gd",
                include_str!("../editors/pixelorama/SnesArt/overlay.gd"),
            ),
            (
                "png.gd",
                include_str!("../editors/pixelorama/SnesArt/png.gd"),
            ),
        ]),
        archive: Some(("SnesArt.zip", "src/Extensions/SnesArt")),
        destination: pixelorama_extension_folder,
        after: "Restart Pixelorama, then enable SNES art once in Preferences > Extensions.",
    },
];

/// Aseprite's `extensions/ppu-art` folder inside its user folder: `dir` if given, else
/// `ASEPRITE_USER_FOLDER`, else the OS default.
fn aseprite_extension_folder(dir: Option<PathBuf>) -> Result<PathBuf> {
    let var = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let user = match dir.or_else(|| var("ASEPRITE_USER_FOLDER")) {
        Some(d) => d,
        None if cfg!(windows) => var("APPDATA")
            .context("APPDATA is not set: pass --dir")?
            .join("Aseprite"),
        None if cfg!(target_os = "macos") => var("HOME")
            .context("HOME is not set: pass --dir")?
            .join("Library/Application Support/Aseprite"),
        None => var("XDG_CONFIG_HOME")
            .or_else(|| var("HOME").map(|h| h.join(".config")))
            .context("neither XDG_CONFIG_HOME nor HOME is set: pass --dir")?
            .join("aseprite"),
    };
    Ok(user.join("extensions").join("ppu-art"))
}

/// Pixelorama's `extensions` folder inside its user data folder: `dir` if given, else the OS default
/// (Godot's `user://` with Pixelorama's custom folder name).
fn pixelorama_extension_folder(dir: Option<PathBuf>) -> Result<PathBuf> {
    let var = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let user = match dir {
        Some(d) => d,
        None if cfg!(windows) => var("APPDATA")
            .context("APPDATA is not set: pass --dir")?
            .join("pixelorama"),
        None if cfg!(target_os = "macos") => var("HOME")
            .context("HOME is not set: pass --dir")?
            .join("Library/Application Support/pixelorama"),
        None => var("XDG_DATA_HOME")
            .or_else(|| var("HOME").map(|h| h.join(".local/share")))
            .context("neither XDG_DATA_HOME nor HOME is set: pass --dir")?
            .join("pixelorama"),
    };
    Ok(user.join("extensions"))
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for b in data {
        c ^= *b as u32;
        for _ in 0..8 {
            c = (c >> 1) ^ (0xEDB8_8320 & (c & 1).wrapping_neg());
        }
    }
    !c
}

/// A zip of `files` (name, bytes), stored uncompressed: a few KB of script, and no zip crate in the workspace.
fn stored_zip(files: &[(String, &[u8])]) -> Vec<u8> {
    let (mut out, mut central) = (vec![], vec![]);
    for (name, data) in files {
        let at = out.len() as u32;
        let (crc, len, n) = (crc32(data), data.len() as u32, name.len() as u16);
        // version, flags, method 0 (stored), DOS time 00:00, DOS date 1980-01-01
        let common = [20u16.to_le_bytes(), [0, 0], [0, 0], [0, 0], [0x21, 0]].concat();
        let sizes = [crc.to_le_bytes(), len.to_le_bytes(), len.to_le_bytes()].concat();
        out.extend(
            [
                b"PK\x03\x04".as_slice(),
                &common,
                &sizes,
                &n.to_le_bytes(),
                &[0, 0],
            ]
            .concat(),
        );
        out.extend(name.as_bytes());
        out.extend(*data);
        central.extend(
            [
                b"PK\x01\x02".as_slice(),
                &20u16.to_le_bytes(),
                &common,
                &sizes,
                &n.to_le_bytes(),
                &[0; 12],
                &at.to_le_bytes(),
            ]
            .concat(),
        );
        central.extend(name.as_bytes());
    }
    let (count, start) = (files.len() as u16, out.len() as u32);
    out.extend(&central);
    out.extend(
        [
            b"PK\x05\x06".as_slice(),
            &[0; 4],
            &count.to_le_bytes(),
            &count.to_le_bytes(),
            &(central.len() as u32).to_le_bytes(),
            &start.to_le_bytes(),
            &[0, 0],
        ]
        .concat(),
    );
    out
}

/// `ppu art install <editor>`: the editor plugin's files into its user folder, over an older
/// install. Returns the folder and the files written.
fn art_install(
    editor: &str,
    dir: Option<PathBuf>,
) -> Result<(PathBuf, Vec<PathBuf>, &'static str)> {
    let known = EDITORS
        .iter()
        .map(|e| e.name)
        .collect::<Vec<_>>()
        .join(", ");
    let e = EDITORS
        .iter()
        .find(|e| e.name == editor)
        .with_context(|| format!("unknown editor {editor:?}: known editors are {known}"))?;
    let files = e
        .files
        .with_context(|| format!("ppu art install {} is not available yet", e.name))?;
    let folder = (e.destination)(dir)?;
    std::fs::create_dir_all(&folder)
        .with_context(|| format!("cannot create {}", folder.display()))?;
    let mut written = vec![];
    if let Some((archive, inside)) = e.archive {
        let entries: Vec<(String, &[u8])> = files
            .iter()
            .map(|(n, c)| (format!("{inside}/{n}"), c.as_bytes()))
            .collect();
        let path = folder.join(archive);
        replace_file(&path, &stored_zip(&entries))?;
        return Ok((folder, vec![path], e.after));
    }
    for (name, content) in files {
        let path = folder.join(name);
        replace_file(&path, content.as_bytes())?;
        written.push(path);
    }
    Ok((folder, written, e.after))
}

fn take_flag(args: &mut Vec<String>, flag: &str) -> bool {
    args.iter()
        .position(|a| a == flag)
        .map(|i| args.remove(i))
        .is_some()
}

fn take_option(args: &mut Vec<String>, flag: &str) -> Result<Option<String>> {
    let Some(i) = args.iter().position(|a| a == flag) else {
        return Ok(None);
    };
    ensure!(
        i + 1 < args.len(),
        bad_usage(format!("{flag} requires a value"))
    );
    args.remove(i);
    Ok(Some(args.remove(i)))
}

const USAGE: &str = "usage: ppu art check <png> [--json] [--stdin --width W --height H]\n       ppu art palette sync <gpl>\n       ppu art palette write <gpl>   (stdin: one 'r g b' line per colour line)\n       ppu art palette set <png> <gpl> --width W --height H   (stdin: index bytes)\n       ppu art new <png> --kind bg|sheet|obj|m7 --size WxH [--bpp 2|4|8] [--cell N] (--palette <gpl> | --new-palette <name>)\n       ppu art convert <png> [--kind bg|sheet|obj|m7] [--bpp 2|4|8] [--cell N] [--cell-height N] [--tile-size 8] [--dither none|bayer|diffusion] [--dither-strength 0-100] [--alpha-threshold 0-255] [--max-palettes 1-8] [--new-palette <name>] [--json]\n       ppu art install <editor> [--dir D]\n       ppu art job <dir> [--detach] [--timeout SECONDS]\n       ppu art rows <png> --width W --height H (--fix | --set ROW --rect X,Y,W,H)   (stdin: index bytes)";

fn read_stdin() -> Result<Vec<u8>> {
    let mut bytes = vec![];
    std::io::stdin()
        .lock()
        .read_to_end(&mut bytes)
        .context("cannot read stdin")?;
    Ok(bytes)
}

/// `--width` and `--height`, with the index bytes from stdin.
fn take_stdin_image(args: &mut Vec<String>) -> Result<(u32, u32, Vec<u8>)> {
    let (Some(w), Some(h)) = (
        take_option(args, "--width")?,
        take_option(args, "--height")?,
    ) else {
        return Err(bad_usage("this command needs --width and --height"));
    };
    Ok((
        number(&w, "--width")?,
        number(&h, "--height")?,
        read_stdin()?,
    ))
}

/// The three-channel lines `palette write` reads, each channel 0 to 255.
fn parse_colours(text: &str) -> Result<Vec<[u8; 3]>> {
    text.lines()
        .enumerate()
        .map(|(i, l)| {
            let ch: Vec<u8> = l
                .split_whitespace()
                .map(|c| c.parse())
                .collect::<Result<_, _>>()
                .map_err(|_| {
                    anyhow!(
                        "stdin line {}: expected 'r g b' (0 to 255), got {l:?}",
                        i + 1
                    )
                })?;
            <[u8; 3]>::try_from(ch).map_err(|_| {
                anyhow!(
                    "stdin line {}: expected 'r g b' (0 to 255), got {l:?}",
                    i + 1
                )
            })
        })
        .collect()
}

/// `--kind`, `--bpp` and `--cell`, as `art new` and `art convert` both take them.
fn take_art_flags(args: &mut Vec<String>) -> Result<(Option<SourceKind>, Option<u8>, Option<u8>)> {
    let num = |v: Option<String>, flag: &str| -> Result<Option<u8>> {
        v.map(|v| number(&v, flag)).transpose()
    };
    let kind = take_option(args, "--kind")?
        .map(|k| {
            k.parse()
                .map_err(|e: String| bad_usage(format!("--kind: {e}")))
        })
        .transpose()?;
    let bpp = num(take_option(args, "--bpp")?, "--bpp")?;
    let cell = num(take_option(args, "--cell")?, "--cell")?;
    Ok((kind, bpp, cell))
}

/// One `--json` answer: `fields` (an object) with the protocol number beside them.
fn answer(mut fields: serde_json::Value) {
    fields["protocol"] = PROTOCOL.into();
    println!("{fields}");
}

/// The index bytes as lowercase hex: what `--json` answers carry in `pixels`.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// `ppu art ...`: prints, and returns the exit code: 0 clean, 1 findings (or an import refused),
/// 2 a usage, IO or manifest error.
pub fn run(mut args: Vec<String>) -> i32 {
    if args.first().map(String::as_str) == Some("job") {
        return super::art_job::run(args.split_off(1));
    }
    let json = take_flag(&mut args, "--json");
    let result = run_inner(&mut args, json);
    match result {
        Ok(code) => code,
        Err(e) => {
            let code = if e.is::<Refused>() { 1 } else { 2 };
            if json {
                let reason = match (e.downcast_ref::<Coded>(), e.is::<Refused>()) {
                    (Some(c), _) => c.0,
                    (None, true) => "refused",
                    _ => "error",
                };
                answer(
                    serde_json::json!({"ok": false, "reason": reason, "error": format!("{e:#}")}),
                );
            } else {
                eprintln!("ppu: {e:#}");
            }
            code
        }
    }
}

/// The PNGs a palette write or sync rewrote: `rewrote <path>` lines, or `synced` in `--json`.
fn synced(paths: Vec<PathBuf>, json: bool) -> Result<i32> {
    if json {
        answer(serde_json::json!({"ok": true, "synced": paths}));
    } else {
        for p in paths {
            println!("rewrote {}", p.display());
        }
    }
    Ok(0)
}

/// Index bytes an editor takes back: raw on stdout (notes on stderr), or `pixels` (hex) and
/// `notes` in `--json`.
fn index_bytes(pixels: &[u8], notes: &[String], json: bool) -> Result<()> {
    if json {
        answer(serde_json::json!({"ok": true, "pixels": hex(pixels), "notes": notes}));
        return Ok(());
    }
    std::io::stdout()
        .write_all(pixels)
        .context("cannot write stdout")?;
    for n in notes {
        eprintln!("{n}");
    }
    Ok(())
}

fn run_inner(args: &mut Vec<String>, json: bool) -> Result<i32> {
    let word = |i: usize| args.get(i).map(String::as_str);
    if word(0) == Some("palette") && word(1) == Some("write") {
        ensure!(args.len() == 3, usage());
        let colours =
            parse_colours(&String::from_utf8(read_stdin()?).context("stdin is not text")?)?;
        return synced(palette_write(Path::new(&args[2]), &colours)?, json);
    }
    if word(0) == Some("palette") && word(1) == Some("set") {
        args.drain(..2);
        let stdin = take_stdin_image(args)?;
        ensure!(args.len() == 2, usage());
        return Ok(
            match palette_set(Path::new(&args[0]), Path::new(&args[1]), stdin)? {
                Switched::Pixels(p) => {
                    index_bytes(&p, &[], json)?;
                    0
                }
                Switched::Unmatched(lines) => {
                    if json {
                        answer(serde_json::json!({
                            "ok": false,
                            "reason": "unmatched",
                            "error": "the palette lacks colours the art uses",
                            "unmatched": lines,
                        }));
                    } else {
                        for l in lines {
                            eprintln!("{l}");
                        }
                    }
                    1
                }
            },
        );
    }
    if word(0) == Some("new") {
        args.remove(0);
        let (kind, bpp, cell) = take_art_flags(args)?;
        let kind = kind.ok_or_else(|| bad_usage("ppu art new needs --kind"))?;
        let size = take_option(args, "--size")?
            .ok_or_else(|| bad_usage("ppu art new needs --size WxH"))?;
        let size = size
            .split_once('x')
            .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
            .ok_or_else(|| bad_usage(format!("--size wants WxH, got {size:?}")))?;
        let palette = match (
            take_option(args, "--palette")?,
            take_option(args, "--new-palette")?,
        ) {
            (Some(p), None) => NewPalette::Existing(p.into()),
            (None, Some(n)) => NewPalette::Named(n),
            _ => {
                return Err(bad_usage(
                    "ppu art new needs exactly one of --palette and --new-palette",
                ))
            }
        };
        ensure!(args.len() == 1, usage());
        let art = NewArt {
            png: PathBuf::from(&args[0]),
            kind,
            size,
            bpp,
            cell,
            palette,
        };
        let files = art_new(art)?;
        if json {
            answer(serde_json::json!({"ok": true, "files": files}));
        } else {
            for p in files {
                println!("wrote {}", p.display());
            }
        }
        return Ok(0);
    }
    if word(0) == Some("convert") {
        args.remove(0);
        let done = convert_with(std::mem::take(args))?;
        if json {
            answer(serde_json::json!({
                "ok": true,
                "colours": done.colours,
                "tiles": done.tiles,
                "palettes": done.palettes,
                "files": done.files,
                "backup": done.backup,
                "notes": done.notes,
            }));
        } else {
            for p in &done.files {
                println!("wrote {}", p.display());
            }
            for n in &done.notes {
                println!("note: {n}");
            }
            println!("kept the original in {}", done.backup.display());
            println!(
                "{} colours, {}/{} tiles, {}/{} palette rows",
                done.colours,
                done.tiles.used,
                done.tiles.max,
                done.palettes.used,
                done.palettes.max
            );
        }
        return Ok(0);
    }
    if word(0) == Some("install") {
        args.remove(0);
        let dir = take_option(args, "--dir")?.map(PathBuf::from);
        ensure!(args.len() == 1, usage());
        let (folder, files, after) = art_install(&args[0], dir)?;
        if json {
            answer(
                serde_json::json!({"ok": true, "files": files, "folder": folder, "after": after}),
            );
        } else {
            for p in files {
                println!("wrote {}", p.display());
            }
            println!("Installed into {}.", folder.display());
            println!("{after}");
        }
        return Ok(0);
    }
    if word(0) == Some("rows") {
        args.remove(0);
        let fix = take_flag(args, "--fix");
        let set = take_option(args, "--set")?;
        let rect = take_option(args, "--rect")?;
        let op = match (fix, set, rect) {
            (true, None, None) => RowsOp::Fix,
            (false, Some(row), Some(rect)) => {
                let n: Vec<usize> = rect
                    .split(',')
                    .map(|v| v.parse())
                    .collect::<Result<_, _>>()
                    .map_err(|_| bad_usage(format!("--rect wants X,Y,W,H, got {rect:?}")))?;
                let [x, y, w, h] = n[..] else {
                    return Err(bad_usage(format!("--rect wants X,Y,W,H, got {rect:?}")));
                };
                RowsOp::Set {
                    row: number(&row, "--set")?,
                    rect: (x, y, w, h),
                }
            }
            _ => return Err(usage()),
        };
        let stdin = take_stdin_image(args)?;
        ensure!(args.len() == 1, usage());
        let (pixels, notes) = rows_op(Path::new(&args[0]), &op, stdin)?;
        index_bytes(&pixels, &notes, json)?;
        return Ok(0);
    }
    if word(0) == Some("palette") && word(1) == Some("sync") {
        ensure!(args.len() == 3, usage());
        return synced(palette_sync(Path::new(&args[2]))?, json);
    }
    ensure!(args.first().map(String::as_str) == Some("check"), usage());
    args.remove(0);
    let stdin = take_flag(args, "--stdin");
    let dims = (
        take_option(args, "--width")?,
        take_option(args, "--height")?,
    );
    ensure!(args.len() == 1, usage());
    let pixels = if stdin {
        let (Some(w), Some(h)) = dims else {
            return Err(bad_usage("--stdin needs --width and --height"));
        };
        let (w, h): (u32, u32) = (number(&w, "--width")?, number(&h, "--height")?);
        let bytes = read_stdin()?;
        Some((w, h, bytes))
    } else {
        ensure!(dims == (None, None), "--width and --height go with --stdin");
        None
    };
    let report = check(Path::new(&args[0]), pixels)?;
    if json {
        println!("{}", serde_json::to_string(&report)?);
    } else {
        print!("{}", human(&report));
    }
    Ok(if report.ok { 0 } else { 1 })
}
