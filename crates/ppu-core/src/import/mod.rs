//! Asset importers: PNG -> authentic VRAM/CGRAM/register data (m4/importer).
//! `quantize`/`tiles` are the shared primitives Mode-7 and OBJ import reuse;
//! this module's own surface is the tile-BG importer.

pub mod dither;
pub mod obj;
pub mod quantize;
pub mod tiles;

use std::collections::BTreeMap;

use serde::Serialize;

use self::dither::{remap_image, RemapOptions};
use self::quantize::{fit_palettes, reduce_palette};
use self::tiles::{pack_planar, split_tiles_at, TileSet};

/// Importer input format. `bit_depth` comes from the target layer's slot in
/// the mode table (`modes::mode_info`) — the BGMODE value itself is omitted
/// from the cache key on purpose: mode only affects import output via
/// bit-depth, so caching by bit-depth avoids spurious re-quantization.
///
/// Bases (map_base/char_base) are NOT format — they're PLACEMENT,
/// decided when a source is written into VRAM (see `crate::source::place_bg`),
/// not when it's authored. They were dropped from this struct in the source-
/// payload refactor.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ImportOptions {
    /// Target bits per pixel: 2, 4, or 8.
    pub bit_depth: u8,
    /// Tile edge in px. Only 8 is packed today; 16 falls back to 8 and is
    /// reported as `Overflow::TileSize16`.
    pub tile_size: u8,
    /// Remap-stage look: dither mode/strength + opacity cutoff.
    pub remap: RemapOptions,
}

impl Default for ImportOptions {
    fn default() -> Self {
        ImportOptions {
            bit_depth: 4,
            tile_size: 8,
            remap: RemapOptions::default(),
        }
    }
}

/// One honest budget overflow. Structured for the UI (m4/inspector); the
/// importer never silently truncates.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind")]
pub enum Overflow {
    /// Image exceeded the 512px (64-tile) map edge; excess cropped.
    Cropped { max_px: u32 },
    /// Distinct colors exceeded the global budget; median-cut down.
    Colors { unique: usize, budget: usize },
    /// Needed more than 8 sub-palettes; overflow tiles were remapped into
    /// their closest sealed palette.
    Palettes {
        needed: usize,
        remapped_tiles: usize,
    },
    /// Tiles exceeded the char budget; `kept` were emitted. For an assembled
    /// bg/obj import `unique` is the deduped tile count and excess map cells
    /// fall back to the blank tile; for a SHEET (no dedup, no blank tile)
    /// `unique` is simply the sheet's cell count and cells past `kept` have no
    /// char data at all — their `cells[k].tile` still names the true index.
    Tiles { unique: usize, kept: usize },
    /// 16x16 import is not implemented; imported as 8x8.
    TileSize16,
}

/// Colors/palettes/tiles/VRAM accounting + honest overflows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BudgetReport {
    /// CGRAM entries written (sum of sub-palette sizes, excl. transparent 0s).
    pub colors_used: usize,
    pub palettes_used: usize,
    /// Map cells covered by the (cropped) image.
    pub tile_cells: usize,
    /// Deduped art tiles (excl. the reserved blank tile 0). A sheet import
    /// dedups nothing and reserves nothing, so there it is just the number of
    /// chars emitted (cell count, or 1024 when the ceiling clipped it).
    pub unique_tiles: usize,
    /// Total VRAM words emitted (char + tilemap).
    pub vram_words: usize,
    pub overflows: Vec<Overflow>,
}

/// Steps 3-5 of both tile importers: global colour budget over every cell,
/// per-cell weighted histograms of the budgeted colours, then the sub-palette
/// fit — pushing `Overflow::Colors` / `Overflow::Palettes` as it goes. Shared
/// by `import_tile_bg` and `import_tile_sheet`, which differ only in what they
/// do with the resulting grids (tilemap + dedup vs. straight sheet order).
fn fit_cell_palettes(
    ptiles: &[self::tiles::PixelTile],
    palette_count: usize,
    cap: usize,
    overflows: &mut Vec<Overflow>,
) -> self::quantize::RegionFit {
    let mut hist: BTreeMap<u16, u32> = BTreeMap::new();
    for t in ptiles {
        for c in t.iter().flatten() {
            *hist.entry(*c).or_default() += 1;
        }
    }
    let budget = palette_count * cap;
    let global: Option<Vec<u16>> = if hist.len() > budget {
        overflows.push(Overflow::Colors {
            unique: hist.len(),
            budget,
        });
        let hv: Vec<(u16, u32)> = hist.iter().map(|(&c, &n)| (c, n)).collect();
        Some(reduce_palette(&hv, budget))
    } else {
        None
    };
    let map_color = |c: u16| global.as_ref().map_or(c, |g| g[quantize::nearest(g, c)]);

    let tile_hists: Vec<Vec<(u16, u32)>> = ptiles
        .iter()
        .map(|t| {
            let mut m: BTreeMap<u16, u32> = BTreeMap::new();
            for c in t.iter().flatten() {
                *m.entry(map_color(*c)).or_default() += 1;
            }
            m.into_iter().collect()
        })
        .collect();

    let fit = fit_palettes(&tile_hists, palette_count, cap);
    if fit.palettes_needed > palette_count {
        let remapped = tile_hists
            .iter()
            .zip(&fit.assignment)
            .filter(|(th, &a)| {
                !th.is_empty()
                    && th
                        .iter()
                        .any(|(c, _)| !fit.palettes[a as usize].contains(c))
            })
            .count();
        overflows.push(Overflow::Palettes {
            needed: fit.palettes_needed,
            remapped_tiles: remapped,
        });
    }
    fit
}

/// Convert an RGBA image into authentic tile-BG data: split -> global color
/// budget -> multi-palette region fit -> nearest-remap -> flip-aware dedup ->
/// bitplane pack + screen-ordered tilemap. Pure and deterministic: identical
/// inputs yield identical outputs. Returns the render-data payload
/// (`BgSource`) plus the dims/budget that travel alongside it (`SourceMeta`);
/// placement (VRAM/CGRAM bases) is a concern handled separately by
/// `crate::source::place_bg`.
pub fn import_tile_bg(
    rgba: &[u8],
    width: u32,
    height: u32,
    opts: &ImportOptions,
) -> (crate::source::BgSource, crate::source::SourceMeta) {
    let mut overflows = Vec::new();
    let (bpp, cap, palette_count) = match opts.bit_depth {
        2 => (2u8, 3usize, 8usize),
        8 => (8u8, 255usize, 1usize),
        _ => (4u8, 15usize, 8usize),
    };
    if opts.tile_size >= 16 {
        overflows.push(Overflow::TileSize16);
    }

    // 1. crop to the 64x64-tile (512px) map limit
    let (w, h) = ((width as usize).min(512), (height as usize).min(512));
    if (w, h) != (width as usize, height as usize) {
        overflows.push(Overflow::Cropped { max_px: 512 });
    }
    let cropped: std::borrow::Cow<[u8]> = if w == width as usize && h == height as usize {
        rgba.into()
    } else {
        let mut v = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            let off = y * width as usize * 4;
            v.extend_from_slice(&rgba[off..off + w * 4]);
        }
        v.into()
    };

    // 2. tile split (BGR555 + transparency happen here)
    let (ptiles, cols, rows) = split_tiles_at(&cropped, w, h, opts.remap.alpha_threshold);

    // 3-5. global color budget -> per-tile histograms -> sub-palette fit
    let fit = fit_cell_palettes(&ptiles, palette_count, cap, &mut overflows);

    // 6. dither-aware index remap (against the ORIGINAL 8-bit pixels) +
    //    flip-aware dedup; tile 0 reserved blank so padding and dropped cells
    //    are honestly transparent
    let grids = remap_image(
        &cropped,
        w,
        h,
        &fit.palettes,
        &fit.assignment,
        cols,
        rows,
        &opts.remap,
    );
    let words_per_tile = bpp as usize * 4;
    let max_tiles = 1024usize; // 10-bit tilemap tile field — placement-fit is a placement concern
    let mut set = TileSet::new(true);
    set.insert([0u8; 64]);
    let mut cells: Vec<u16> = Vec::with_capacity(grids.len());
    for (grid, &pal) in grids.iter().zip(&fit.assignment) {
        let (n, hf, vf) = set.insert(*grid);
        let word = if (n as usize) >= max_tiles {
            0 // over char budget: blank cell (reported below, never mangled)
        } else {
            n | ((pal as u16 & 7) << 10) | ((hf as u16) << 14) | ((vf as u16) << 15)
        };
        cells.push(word);
    }
    let unique_tiles = set.len() - 1; // excl. reserved blank
    let kept = set.len().min(max_tiles);
    if set.len() > max_tiles {
        overflows.push(Overflow::Tiles {
            unique: unique_tiles,
            kept: kept - 1,
        });
    }

    // 7. char emit
    let mut char_words = Vec::with_capacity(kept * words_per_tile);
    for t in &set.tiles()[..kept] {
        char_words.extend(pack_planar(t, bpp));
    }

    // 8. screen-ordered tilemap (SC0 TL, SC1 TR, SC2 BL, SC3 BR)
    let map_cols = if cols > 32 { 64 } else { 32 };
    let map_rows = if rows > 32 { 64 } else { 32 };
    let screen_size = match (map_cols, map_rows) {
        (32, 32) => 0u8,
        (64, 32) => 1,
        (32, 64) => 2,
        _ => 3,
    };
    let n_screens = (map_cols / 32) * (map_rows / 32);
    let mut tilemap_words = vec![0u16; n_screens * 0x400];
    for ty in 0..rows {
        for tx in 0..cols {
            let sc = (ty / 32) * (map_cols / 32) + (tx / 32);
            tilemap_words[sc * 0x400 + (ty % 32) * 32 + (tx % 32)] = cells[ty * cols + tx];
        }
    }

    let report = BudgetReport {
        colors_used: fit.palettes.iter().map(|p| p.len()).sum(),
        palettes_used: fit.palettes.len(),
        tile_cells: cols * rows,
        unique_tiles,
        vram_words: char_words.len() + tilemap_words.len(),
        overflows,
    };
    (
        crate::source::BgSource {
            bit_depth: bpp,
            tile_size: 8,
            palettes: fit.palettes,
            char_words,
            screen_size,
            tilemap_words,
        },
        crate::source::SourceMeta {
            width,
            height,
            report: crate::source::SourceReport::Tile { report },
            cells: None,
        },
    )
}

/// Convert an RGBA tilesheet into BG char data: the same pipeline as
/// [`import_tile_bg`] minus the 512px crop, the flip-aware dedup and the
/// tilemap. Chars come out in row-major sheet order with NO reserved blank
/// tile, so char N is the Nth 8x8 cell of the PNG and `bg[n].map[x][y] =
/// {tile = N}` addresses it directly against author-set geometry.
pub fn import_tile_sheet(
    rgba: &[u8],
    width: u32,
    height: u32,
    opts: &ImportOptions,
) -> (crate::source::SheetSource, crate::source::SourceMeta) {
    let mut overflows = Vec::new();
    let (bpp, cap, palette_count) = match opts.bit_depth {
        2 => (2u8, 3usize, 8usize),
        8 => (8u8, 255usize, 1usize),
        _ => (4u8, 15usize, 8usize),
    };
    let (w, h) = (width as usize, height as usize);

    let (ptiles, cols, rows) = split_tiles_at(rgba, w, h, opts.remap.alpha_threshold);

    let fit = fit_cell_palettes(&ptiles, palette_count, cap, &mut overflows);

    let grids = remap_image(
        rgba,
        w,
        h,
        &fit.palettes,
        &fit.assignment,
        cols,
        rows,
        &opts.remap,
    );

    // No dedup and no reserved blank: cell k IS char k. Only the 10-bit
    // tilemap tile field caps how many of them a map can name.
    let ncells = grids.len();
    let max_tiles = 1024usize;
    let kept = ncells.min(max_tiles);
    if ncells > max_tiles {
        overflows.push(Overflow::Tiles {
            unique: ncells,
            kept,
        });
    }
    let words_per_tile = bpp as usize * 4;
    let mut char_words = Vec::with_capacity(kept * words_per_tile);
    for g in &grids[..kept] {
        char_words.extend(pack_planar(g, bpp));
    }

    // Cells keep their TRUE sheet index even past the ceiling: an honest
    // "this cell has no char" beats renumbering a WYSIWYG sheet.
    let cells: Vec<obj::ObjCell> = fit
        .assignment
        .iter()
        .enumerate()
        .map(|(k, &pal)| obj::ObjCell {
            tile: k as u16,
            pal: pal & 7,
            flip_x: false,
            flip_y: false,
        })
        .collect();

    let report = BudgetReport {
        colors_used: fit.palettes.iter().map(|p| p.len()).sum(),
        palettes_used: fit.palettes.len(),
        tile_cells: ncells,
        unique_tiles: kept,
        vram_words: char_words.len(),
        overflows,
    };
    (
        crate::source::SheetSource {
            bit_depth: bpp,
            palettes: fit.palettes,
            char_words,
        },
        crate::source::SourceMeta {
            width,
            height,
            report: crate::source::SourceReport::Sheet { report },
            cells: Some(cells),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 16x8 RGBA: tile A all red; tile B left half red, right half blue.
    fn two_tile_rgba() -> Vec<u8> {
        let mut v = Vec::new();
        for _y in 0..8 {
            for x in 0..16 {
                if x < 12 {
                    v.extend_from_slice(&[255, 0, 0, 255]);
                } else {
                    v.extend_from_slice(&[0, 0, 255, 255]);
                }
            }
        }
        v
    }

    #[test]
    fn imports_two_tiles_one_palette_4bpp() {
        let (src, meta) = import_tile_bg(&two_tile_rgba(), 16, 8, &ImportOptions::default());
        // palette 0 sorted: red 0x001f -> index 1, blue 0x7c00 -> index 2
        assert_eq!(src.palettes, vec![vec![0x001f, 0x7c00]]);
        // tile 0 = reserved blank, tile 1 = all-red, tile 2 = half/half
        assert_eq!(src.char_words.len(), 3 * 16);
        assert_eq!(&src.char_words[0..16], &[0u16; 16]); // blank
        assert_eq!(src.char_words[16], 0x00ff); // all index 1: plane0 full row
        assert_eq!(src.char_words[24], 0x0000); // planes 2/3 empty
        assert_eq!(src.char_words[32], 0x0ff0); // 1111 2222 row
                                                // tilemap: 32x32 screen, cells (0,0)=tile1 (1,0)=tile2, pal 0, rest blank
        assert_eq!(src.tilemap_words.len(), 0x400);
        assert_eq!(src.tilemap_words[0], 0x0001);
        assert_eq!(src.tilemap_words[1], 0x0002);
        assert!(src.tilemap_words[2..].iter().all(|&w| w == 0));
        // registers + report
        assert_eq!((src.screen_size, src.tile_size), (0, 8));
        let crate::source::SourceReport::Tile { report } = &meta.report else {
            panic!("expected tile report");
        };
        assert_eq!(report.palettes_used, 1);
        assert_eq!(report.colors_used, 2);
        assert_eq!(report.tile_cells, 2);
        assert_eq!(report.unique_tiles, 2);
        assert!(report.overflows.is_empty());
    }

    // PPU-93: char N is the Nth PNG cell — sheet order, no dedup, no blank tile.
    #[test]
    fn sheet_emits_chars_in_sheet_order_with_no_reserved_blank() {
        let (src, meta) = import_tile_sheet(&two_tile_rgba(), 16, 8, &ImportOptions::default());
        assert_eq!(src.palettes, vec![vec![0x001f, 0x7c00]]);
        // two source cells -> exactly two chars, the FIRST of them being cell 0.
        assert_eq!(src.char_words.len(), 2 * 16);
        assert_eq!(src.char_words[0], 0x00ff); // char 0 = all-red cell
        assert_eq!(src.char_words[16], 0x0ff0); // char 1 = half/half cell
        let cells = meta.cells.as_ref().unwrap();
        assert_eq!(
            cells[0],
            crate::import::obj::ObjCell {
                tile: 0,
                pal: 0,
                flip_x: false,
                flip_y: false
            }
        );
        assert_eq!(cells[1].tile, 1);
        let crate::source::SourceReport::Sheet { report } = &meta.report else {
            panic!("expected sheet report");
        };
        assert_eq!(report.tile_cells, 2);
        assert!(report.overflows.is_empty());
    }

    // PPU-93: identical cells are NOT deduped — sheet indexing must stay WYSIWYG.
    #[test]
    fn sheet_keeps_duplicate_and_mirrored_cells_distinct() {
        // three cells: A, A again, and A mirrored. Dedup would collapse to one.
        let mut v = Vec::new();
        for _y in 0..8 {
            for x in 0..24 {
                let red = match x / 8 {
                    0 | 1 => x % 8 < 4,
                    _ => x % 8 >= 4,
                };
                v.extend_from_slice(if red {
                    &[255u8, 0, 0, 255]
                } else {
                    &[0, 0, 255, 255]
                });
            }
        }
        let (src, meta) = import_tile_sheet(&v, 24, 8, &ImportOptions::default());
        assert_eq!(src.char_words.len(), 3 * 16);
        let cells = meta.cells.as_ref().unwrap();
        assert_eq!([cells[0].tile, cells[1].tile, cells[2].tile], [0, 1, 2]);
        assert!(cells.iter().all(|c| !c.flip_x && !c.flip_y));
        assert_eq!(&src.char_words[0..16], &src.char_words[16..32]); // cell 1 stored again
    }

    // PPU-93: no 512px crop, and the 1024-char ceiling reports through the
    // existing overflow/budget report.
    #[test]
    fn sheet_skips_the_512px_crop_and_reports_the_1024_char_ceiling() {
        // 8px tall, 8264px wide -> 1033 cells: past 512px AND past 1024 chars.
        let mut v = Vec::new();
        for _y in 0..8 {
            for x in 0..8264 {
                let red = (x / 8) % 2 == 0;
                v.extend_from_slice(if red {
                    &[255u8, 0, 0, 255]
                } else {
                    &[0, 0, 255, 255]
                });
            }
        }
        let (src, meta) = import_tile_sheet(&v, 8264, 8, &ImportOptions::default());
        let crate::source::SourceReport::Sheet { report } = &meta.report else {
            panic!("expected sheet report");
        };
        // every cell of the sheet is accounted for: nothing was cropped away
        assert_eq!(report.tile_cells, 1033);
        assert_eq!(meta.cells.as_ref().unwrap().len(), 1033);
        assert!(!report
            .overflows
            .iter()
            .any(|o| matches!(o, Overflow::Cropped { .. })));
        // ...but only 1024 chars are emitted, and that is reported honestly
        assert_eq!(src.char_words.len(), 1024 * 16);
        assert!(report.overflows.contains(&Overflow::Tiles {
            unique: 1033,
            kept: 1024
        }));
    }

    #[test]
    fn imports_2bpp_palette_is_color_list() {
        let opts = ImportOptions {
            bit_depth: 2,
            ..Default::default()
        };
        let (src, _meta) = import_tile_bg(&two_tile_rgba(), 16, 8, &opts);
        assert_eq!(src.palettes, vec![vec![0x001f, 0x7c00]]);
        assert_eq!(src.char_words.len(), 3 * 8); // 8 words/tile
        assert_eq!(src.char_words[8], 0x00ff);
        assert_eq!(src.char_words[16], 0x0ff0);
        assert_eq!(src.tilemap_words[1], 0x0002);
    }

    #[test]
    fn hflip_mirror_tile_dedups_with_flip_bit() {
        // tile A: left half red / right blue; tile B mirrored
        let mut v = Vec::new();
        for _y in 0..8 {
            for x in 0..16 {
                let red = if x < 8 { x < 4 } else { x >= 12 };
                if red {
                    v.extend_from_slice(&[255, 0, 0, 255]);
                } else {
                    v.extend_from_slice(&[0, 0, 255, 255]);
                }
            }
        }
        let (src, meta) = import_tile_bg(&v, 16, 8, &ImportOptions::default());
        let crate::source::SourceReport::Tile { report } = &meta.report else {
            panic!("expected tile report");
        };
        assert_eq!(report.unique_tiles, 1);
        assert_eq!(src.tilemap_words[0], 0x0001);
        assert_eq!(src.tilemap_words[1], 0x0001 | 1 << 14); // H-flip bit
    }

    #[test]
    fn fully_transparent_image_yields_blank_everything() {
        let rgba = vec![0u8; 8 * 8 * 4];
        let (src, meta) = import_tile_bg(&rgba, 8, 8, &ImportOptions::default());
        assert!(src.palettes.is_empty() || src.palettes.iter().all(|p| p.is_empty()));
        let crate::source::SourceReport::Tile { report } = &meta.report else {
            panic!("expected tile report");
        };
        assert_eq!(report.palettes_used, 0);
        assert_eq!(report.unique_tiles, 0);
        assert_eq!(src.char_words.len(), 16); // just the reserved blank tile
        assert!(src.tilemap_words.iter().all(|&w| w == 0));
    }

    #[test]
    fn wide_image_picks_64x32_screen_and_screen_ordered_map() {
        // 264x8 -> 33 tile columns -> 64x32 map (two 32x32 screens)
        let mut v = Vec::new();
        for _y in 0..8 {
            for x in 0..264 {
                let c: [u8; 4] = if x < 8 {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 0, 0]
                };
                v.extend_from_slice(&c);
            }
        }
        let opts = ImportOptions::default();
        let (src, _meta) = import_tile_bg(&v, 264, 8, &opts);
        assert_eq!(src.screen_size, 1); // 64x32
        assert_eq!(src.tilemap_words.len(), 2 * 0x400);
        assert_eq!(src.tilemap_words[0], 0x0001); // red tile in SC0 cell 0
                                                  // column 32 lives in SC1 (words 0x400..)
        assert!(src.tilemap_words[0x400..].iter().all(|&w| w == 0));
    }

    #[test]
    fn global_color_overflow_median_cuts_and_reports() {
        // 16x16: 256 pixels, 200 distinct colors > 120 budget
        let mut v = Vec::new();
        for i in 0..256u32 {
            let c = i % 200;
            v.extend_from_slice(&[(c % 32 * 8) as u8, (c / 32 * 8 + 8) as u8, 128, 255]);
        }
        let (_src, meta) = import_tile_bg(&v, 16, 16, &ImportOptions::default());
        let crate::source::SourceReport::Tile { report } = &meta.report else {
            panic!("expected tile report");
        };
        assert!(report.colors_used <= 120);
        assert!(report
            .overflows
            .iter()
            .any(|o| matches!(o, Overflow::Colors { budget: 120, .. })));
    }
}
