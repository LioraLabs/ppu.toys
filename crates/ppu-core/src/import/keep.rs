//! The keep path: an indexed PNG that is already SNES art imports with its indices as they are.
//!
//! No median cut, no k-means, no reordering: code depends on index order (`cgram[i]`, `pal`,
//! cycling). This is a front half, like the RGBA ones: it reads a palette row per unit and a grid
//! of local indices off the pixels, builds an [`IndexedArt`] (or Mode 7's byte grid) and hands it
//! to the shared back halves, so dedup, tilemap, band and cell code are never forked.
//!
//! An indexed PNG is SNES art when its `tRNS` gives alpha 0 to entry 0 of every palette row (a
//! row is 4, 16 or 256 entries by the kind's bit depth; Mode 7 EXTBG also makes entry 128
//! transparent) and to no other entry. That is what [`crate::render::render_png`] writes. Art
//! that mixes rows inside a unit or overflows a budget is refused, never re-quantized; any other
//! PNG quantizes as [`crate::convert_source_with_priority`] always has.

use super::check::{ArtCheck, Budget, Budgets, Finding, Rule, UnitSize};
use super::obj::OBJ_MAX_TILES;
use super::tiles::{IndexTile, TileSet};
use super::{crop_to_map, IndexedArt, Overflow, BG_MAX_TILES};
use crate::render::{decode_png, is_transparent_entry, rgba_of, DecodedPng, IndexedPng};
use crate::source::{
    convert_source_with_priority, row_size_for, ConvertOptions, PaletteSource, SourceKind,
    SourceMeta, SourcePayload, SourceReport,
};

/// Colours per palette row for an image kind, or why it has none.
fn row_size(kind: SourceKind, opts: &ConvertOptions) -> Result<usize, String> {
    let depth = opts.bit_depth.unwrap_or(4);
    if matches!(kind, SourceKind::Bg | SourceKind::Sheet) && !matches!(depth, 2 | 4 | 8) {
        return Err(format!(
            "{} bit_depth must be 2, 4 or 8 (got {depth})",
            kind.name()
        ));
    }
    row_size_for(kind, depth)
        .map(usize::from)
        .ok_or_else(|| format!("{} sources are not indexed images", kind.name()))
}

/// Is `k` an entry the art's rows make transparent?
fn transparent(kind: SourceKind, opts: &ConvertOptions, row_size: usize, k: usize) -> bool {
    is_transparent_entry(
        row_size,
        kind == SourceKind::M7 && opts.extbg == Some(true),
        k,
    )
}

/// Does the PNG's `tRNS` mark it SNES art for this kind (see the module doc)?
pub fn is_snes_art(kind: SourceKind, opts: &ConvertOptions, png: &IndexedPng) -> bool {
    let Ok(rs) = row_size(kind, opts) else {
        return false;
    };
    png.alpha
        .iter()
        .enumerate()
        .all(|(k, &a)| (a == 0) == transparent(kind, opts, rs, k))
}

/// Convert an indexed image that is SNES art, keeping its indices (see the module doc). Refuses
/// one that is not SNES art, mixes palette rows inside a tile or cell (naming its x,y and the
/// rows), or overflows a budget (naming it): the first [`check_indexed`](super::check::check_indexed)
/// finding.
pub fn convert_indexed_source(
    kind: SourceKind,
    opts: &ConvertOptions,
    png: &IndexedPng,
) -> Result<(SourcePayload, SourceMeta), String> {
    let a = analyze(kind, opts, png, None)?;
    match (a.check.findings.first(), a.art) {
        (Some(f), _) => Err(f.message.clone()),
        (None, art) => Ok(art.expect("analyze builds the art when it finds nothing")),
    }
}

pub(super) struct Analysis {
    pub check: ArtCheck,
    /// The import, when there are no findings.
    art: Option<(SourcePayload, SourceMeta)>,
}

/// The one pass import and check share: every finding, in the order import meets them, and the
/// art when there are none.
pub(super) fn analyze(
    kind: SourceKind,
    opts: &ConvertOptions,
    png: &IndexedPng,
    gpl: Option<&PaletteSource>,
) -> Result<Analysis, String> {
    let rs = row_size(kind, opts)?;
    let bpp = opts.bit_depth.unwrap_or(4);
    let (unit, tile_max, pal_max) = match kind {
        SourceKind::M7 => ((1, 1), super::super::M7_MAX_TILES, 1),
        SourceKind::Bg => ((1, 1), BG_MAX_TILES - 1, max_rows(bpp, opts)),
        SourceKind::Sheet => ((1, 1), BG_MAX_TILES, max_rows(bpp, opts)),
        _ => {
            let cell_size = opts.cell_size.unwrap_or(8);
            let cell_height = opts.cell_height.unwrap_or(cell_size);
            for (name, n) in [("cell_size", cell_size), ("cell_height", cell_height)] {
                if !n.is_multiple_of(8) || !(8..=128).contains(&n) {
                    return Err(format!(
                        "obj {name} must be a multiple of 8 from 8 to 128 (got {n})"
                    ));
                }
            }
            let unit = ((cell_size / 8) as usize, (cell_height / 8) as usize);
            let cap = if unit == (1, 1) {
                OBJ_MAX_TILES - 1
            } else {
                OBJ_MAX_TILES
            };
            (unit, cap, 8)
        }
    };
    // Blocks hold whole cells: the cap is what the name table's bands fit, not a flat 512.
    let tile_max = if unit == (1, 1) {
        tile_max
    } else {
        super::obj::block_capacity(unit.0, unit.1) * unit.0 * unit.1
    };
    let px = if kind == SourceKind::M7 {
        (8, 8)
    } else {
        (unit.0 * 8, unit.1 * 8)
    };
    let mut check = ArtCheck {
        unit: UnitSize {
            width: px.0,
            height: px.1,
        },
        budgets: Budgets {
            tiles: Budget {
                used: 0,
                max: tile_max,
            },
            palettes: Budget {
                used: 0,
                max: pal_max,
            },
        },
        findings: vec![],
    };
    let f = &mut check.findings;
    if !is_snes_art(kind, opts, png) {
        f.push(Finding::new(Rule::NotSnesArt, format!(
            "not SNES art: tRNS must give alpha 0 to entry 0 of every row of {rs} colours{} and to no other entry",
            if kind == SourceKind::M7 && opts.extbg == Some(true) { " (and entry 128)" } else { "" }
        )));
    }
    let img = &png.image;
    let (w, h) = (img.width as usize, img.height as usize);
    if w == 0 || h == 0 || img.pixels.len() != w * h {
        return Err("imageData dimensions/buffer mismatch".into());
    }
    if let Some(&p) = img
        .pixels
        .iter()
        .find(|&&p| p as usize >= img.palette.len())
    {
        f.push(Finding::new(
            Rule::Plte,
            format!(
                "a pixel is index {p}, but the PLTE holds {} entries",
                img.palette.len()
            ),
        ));
        return Ok(Analysis { check, art: None });
    }
    // Where the first tile past the char budget is, when the back half only reports the overflow.
    let mut over = None;
    let (payload, meta, pal_used) = match kind {
        SourceKind::M7 => {
            let (p, m) = keep_m7(opts, png, f);
            (p, m, 1)
        }
        SourceKind::Bg => {
            let rows = max_rows(bpp, opts);
            let mut overflows = vec![];
            if w > 512 || h > 512 {
                // Where the first tile past the 64x64-tile map is.
                let (x, y) = if w > 512 { (64, 0) } else { (0, 64) };
                f.push(
                    Finding::new(
                        Rule::MapBudget,
                        format!(
                            "the bg is {w}x{h} px; a tile map holds 512x512 (64x64 tiles) at most"
                        ),
                    )
                    .at(x, y),
                );
            }
            let (cw, ch) = crop_to_map(img.width, img.height, &mut overflows);
            let mut art = read_units(png, (cw, ch), rs, (1, 1), rows, f);
            art.overflows = overflows;
            over = first_past(&art, BG_MAX_TILES, true);
            let used = art.palettes.len();
            let (s, m) = super::bg_from_indexed(art, bpp, img.width, img.height);
            (SourcePayload::Bg(s), m, used)
        }
        SourceKind::Sheet => {
            let art = read_units(png, (w, h), rs, (1, 1), max_rows(bpp, opts), f);
            over = (art.grids.len() > BG_MAX_TILES)
                .then(|| (BG_MAX_TILES % art.cols, BG_MAX_TILES / art.cols));
            let used = art.palettes.len();
            let (s, m) = super::sheet_from_indexed(art, bpp, img.width, img.height);
            (SourcePayload::Sheet(s), m, used)
        }
        _ => {
            let art = read_units(png, (w, h), rs, unit, 8, f);
            if unit == (1, 1) {
                over = first_past(&art, OBJ_MAX_TILES, true);
            }
            let used = art.palettes.len();
            let cell = (unit.0 as u8 * 8, unit.1 as u8 * 8);
            let big_cols = art.cols.div_ceil(unit.0);
            let (s, m) = super::obj::obj_from_indexed(art, img.width, img.height, cell);
            if unit != (1, 1) {
                // Blocks overflow by whole cells: the first one past `kept`.
                over = tile_overflow(&m).map(|(_, kept)| {
                    let k = kept / (unit.0 * unit.1);
                    (k % big_cols, k / big_cols)
                });
            }
            (SourcePayload::Obj(s), m, used)
        }
    };
    check.budgets.palettes.used = pal_used;
    if let Some((used, max, msg)) = tile_budget(&meta) {
        check.budgets.tiles = Budget { used, max };
        let at = over.or_else(|| m7_first_past(kind, png, max));
        let mut fd = Finding::new(Rule::TileBudget, msg);
        if let Some((x, y)) = at {
            fd = fd.at(x, y);
        }
        check.findings.push(fd);
    } else if let Some(used) = tiles_used(&meta) {
        check.budgets.tiles.used = used;
    }
    if let Some(gpl) = gpl {
        check_plte(
            rs,
            gpl,
            &img.palette,
            opts.extbg == Some(true) && kind == SourceKind::M7,
            &mut check.findings,
        );
    }
    let art = check.findings.is_empty().then_some((payload, meta));
    Ok(Analysis { check, art })
}

/// Convert a PNG file: an indexed one that is SNES art keeps its indices; anything else (a
/// full-colour PNG, or an indexed one without the `tRNS` signature) quantizes exactly as
/// [`convert_source_with_priority`] does. `priority_png` is Mode 7 EXTBG's black/white mask; SNES
/// art carries priority as bit 7 of its indices, so a mask beside it is refused.
pub fn convert_png_source(
    kind: SourceKind,
    opts: &ConvertOptions,
    png: &[u8],
    priority_png: Option<&[u8]>,
) -> Result<(SourcePayload, SourceMeta), String> {
    let (rgba, w, h) = match decode_png(png)? {
        DecodedPng::Indexed(ip) if is_snes_art(kind, opts, &ip) => {
            if priority_png.is_some() {
                return Err("a priority mask does not go with indexed SNES art: Mode 7 EXTBG priority is bit 7 of the index".into());
            }
            return convert_indexed_source(kind, opts, &ip);
        }
        DecodedPng::Indexed(_) => rgba_of(png)?,
        DecodedPng::Rgba {
            rgba,
            width,
            height,
        } => (rgba, width, height),
    };
    let priority = priority_png
        .map(|p| match rgba_of(p)? {
            (r, pw, ph) if (pw, ph) == (w, h) => Ok(r),
            (_, pw, ph) => Err(format!(
                "the priority mask is {pw}x{ph}, its image {w}x{h}: they must match"
            )),
        })
        .transpose()?;
    convert_source_with_priority(kind, opts, &rgba, w, h, priority.as_deref())
}

/// The sub-palette rows a bg or sheet may use (`max_palettes`, and one block at 8bpp).
fn max_rows(bpp: u8, opts: &ConvertOptions) -> usize {
    let cap = if bpp == 8 { 1 } else { 8 };
    cap.min(opts.max_palettes.unwrap_or(8).clamp(1, 8) as usize)
}

/// Split the first `(w, h)` pixels into 8x8 grids of local indices and give every unit (`unit`
/// tiles across and down: one tile, or an obj cell) the palette row its non-zero pixels name.
/// Entry 0 of any row is row-neutral. A row is as long as its highest used local index. A unit
/// that mixes rows, or is on a row past `max_rows`, is a finding (once per unit) and keeps no row.
fn read_units(
    png: &IndexedPng,
    (w, h): (usize, usize),
    rs: usize,
    (nw, nh): (usize, usize),
    max_rows: usize,
    findings: &mut Vec<Finding>,
) -> IndexedArt {
    let img = &png.image;
    let (cols, trows) = (w.div_ceil(8), h.div_ceil(8));
    let ucols = cols.div_ceil(nw);
    let what = if (nw, nh) == (1, 1) { "tile" } else { "cell" };
    let mut grids: Vec<IndexTile> = vec![[0; 64]; cols * trows];
    let mut unit_row: Vec<Option<usize>> = vec![None; ucols * trows.div_ceil(nh)];
    let mut refused = vec![false; unit_row.len()];
    let mut high = vec![0usize; max_rows];
    for ty in 0..trows {
        for tx in 0..cols {
            let (unit, ux, uy) = ((ty / nh) * ucols + tx / nw, tx / nw, ty / nh);
            for (i, slot) in grids[ty * cols + tx].iter_mut().enumerate() {
                let (x, y) = (tx * 8 + i % 8, ty * 8 + i / 8);
                if x >= w || y >= h {
                    continue;
                }
                let p = img.pixels[y * img.width as usize + x] as usize;
                let (row, local) = (p / rs, p % rs);
                *slot = local as u8;
                if local == 0 || refused[unit] {
                    continue;
                }
                let finding = match unit_row[unit] {
                    None if row >= max_rows => Finding::new(Rule::PaletteBudget, format!(
                        "{what} {ux},{uy} uses palette row {row}, but the budget is {max_rows} palette rows (a row is {rs} colours)"
                    )).rows(vec![row]),
                    None => {
                        unit_row[unit] = Some(row);
                        high[row] = high[row].max(local);
                        continue;
                    }
                    Some(r) if r != row => Finding::new(Rule::MixedRows, format!("{what} {ux},{uy} mixes rows {r} and {row}"))
                        .rows(vec![r, row]),
                    Some(_) => {
                        high[row] = high[row].max(local);
                        continue;
                    }
                };
                refused[unit] = true;
                findings.push(finding.at(ux, uy));
            }
        }
    }
    let used = high.iter().rposition(|&hi| hi > 0).map_or(0, |r| r + 1);
    let palettes = (0..used)
        .map(|r| (1..=high[r]).map(|l| img.palette[r * rs + l]).collect())
        .collect();
    IndexedArt {
        palettes,
        grids,
        rows: unit_row.iter().map(|r| r.unwrap_or(0) as u8).collect(),
        cols,
        tile_rows: trows,
        overflows: vec![],
    }
}

/// The first tile (x,y) whose flip-aware dedup is past `max` chars, blank tile 0 counted.
fn first_past(art: &IndexedArt, max: usize, flips: bool) -> Option<(usize, usize)> {
    let mut set = TileSet::new(flips);
    set.insert([0; 64]);
    let i = art.grids.iter().position(|g| {
        set.insert(*g);
        set.len() > max
    })?;
    Some((i % art.cols, i / art.cols))
}

/// Mode 7's first tile past the 256 it names (it dedups on exact bytes, in scan order).
fn m7_first_past(kind: SourceKind, png: &IndexedPng, max: usize) -> Option<(usize, usize)> {
    if kind != SourceKind::M7 {
        return None;
    }
    let img = &png.image;
    let (w, h) = (img.width as usize, img.height as usize);
    let (tw, th) = (
        w.div_ceil(8).min(crate::M7_MAP_DIM),
        h.div_ceil(8).min(crate::M7_MAP_DIM),
    );
    let mut set = TileSet::new(false);
    for t in 0..tw * th {
        let (tx, ty) = (t % tw, t / tw);
        let mut tile = [0u8; 64];
        for (i, b) in tile.iter_mut().enumerate() {
            let (x, y) = (tx * 8 + i % 8, ty * 8 + i / 8);
            if x < w && y < h {
                *b = img.pixels[y * w + x];
            }
        }
        set.insert(tile);
        if set.len() > max {
            return Some((tx, ty));
        }
    }
    None
}

/// Mode 7: the index byte is the colour; with EXTBG bit 7 is priority and PLTE 128..=255 must
/// mirror 0..=127 (the hardware's second half).
fn keep_m7(
    opts: &ConvertOptions,
    png: &IndexedPng,
    findings: &mut Vec<Finding>,
) -> (SourcePayload, SourceMeta) {
    let img = &png.image;
    let extbg = opts.extbg == Some(true);
    let mask = if extbg { 0x7f } else { 0xff };
    if extbg {
        for k in 1..128 {
            if img
                .palette
                .get(128 + k)
                .is_some_and(|&c| c != img.palette[k])
            {
                let mut f = Finding::new(
                    Rule::Plte,
                    format!("Mode 7 EXTBG: PLTE entry {} must equal entry {k}", 128 + k),
                );
                f.index = Some(128 + k);
                findings.push(f);
                break;
            }
        }
    }
    let top = img
        .pixels
        .iter()
        .map(|&p| (p & mask) as usize)
        .max()
        .unwrap_or(0);
    let palette = (1..=top).map(|i| img.palette[i]).collect();
    let (s, m) = crate::import_m7::m7_from_indexed(
        &img.pixels,
        palette,
        img.width as usize,
        img.height as usize,
        extbg,
    );
    (SourcePayload::M7(s), m)
}

/// The tile budget the back half could only report (it blanks the cells past it): its use, its
/// ceiling and the refusal text.
fn tile_budget(meta: &SourceMeta) -> Option<(usize, usize, String)> {
    if let SourceReport::Mode7 { report } = &meta.report {
        return (report.overflow_tiles > 0).then(|| {
            (
                report.unique_tiles as usize,
                report.tile_capacity as usize,
                format!(
                    "{} unique tiles, but a Mode 7 map holds {}",
                    report.unique_tiles, report.tile_capacity
                ),
            )
        });
    }
    tile_overflow(meta).map(|(unique, kept)| {
        (
            unique,
            kept,
            format!("{unique} tiles, but the budget keeps {kept}"),
        )
    })
}

fn tile_overflow(meta: &SourceMeta) -> Option<(usize, usize)> {
    let (SourceReport::Tile { report }
    | SourceReport::Obj { report }
    | SourceReport::Sheet { report }) = &meta.report
    else {
        return None;
    };
    report.overflows.iter().find_map(|o| match o {
        Overflow::Tiles { unique, kept } => Some((*unique, *kept)),
        _ => None,
    })
}

/// Unique tiles a clean import uses.
fn tiles_used(meta: &SourceMeta) -> Option<usize> {
    match &meta.report {
        SourceReport::Mode7 { report } => Some(report.unique_tiles as usize),
        SourceReport::Tile { report }
        | SourceReport::Obj { report }
        | SourceReport::Sheet { report } => Some(report.unique_tiles),
        _ => None,
    }
}

/// The PLTE must be the palette's rows, entry for entry (BGR555, the precision a PNG round trip keeps).
fn check_plte(
    rs: usize,
    gpl: &PaletteSource,
    plte: &[u16],
    extbg: bool,
    findings: &mut Vec<Finding>,
) {
    let want = crate::render::plte_of(gpl, extbg);
    let first_diff = plte.iter().zip(&want).position(|(a, b)| a != b);
    let (msg, index) = if gpl.row_size as usize != rs {
        (
            format!(
                "the palette has rows of {} colours, but this art's rows are {rs}",
                gpl.row_size
            ),
            None,
        )
    } else if let Some(i) = first_diff {
        (
            format!(
                "PLTE entry {i} is {:#06x}, but the palette has {:#06x}",
                plte[i], want[i]
            ),
            Some(i),
        )
    } else if plte.len() != want.len() {
        (
            format!(
                "the PLTE holds {} entries, but the palette has {}",
                plte.len(),
                want.len()
            ),
            Some(plte.len().min(want.len())),
        )
    } else {
        return;
    };
    let mut f = Finding::new(Rule::Plte, msg);
    f.index = index;
    findings.push(f);
}
