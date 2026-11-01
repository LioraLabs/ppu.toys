//! OBJ (sprite) sheet importer: PNG -> 4bpp OBJ VRAM char + OBJ CGRAM palettes
//! (8-15). Reuses the shared quantizer/dedup/packer (`import::quantize` +
//! `import::tiles`) exactly like the tile-BG importer — no forked quantizer.
//! Sprites address tiles by number, so there is no tilemap: `cells` maps each
//! source 8x8 cell (row-major) to the OBJ tile#/palette/flip it became, for
//! demos/inspector to drive `obj[i]`.

use std::collections::BTreeMap;

use super::dither::{remap_image, RemapOptions};
use super::quantize::{fit_palettes, nearest, reduce_palette};
use super::tiles::{pack_planar, split_tiles_at, IndexTile, PixelTile, TileSet};
use super::{BudgetReport, Overflow};

/// OBJ 4bpp: 15 colors/sub-palette (index 0 transparent), 8 palettes, 16
/// words/tile. CGRAM base 128 (OBJ palettes 8-15) is `place_obj`'s concern.
const CAP: usize = 15;
const WORDS_PER_TILE: usize = 16;

/// The OBJ attributes one source cell resolved to. `tile` indexes the emitted
/// OBJ char data (0 = reserved blank); flips reproduce the source from the
/// deduped stored tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ObjCell {
    pub tile: u16,
    pub pal: u8,
    pub flip_x: bool,
    pub flip_y: bool,
}

/// Convert an RGBA sprite sheet into 4bpp OBJ char + OBJ CGRAM palettes.
/// Pure and deterministic. The cell size (width, height; multiples of 8 up to
/// 128) picks the layout: 8x8 runs the classic per-tile pipeline (flip-aware
/// dedup, reserved blank tile 0); any other runs the block layout, where each
/// cell is a straight raster copy into contiguous name-table slots so ONE
/// `obj[i].tile` (the block's base) addresses the whole cell, or any piece of
/// it, via the renderer's `obj_tile_addr` stride (right +1 within the 16-wide
/// band, down +16).
pub fn import_obj_sheet(
    rgba: &[u8],
    width: u32,
    height: u32,
    cell_size: u8,
    remap: &RemapOptions,
) -> (crate::source::ObjSource, crate::source::SourceMeta) {
    import_obj_sheet_cells(rgba, width, height, (cell_size, cell_size), remap)
}

/// [`import_obj_sheet`] with a rectangular cell (width, height).
pub fn import_obj_sheet_cells(
    rgba: &[u8],
    width: u32,
    height: u32,
    cell: (u8, u8),
    remap: &RemapOptions,
) -> (crate::source::ObjSource, crate::source::SourceMeta) {
    if cell == (8, 8) {
        import_obj_cells8(rgba, width, height, remap)
    } else {
        import_obj_blocks(rgba, width, height, cell, remap)
    }
}

/// The first 8x8 tile row of each of `bands` bands of cells `nh` tiles tall: one after another, except that a
/// band never crosses tile row 16, the seam between the two name tables (`obj.name_select` can move the second).
fn band_rows(nh: usize, bands: usize) -> Vec<usize> {
    let mut rows = Vec::with_capacity(bands);
    let mut at = 0;
    for _ in 0..bands {
        if at < 16 && at + nh > 16 && nh <= 16 {
            at = 16;
        }
        rows.push(at);
        at += nh;
    }
    rows
}

/// Global color budget over all source tiles: returns a remap closure that
/// snaps any BGR555 color to the (<= 8*CAP) budgeted global palette, plus the
/// honest Colors overflow if the image blew the budget.
fn global_remap(ptiles: &[PixelTile]) -> (impl Fn(u16) -> u16, Option<Overflow>) {
    let mut hist: BTreeMap<u16, u32> = BTreeMap::new();
    for t in ptiles {
        for c in t.iter().flatten() {
            *hist.entry(*c).or_default() += 1;
        }
    }
    let budget = 8 * CAP;
    let (global, overflow) = if hist.len() > budget {
        let hv: Vec<(u16, u32)> = hist.iter().map(|(&c, &n)| (c, n)).collect();
        (
            Some(reduce_palette(&hv, budget)),
            Some(Overflow::Colors {
                unique: hist.len(),
                budget,
            }),
        )
    } else {
        (None, None)
    };
    (
        move |c: u16| global.as_ref().map_or(c, |g| g[nearest(g, c)]),
        overflow,
    )
}

/// The classic 8x8 path: split -> global color budget -> region fit ->
/// nearest remap -> flip-aware dedup -> bitplane pack. No CGRAM base baked in
/// (placement's job), no tilemap, a per-cell attribute map.
fn import_obj_cells8(
    rgba: &[u8],
    width: u32,
    height: u32,
    remap: &RemapOptions,
) -> (crate::source::ObjSource, crate::source::SourceMeta) {
    let mut overflows = Vec::new();
    let (w, h) = (width as usize, height as usize);
    let (ptiles, cols, rows) = split_tiles_at(rgba, w, h, remap.alpha_threshold);

    let (map_color, color_overflow) = global_remap(&ptiles);
    overflows.extend(color_overflow);

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

    let fit = fit_palettes(&tile_hists, 8, CAP);
    if fit.palettes_needed > 8 {
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

    let grids = remap_image(
        rgba,
        w,
        h,
        &fit.palettes,
        &fit.assignment,
        cols,
        rows,
        remap,
    );
    let max_tiles = (0x8000usize / WORDS_PER_TILE).min(512); // OBJ name table = 9-bit tile#
    let mut set = TileSet::new(true);
    set.insert([0u8; 64]);
    let mut cells: Vec<ObjCell> = Vec::with_capacity(grids.len());
    for (grid, &pal) in grids.iter().zip(&fit.assignment) {
        let (n, hf, vf) = set.insert(*grid);
        let tile = if (n as usize) >= max_tiles { 0 } else { n };
        cells.push(ObjCell {
            tile,
            pal: pal & 7,
            flip_x: hf,
            flip_y: vf,
        });
    }
    let unique_tiles = set.len() - 1;
    let kept = set.len().min(max_tiles);
    if set.len() > max_tiles {
        overflows.push(Overflow::Tiles {
            unique: unique_tiles,
            kept: kept - 1,
        });
    }

    let mut char_words = Vec::with_capacity(kept * WORDS_PER_TILE);
    for t in &set.tiles()[..kept] {
        char_words.extend(pack_planar(t, 4));
    }

    let report = BudgetReport {
        colors_used: fit.palettes.iter().map(|p| p.len()).sum(),
        palettes_used: fit.palettes.len(),
        tile_cells: cols * rows,
        unique_tiles,
        vram_words: char_words.len(),
        overflows,
    };
    (
        crate::source::ObjSource {
            cell_size: 8,
            cell_height: 8,
            palettes: fit.palettes,
            char_words,
            cells: cells.clone(),
        },
        crate::source::SourceMeta {
            width,
            height,
            report: crate::source::SourceReport::Obj { report },
            cells: Some(cells),
        },
    )
}

/// Block layout: each cell is an nw x nh block of 8x8 subtiles copied
/// raster-order into contiguous name-table slots (`base + sy*16 + sx`,
/// matching `sprite::obj_tile_addr`), never across the 16-wide band's edge.
/// One palette per CELL (one OAM entry = one palette), no dedup, no flips
/// (an OAM flip flips the whole sprite).
fn import_obj_blocks(
    rgba: &[u8],
    width: u32,
    height: u32,
    (cell_w, cell_h): (u8, u8),
    remap: &RemapOptions,
) -> (crate::source::ObjSource, crate::source::SourceMeta) {
    let mut overflows = Vec::new();
    let (nw, nh) = ((cell_w / 8) as usize, (cell_h / 8) as usize); // tiles per cell edge
    let per_row = 16 / nw; // cells per 16-wide name-table band
    let (ptiles, cols, rows) =
        split_tiles_at(rgba, width as usize, height as usize, remap.alpha_threshold);
    let big_cols = cols.div_ceil(nw);
    let big_rows = rows.div_ceil(nh);
    let ncells = big_cols * big_rows;

    // Subtile (sy, sx) of cell (cy, cx); None = off-sheet (fully transparent).
    let subtile = |cy: usize, cx: usize, sy: usize, sx: usize| -> Option<&PixelTile> {
        let (gy, gx) = (cy * nh + sy, cx * nw + sx);
        (gy < rows && gx < cols).then(|| &ptiles[gy * cols + gx])
    };

    let (map_color, color_overflow) = global_remap(&ptiles);
    overflows.extend(color_overflow);

    // Per-CELL weighted histograms: one OAM entry = one palette, so all
    // subtiles of a cell must share it.
    let cell_hists: Vec<Vec<(u16, u32)>> = (0..ncells)
        .map(|k| {
            let (cy, cx) = (k / big_cols, k % big_cols);
            let mut m: BTreeMap<u16, u32> = BTreeMap::new();
            for sy in 0..nh {
                for sx in 0..nw {
                    if let Some(t) = subtile(cy, cx, sy, sx) {
                        for c in t.iter().flatten() {
                            *m.entry(map_color(*c)).or_default() += 1;
                        }
                    }
                }
            }
            m.into_iter().collect()
        })
        .collect();

    let fit = fit_palettes(&cell_hists, 8, CAP);
    if fit.palettes_needed > 8 {
        let remapped = cell_hists
            .iter()
            .zip(&fit.assignment)
            .filter(|(ch, &a)| {
                !ch.is_empty()
                    && ch
                        .iter()
                        .any(|(c, _)| !fit.palettes[a as usize].contains(c))
            })
            .count();
        overflows.push(Overflow::Palettes {
            needed: fit.palettes_needed,
            remapped_tiles: remapped,
        });
    }

    // Dither-aware remap over the whole sheet: every 8x8 tile inherits its
    // cell's palette assignment.
    let tile_assignment: Vec<u8> = (0..rows * cols)
        .map(|t| {
            let (gy, gx) = (t / cols, t % cols);
            fit.assignment[(gy / nh) * big_cols + gx / nw]
        })
        .collect();
    let grids = remap_image(
        rgba,
        width as usize,
        height as usize,
        &fit.palettes,
        &tile_assignment,
        cols,
        rows,
        remap,
    );

    // Name table = 512 8x8 tiles in 32 rows of 16; cells pack per_row per
    // 16-wide band, each band nh tile rows tall.
    let bands = band_rows(nh, ncells.div_ceil(per_row));
    let max_cells = bands.iter().take_while(|r| *r + nh <= 32).count() * per_row;
    let kept_cells = ncells.min(max_cells);
    if ncells > max_cells {
        overflows.push(Overflow::Tiles {
            unique: ncells * nw * nh,
            kept: kept_cells * nw * nh,
        });
    }

    // Base tile of kept cell k.
    let base_of = |k: usize| -> u16 {
        let gx = (k % per_row) * nw;
        let gy = bands[k / per_row];
        (gy * 16 + gx) as u16
    };
    let tile_rows = if kept_cells == 0 { 0 } else { bands[(kept_cells - 1) / per_row] + nh }; // 8x8 name-table rows used
    let mut char_words = vec![0u16; tile_rows * 16 * WORDS_PER_TILE];
    for k in 0..kept_cells {
        let (cy, cx) = (k / big_cols, k % big_cols);
        let base = base_of(k) as usize;
        for sy in 0..nh {
            for sx in 0..nw {
                let (gy, gx) = (cy * nh + sy, cx * nw + sx);
                let grid: IndexTile = if gy < rows && gx < cols {
                    grids[gy * cols + gx]
                } else {
                    [0u8; 64] // off-sheet subtile: transparent
                };
                let words = pack_planar(&grid, 4);
                let slot = base + sy * 16 + sx;
                char_words[slot * WORDS_PER_TILE..(slot + 1) * WORDS_PER_TILE]
                    .copy_from_slice(&words);
            }
        }
    }

    let cells: Vec<ObjCell> = (0..ncells)
        .map(|k| ObjCell {
            tile: if k < kept_cells { base_of(k) } else { 0 },
            pal: fit.assignment[k] & 7,
            flip_x: false,
            flip_y: false,
        })
        .collect();

    let report = BudgetReport {
        colors_used: fit.palettes.iter().map(|p| p.len()).sum(),
        palettes_used: fit.palettes.len(),
        tile_cells: ncells,
        unique_tiles: kept_cells * nw * nh,
        vram_words: char_words.len(),
        overflows,
    };
    (
        crate::source::ObjSource {
            cell_size: cell_w,
            cell_height: cell_h,
            palettes: fit.palettes,
            char_words,
            cells: cells.clone(),
        },
        crate::source::SourceMeta {
            width,
            height,
            report: crate::source::SourceReport::Obj { report },
            cells: Some(cells),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Memory;

    /// 16x8 RGBA: left tile all red, right tile left-half red / right-half blue.
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
    fn imports_two_obj_tiles_into_palette_8() {
        let (src, meta) = import_obj_sheet(&two_tile_rgba(), 16, 8, 8, &RemapOptions::default());
        assert_eq!(src.palettes, vec![vec![0x001f, 0x7c00]]);
        assert_eq!(src.char_words.len(), 3 * 16);
        assert_eq!(&src.char_words[0..16], &[0u16; 16]);
        assert_eq!(src.char_words[16], 0x00ff);
        assert_eq!(src.char_words[32], 0x0ff0);
        let cells = meta.cells.as_ref().unwrap();
        assert_eq!(
            cells[0],
            ObjCell {
                tile: 1,
                pal: 0,
                flip_x: false,
                flip_y: false
            }
        );
        assert_eq!(
            cells[1],
            ObjCell {
                tile: 2,
                pal: 0,
                flip_x: false,
                flip_y: false
            }
        );
        let crate::source::SourceReport::Obj { report } = &meta.report else {
            panic!("expected obj report");
        };
        assert_eq!(report.palettes_used, 1);
        assert_eq!(report.colors_used, 2);
        assert_eq!(report.unique_tiles, 2);
        assert!(report.overflows.is_empty());
    }

    #[test]
    fn place_obj_writes_char_words_and_obj_cgram() {
        let (src, _meta) = import_obj_sheet(&two_tile_rgba(), 16, 8, 8, &RemapOptions::default());
        let mut mem = Memory::new();
        crate::source::place_obj(&src, &mut mem, 0x2000, 0);
        assert_eq!(mem.vram[0x2000 + 16], 0x00ff);
        assert_eq!(mem.cgram[129], 0x001f);
        assert_eq!(mem.cgram[130], 0x7c00);
        assert_eq!(mem.cgram[1], 0);
    }

    #[test]
    fn hflip_mirror_dedups_with_flip_bit() {
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
        let (_src, meta) = import_obj_sheet(&v, 16, 8, 8, &RemapOptions::default());
        let crate::source::SourceReport::Obj { report } = &meta.report else {
            panic!("expected obj report");
        };
        assert_eq!(report.unique_tiles, 1);
        let cells = meta.cells.as_ref().unwrap();
        assert_eq!(cells[0].tile, 1);
        assert_eq!(
            cells[1],
            ObjCell {
                tile: 1,
                pal: 0,
                flip_x: true,
                flip_y: false
            }
        );
    }

    #[test]
    fn import_is_size_agnostic_emits_8x8_tiles_only() {
        // A 16x16 sheet imports as four independent 8x8 cells regardless of any OBJ
        // size selector — sizing is a runtime OAM/OBSEL property, not an import input.
        let mut rgba = Vec::new();
        for y in 0..16 {
            for x in 0..16 {
                let on = (x / 8 + y / 8) % 2 == 0;
                rgba.extend_from_slice(if on {
                    &[255, 0, 0, 255]
                } else {
                    &[0, 0, 255, 255]
                });
            }
        }
        let (src, meta) = import_obj_sheet(&rgba, 16, 16, 8, &RemapOptions::default());
        assert_eq!(src.cell_size, 8);
        assert_eq!(meta.cells.as_ref().unwrap().len(), 4); // 4 source cells
                                                           // Every emitted tile is exactly 16 words (one 8x8 4bpp tile) — no size coupling.
        assert_eq!(src.char_words.len() % WORDS_PER_TILE, 0);
        // The import signature takes (rgba, w, h, cell_size, remap) and returns a source + meta pair.
        let _f: fn(
            &[u8],
            u32,
            u32,
            u8,
            &RemapOptions,
        ) -> (crate::source::ObjSource, crate::source::SourceMeta) = import_obj_sheet;
    }

    #[test]
    fn cell_size_16_lays_subtiles_for_one_large_tile() {
        // 16x16 sheet, four distinct solid quadrants.
        let mut rgba = Vec::new();
        let quads = [[255u8, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0]];
        for y in 0..16 {
            for x in 0..16 {
                let q = &quads[(y / 8) * 2 + x / 8];
                rgba.extend_from_slice(&[q[0], q[1], q[2], 255]);
            }
        }
        let (src, meta) = import_obj_sheet(&rgba, 16, 16, 16, &RemapOptions::default());
        assert_eq!(src.cell_size, 16);
        let cells = meta.cells.as_ref().unwrap();
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].tile, 0); // base of block (0,0)
        assert!(!cells[0].flip_x && !cells[0].flip_y);
        // Subtiles live at name-table slots base+0, +1, +16, +17 (obj_tile_addr stride).
        assert_eq!(src.char_words.len(), 32 * 16); // two 16-wide tile rows of name-table space
                                                   // Non-blank = any nonzero word in the tile's 16-word 4bpp char (a solid
                                                   // tile of palette index 4 has zero planes 0/1, so word 0 alone is not enough).
        let tile_nonblank = |t: usize| src.char_words[t * 16..(t + 1) * 16].iter().any(|&w| w != 0);
        assert!(tile_nonblank(0)); // (0,0)
        assert!(tile_nonblank(1)); // (0,1)
        assert!(tile_nonblank(16)); // (1,0)
        assert!(tile_nonblank(17)); // (1,1)
        assert!(!tile_nonblank(2)); // outside the cell block: blank
    }

    #[test]
    fn cell_size_16_second_cell_gets_next_block() {
        // 32x16 sheet -> two 16px cells side by side; cell 1 base = tile 2.
        let mut rgba = Vec::new();
        for _y in 0..16 {
            for x in 0..32 {
                let c: [u8; 4] = if x < 16 {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 255, 255]
                };
                rgba.extend_from_slice(&c);
            }
        }
        let (_src, meta) = import_obj_sheet(&rgba, 32, 16, 16, &RemapOptions::default());
        let cells = meta.cells.as_ref().unwrap();
        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0].tile, 0);
        assert_eq!(cells[1].tile, 2); // two 8x8 tiles right of cell 0's base
    }

    #[test]
    fn odd_and_rectangular_cells_stay_inside_their_band() {
        // 24x24 cells: five per band (3 tiles each), bands 3 rows tall; the sixth band would cross tile row 16.
        let rgba = vec![255u8; 120 * 144 * 4];
        let (src, meta) = import_obj_sheet_cells(&rgba, 120, 144, (24, 24), &RemapOptions::default());
        assert_eq!((src.cell_size, src.cell_height), (24, 24));
        let tiles: Vec<u16> = meta.cells.as_ref().unwrap().iter().map(|c| c.tile).collect();
        assert_eq!(&tiles[..6], &[0, 3, 6, 9, 12, 48]);
        assert_eq!(tiles[25], 256); // band 6 starts at row 16, not 15
        for t in &tiles {
            assert!(t % 16 + 3 <= 16 && (t / 16 < 16) == ((t / 16 + 2) < 16));
        }
        // 48x32: two per band.
        let rgba = vec![255u8; 96 * 32 * 4];
        let (src, meta) = import_obj_sheet_cells(&rgba, 96, 32, (48, 32), &RemapOptions::default());
        assert_eq!((src.cell_size, src.cell_height), (48, 32));
        let tiles: Vec<u16> = meta.cells.as_ref().unwrap().iter().map(|c| c.tile).collect();
        assert_eq!(tiles, vec![0, 6]);
    }

    #[test]
    fn cell_size_blocks_have_uniform_palette_per_cell() {
        // One 16px cell whose four subtiles use 4 different colors -> a single
        // sub-palette serves the whole cell (one OAM entry = one palette).
        let mut rgba = Vec::new();
        let quads = [[255u8, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0]];
        for y in 0..16 {
            for x in 0..16 {
                let q = &quads[(y / 8) * 2 + x / 8];
                rgba.extend_from_slice(&[q[0], q[1], q[2], 255]);
            }
        }
        let (src, meta) = import_obj_sheet(&rgba, 16, 16, 16, &RemapOptions::default());
        assert_eq!(meta.cells.as_ref().unwrap()[0].pal, 0);
        assert_eq!(src.palettes.len(), 1);
        assert_eq!(src.palettes[0].len(), 4);
    }

    #[test]
    fn cell_size_overflow_reports_and_falls_back() {
        // 64px cells: 512/(8*8) = 8 cells fit. A 9-cell sheet (576x64) overflows.
        let mut rgba = Vec::new();
        for _y in 0..64 {
            for x in 0..576 {
                let v = (x / 64 * 20) as u8;
                rgba.extend_from_slice(&[v, 255 - v, 128, 255]);
            }
        }
        let (_src, meta) = import_obj_sheet(&rgba, 576, 64, 64, &RemapOptions::default());
        let cells = meta.cells.as_ref().unwrap();
        assert_eq!(cells.len(), 9);
        assert_eq!(cells[8].tile, 0); // over budget -> honest fallback
        let crate::source::SourceReport::Obj { report } = &meta.report else {
            panic!()
        };
        assert!(report
            .overflows
            .iter()
            .any(|o| matches!(o, Overflow::Tiles { .. })));
    }

    #[test]
    fn fully_transparent_sheet_is_blank() {
        let (src, meta) =
            import_obj_sheet(&vec![0u8; 8 * 8 * 4], 8, 8, 8, &RemapOptions::default());
        assert!(src.palettes.iter().all(|p| p.is_empty()));
        let crate::source::SourceReport::Obj { report } = &meta.report else {
            panic!("expected obj report");
        };
        assert_eq!(report.unique_tiles, 0);
        assert_eq!(src.char_words.len(), 16);
        assert_eq!(
            meta.cells.as_ref().unwrap()[0],
            ObjCell {
                tile: 0,
                pal: 0,
                flip_x: false,
                flip_y: false
            }
        );
    }
}
