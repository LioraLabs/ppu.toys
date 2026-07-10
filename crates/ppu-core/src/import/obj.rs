//! OBJ (sprite) sheet importer: PNG -> 4bpp OBJ VRAM char + OBJ CGRAM palettes
//! (8-15). Reuses the shared quantizer/dedup/packer (`import::quantize` +
//! `import::tiles`) exactly like the tile-BG importer — no forked quantizer.
//! Sprites address tiles by number, so there is no tilemap: `cells` maps each
//! source 8x8 cell (row-major) to the OBJ tile#/palette/flip it became, for
//! demos/inspector to drive `obj[i]`.

use std::collections::BTreeMap;

use super::quantize::{median_cut, nearest, region_fit};
use super::tiles::{pack_planar, split_tiles, IndexTile, TileSet};
use super::{BudgetReport, Overflow};
use crate::memory::Memory;

/// OBJ 4bpp: 15 colors/sub-palette (index 0 transparent), 8 palettes, 16
/// words/tile. CGRAM base 128 (OBJ palettes 8-15).
const CAP: usize = 15;
const PAL_STRIDE: usize = 16;
const WORDS_PER_TILE: usize = 16;
const OBJ_CGRAM_BASE: usize = 128;

/// The OBJ attributes one source cell resolved to. `tile` indexes the emitted
/// OBJ char data (0 = reserved blank); flips reproduce the source from the
/// deduped stored tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjCell {
    pub tile: u16,
    pub pal: u8,
    pub flip_x: bool,
    pub flip_y: bool,
}

/// OBJ importer output: bitplane-packed 4bpp char data (write at the OBJ char
/// base), OBJ CGRAM writes (`(index >= 128, BGR555)`), the source-grid cell map,
/// and the budget report.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjImport {
    pub char_words: Vec<u16>,
    pub cgram: Vec<(u8, u16)>,
    pub cells: Vec<ObjCell>,
    pub cols: usize,
    pub rows: usize,
    pub report: BudgetReport,
}

/// Convert an RGBA sprite sheet into 4bpp OBJ char + OBJ CGRAM palettes.
/// Pure and deterministic. Mirrors the tile-BG importer's pipeline (split ->
/// global color budget -> region fit -> nearest remap -> flip-aware dedup ->
/// bitplane pack) but emits for OBJ: CGRAM base 128, no tilemap, a per-cell
/// attribute map.
pub fn import_obj_sheet(rgba: &[u8], width: u32, height: u32) -> ObjImport {
    let mut overflows = Vec::new();
    let (w, h) = (width as usize, height as usize);
    let (ptiles, cols, rows) = split_tiles(rgba, w, h);

    let mut hist: BTreeMap<u16, u32> = BTreeMap::new();
    for t in &ptiles {
        for c in t.iter().flatten() {
            *hist.entry(*c).or_default() += 1;
        }
    }
    let budget = 8 * CAP;
    let global: Option<Vec<u16>> = if hist.len() > budget {
        overflows.push(Overflow::Colors {
            unique: hist.len(),
            budget,
        });
        let hv: Vec<(u16, u32)> = hist.iter().map(|(&c, &n)| (c, n)).collect();
        Some(median_cut(&hv, budget))
    } else {
        None
    };
    let map_color = |c: u16| global.as_ref().map_or(c, |g| g[nearest(g, c)]);

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

    let mut cgram = Vec::new();
    for (pi, p) in fit.palettes.iter().enumerate() {
        for (ci, &c) in p.iter().enumerate() {
            cgram.push(((OBJ_CGRAM_BASE + pi * PAL_STRIDE + ci + 1) as u8, c));
        }
    }

    let report = BudgetReport {
        colors_used: fit.palettes.iter().map(|p| p.len()).sum(),
        palettes_used: fit.palettes.len(),
        tile_cells: cols * rows,
        unique_tiles,
        vram_words: char_words.len(),
        overflows,
    };
    ObjImport {
        char_words,
        cgram,
        cells,
        cols,
        rows,
        report,
    }
}

/// Write an `ObjImport` into real memory: char words at `char_base` (wrapping
/// VRAM) and the OBJ CGRAM palette entries (indices are already >= 128).
pub fn apply_obj_import(mem: &mut Memory, import: &ObjImport, char_base: u16) {
    for (i, &w) in import.char_words.iter().enumerate() {
        mem.vram[(char_base as usize + i) & 0x7fff] = w;
    }
    for &(idx, color) in &import.cgram {
        mem.cgram[idx as usize] = color;
    }
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
        let out = import_obj_sheet(&two_tile_rgba(), 16, 8);
        assert_eq!(out.cgram, vec![(129, 0x001f), (130, 0x7c00)]);
        assert_eq!(out.char_words.len(), 3 * 16);
        assert_eq!(&out.char_words[0..16], &[0u16; 16]);
        assert_eq!(out.char_words[16], 0x00ff);
        assert_eq!(out.char_words[32], 0x0ff0);
        assert_eq!(out.cols, 2);
        assert_eq!(out.rows, 1);
        assert_eq!(
            out.cells[0],
            ObjCell {
                tile: 1,
                pal: 0,
                flip_x: false,
                flip_y: false
            }
        );
        assert_eq!(
            out.cells[1],
            ObjCell {
                tile: 2,
                pal: 0,
                flip_x: false,
                flip_y: false
            }
        );
        assert_eq!(out.report.palettes_used, 1);
        assert_eq!(out.report.colors_used, 2);
        assert_eq!(out.report.unique_tiles, 2);
        assert!(out.report.overflows.is_empty());
    }

    #[test]
    fn apply_writes_char_words_and_obj_cgram() {
        let out = import_obj_sheet(&two_tile_rgba(), 16, 8);
        let mut mem = Memory::new();
        apply_obj_import(&mut mem, &out, 0x2000);
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
        let out = import_obj_sheet(&v, 16, 8);
        assert_eq!(out.report.unique_tiles, 1);
        assert_eq!(out.cells[0].tile, 1);
        assert_eq!(
            out.cells[1],
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
        let out = import_obj_sheet(&rgba, 16, 16);
        assert_eq!((out.cols, out.rows), (2, 2)); // 4 source cells
        assert_eq!(out.cells.len(), 4);
        // Every emitted tile is exactly 16 words (one 8x8 4bpp tile) — no size coupling.
        assert_eq!(out.char_words.len() % WORDS_PER_TILE, 0);
        // The import signature takes only (rgba, w, h): no size/size_sel parameter.
        let _f: fn(&[u8], u32, u32) -> ObjImport = import_obj_sheet;
    }

    #[test]
    fn fully_transparent_sheet_is_blank() {
        let out = import_obj_sheet(&vec![0u8; 8 * 8 * 4], 8, 8);
        assert!(out.cgram.is_empty());
        assert_eq!(out.report.unique_tiles, 0);
        assert_eq!(out.char_words.len(), 16);
        assert_eq!(
            out.cells[0],
            ObjCell {
                tile: 0,
                pal: 0,
                flip_x: false,
                flip_y: false
            }
        );
    }
}
