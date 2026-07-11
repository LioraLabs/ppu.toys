//! OBJ (sprite) sheet importer: PNG -> 4bpp OBJ VRAM char + OBJ CGRAM palettes
//! (8-15). Reuses the shared quantizer/dedup/packer (`import::quantize` +
//! `import::tiles`) exactly like the tile-BG importer — no forked quantizer.
//! Sprites address tiles by number, so there is no tilemap: `cells` maps each
//! source 8x8 cell (row-major) to the OBJ tile#/palette/flip it became, for
//! demos/inspector to drive `obj[i]`.

use std::collections::BTreeMap;

use super::quantize::{median_cut, nearest, region_fit};
use super::tiles::{pack_planar, split_tiles, IndexTile, PixelTile, TileSet};
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
/// Pure and deterministic. `cell_size` picks the layout: 8 runs the classic
/// per-tile pipeline (flip-aware dedup, reserved blank tile 0); 16/32/64 run
/// the block layout, where each NxN-tile cell is a straight raster copy into
/// contiguous name-table slots so ONE `obj[i].tile` (the block's base)
/// addresses the whole cell via the renderer's `obj_tile_addr` stride
/// (right +1 within the 16-wide band, down +16).
pub fn import_obj_sheet(
    rgba: &[u8],
    width: u32,
    height: u32,
    cell_size: u8,
) -> (crate::source::ObjSource, crate::source::SourceMeta) {
    match cell_size {
        16 | 32 | 64 => import_obj_blocks(rgba, width, height, cell_size),
        _ => import_obj_cells8(rgba, width, height),
    }
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
            Some(median_cut(&hv, budget)),
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
) -> (crate::source::ObjSource, crate::source::SourceMeta) {
    let mut overflows = Vec::new();
    let (w, h) = (width as usize, height as usize);
    let (ptiles, cols, rows) = split_tiles(rgba, w, h);

    let (map_color, color_overflow) = global_remap(&ptiles);
    overflows.extend(color_overflow);

    let tile_pals: Vec<Vec<u16>> = ptiles
        .iter()
        .map(|t| {
            let mut cs: Vec<u16> = t.iter().flatten().map(|&c| map_color(c)).collect();
            cs.sort_unstable();
            cs.dedup();
            if cs.len() > CAP {
                let hv: Vec<(u16, u32)> = cs.iter().map(|&c| (c, 1)).collect();
                median_cut(&hv, CAP)
            } else {
                cs
            }
        })
        .collect();

    let fit = region_fit(&tile_pals, 8, CAP);
    if fit.palettes_needed > 8 {
        let remapped = tile_pals
            .iter()
            .zip(&fit.assignment)
            .filter(|(tp, &a)| {
                !tp.is_empty() && tp.iter().any(|c| !fit.palettes[a as usize].contains(c))
            })
            .count();
        overflows.push(Overflow::Palettes {
            needed: fit.palettes_needed,
            remapped_tiles: remapped,
        });
    }

    let max_tiles = (0x8000usize / WORDS_PER_TILE).min(512); // OBJ name table = 9-bit tile#
    let mut set = TileSet::new(true);
    set.insert([0u8; 64]);
    let mut cells: Vec<ObjCell> = Vec::with_capacity(ptiles.len());
    for (pt, &pal) in ptiles.iter().zip(&fit.assignment) {
        let palette: &[u16] = fit
            .palettes
            .get(pal as usize)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        let grid: IndexTile = std::array::from_fn(|i| match pt[i] {
            Some(c) if !palette.is_empty() => nearest(palette, map_color(c)) as u8 + 1,
            _ => 0,
        });
        let (n, hf, vf) = set.insert(grid);
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
            palettes: fit.palettes,
            char_words,
        },
        crate::source::SourceMeta {
            width,
            height,
            report: crate::source::SourceReport::Obj { report },
            cells: Some(cells),
        },
    )
}

/// 16/32/64 block layout: each cell is an NxN block of 8x8 subtiles copied
/// raster-order into contiguous name-table slots (`base + sy*16 + sx`,
/// matching `sprite::obj_tile_addr`). One palette per CELL (one OAM entry =
/// one palette), no dedup, no flips (an OAM flip flips the whole sprite).
fn import_obj_blocks(
    rgba: &[u8],
    width: u32,
    height: u32,
    cell_size: u8,
) -> (crate::source::ObjSource, crate::source::SourceMeta) {
    let mut overflows = Vec::new();
    let n = (cell_size / 8) as usize; // tiles per cell edge: 2/4/8
    let per_row = 16 / n; // cells per 16-wide name-table band: 8/4/2
    let (ptiles, cols, rows) = split_tiles(rgba, width as usize, height as usize);
    let big_cols = cols.div_ceil(n);
    let big_rows = rows.div_ceil(n);
    let ncells = big_cols * big_rows;

    // Subtile (sy, sx) of cell (cy, cx); None = off-sheet (fully transparent).
    let subtile = |cy: usize, cx: usize, sy: usize, sx: usize| -> Option<&PixelTile> {
        let (gy, gx) = (cy * n + sy, cx * n + sx);
        (gy < rows && gx < cols).then(|| &ptiles[gy * cols + gx])
    };

    let (map_color, color_overflow) = global_remap(&ptiles);
    overflows.extend(color_overflow);

    // Per-CELL palettes: one OAM entry = one palette, so all subtiles of a
    // cell must share it.
    let cell_pals: Vec<Vec<u16>> = (0..ncells)
        .map(|k| {
            let (cy, cx) = (k / big_cols, k % big_cols);
            let mut cs: Vec<u16> = Vec::new();
            for sy in 0..n {
                for sx in 0..n {
                    if let Some(t) = subtile(cy, cx, sy, sx) {
                        for c in t.iter().flatten() {
                            cs.push(map_color(*c));
                        }
                    }
                }
            }
            cs.sort_unstable();
            cs.dedup();
            if cs.len() > CAP {
                let hv: Vec<(u16, u32)> = cs.iter().map(|&c| (c, 1)).collect();
                median_cut(&hv, CAP)
            } else {
                cs
            }
        })
        .collect();

    let fit = region_fit(&cell_pals, 8, CAP);
    if fit.palettes_needed > 8 {
        let remapped = cell_pals
            .iter()
            .zip(&fit.assignment)
            .filter(|(cp, &a)| {
                !cp.is_empty() && cp.iter().any(|c| !fit.palettes[a as usize].contains(c))
            })
            .count();
        overflows.push(Overflow::Palettes {
            needed: fit.palettes_needed,
            remapped_tiles: remapped,
        });
    }

    // Name table = 512 8x8 tiles; each cell consumes n*n of them.
    let max_cells = 512 / (n * n);
    let kept_cells = ncells.min(max_cells);
    if ncells > max_cells {
        overflows.push(Overflow::Tiles {
            unique: ncells * n * n,
            kept: kept_cells * n * n,
        });
    }

    // Base tile of kept cell k: cells pack per_row per 16-wide band; each
    // band-row of cells spans n tile rows.
    let base_of = |k: usize| -> u16 {
        let gx = (k % per_row) * n;
        let gy = (k / per_row) * n;
        (gy * 16 + gx) as u16
    };
    let tile_rows = kept_cells.div_ceil(per_row) * n; // 8x8 name-table rows used
    let mut char_words = vec![0u16; tile_rows * 16 * WORDS_PER_TILE];
    for k in 0..kept_cells {
        let (cy, cx) = (k / big_cols, k % big_cols);
        let palette: &[u16] = fit
            .palettes
            .get(fit.assignment[k] as usize)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        let base = base_of(k) as usize;
        for sy in 0..n {
            for sx in 0..n {
                let grid: IndexTile = match subtile(cy, cx, sy, sx) {
                    Some(pt) => std::array::from_fn(|i| match pt[i] {
                        Some(c) if !palette.is_empty() => nearest(palette, map_color(c)) as u8 + 1,
                        _ => 0,
                    }),
                    None => [0u8; 64],
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
        unique_tiles: kept_cells * n * n,
        vram_words: char_words.len(),
        overflows,
    };
    (
        crate::source::ObjSource {
            cell_size,
            palettes: fit.palettes,
            char_words,
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
        let (src, meta) = import_obj_sheet(&two_tile_rgba(), 16, 8, 8);
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
        let (src, _meta) = import_obj_sheet(&two_tile_rgba(), 16, 8, 8);
        let mut mem = Memory::new();
        crate::source::place_obj(&src, &mut mem, 0x2000);
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
        let (_src, meta) = import_obj_sheet(&v, 16, 8, 8);
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
        let (src, meta) = import_obj_sheet(&rgba, 16, 16, 8);
        assert_eq!(src.cell_size, 8);
        assert_eq!(meta.cells.as_ref().unwrap().len(), 4); // 4 source cells
                                                           // Every emitted tile is exactly 16 words (one 8x8 4bpp tile) — no size coupling.
        assert_eq!(src.char_words.len() % WORDS_PER_TILE, 0);
        // The import signature takes (rgba, w, h, cell_size) and returns a source + meta pair.
        let _f: fn(&[u8], u32, u32, u8) -> (crate::source::ObjSource, crate::source::SourceMeta) =
            import_obj_sheet;
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
        let (src, meta) = import_obj_sheet(&rgba, 16, 16, 16);
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
        let (_src, meta) = import_obj_sheet(&rgba, 32, 16, 16);
        let cells = meta.cells.as_ref().unwrap();
        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0].tile, 0);
        assert_eq!(cells[1].tile, 2); // two 8x8 tiles right of cell 0's base
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
        let (src, meta) = import_obj_sheet(&rgba, 16, 16, 16);
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
        let (_src, meta) = import_obj_sheet(&rgba, 576, 64, 64);
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
        let (src, meta) = import_obj_sheet(&vec![0u8; 8 * 8 * 4], 8, 8, 8);
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
