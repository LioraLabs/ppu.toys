//! Asset importers: PNG -> authentic VRAM/CGRAM/register data (m4/importer).
//! `quantize`/`tiles` are the shared primitives Mode-7 and OBJ import reuse;
//! this module's own surface is the tile-BG importer.

pub mod obj;
pub mod quantize;
pub mod tiles;

use std::collections::BTreeMap;

use serde::Serialize;

use self::quantize::{median_cut, nearest, region_fit};
use self::tiles::{pack_planar, split_tiles, IndexTile, TileSet};

/// Importer input format. `bit_depth` comes from the target layer's slot in
/// the mode table (`modes::mode_info`) — the BGMODE value itself is omitted
/// from the cache key on purpose: mode only affects import output via
/// bit-depth, so caching by bit-depth avoids spurious re-quantization.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ImportOptions {
    /// Target bits per pixel: 2, 4, or 8.
    pub bit_depth: u8,
    /// Tile edge in px. Only 8 is packed today; 16 falls back to 8 and is
    /// reported as `Overflow::TileSize16`.
    pub tile_size: u8,
    /// Tilemap base VRAM word address the caller will bind (register echo).
    pub map_base: u16,
    /// Char base VRAM word address; bounds the char budget.
    pub char_base: u16,
}

impl Default for ImportOptions {
    fn default() -> Self {
        ImportOptions {
            bit_depth: 4,
            tile_size: 8,
            map_base: 0x0000,
            char_base: 0x1000,
        }
    }
}

/// Register values the importer wants bound alongside its data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ImportRegisters {
    pub map_base: u16,
    pub char_base: u16,
    pub screen_size: u8,
    pub tile_size: u8,
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
    /// Unique tiles exceeded the char budget; excess map cells fall back to
    /// the blank tile.
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
    /// Deduped art tiles (excl. the reserved blank tile 0).
    pub unique_tiles: usize,
    /// Total VRAM words emitted (char + tilemap).
    pub vram_words: usize,
    pub overflows: Vec<Overflow>,
}

/// The importer's full output: everything `bg[n].source =` writes into real
/// memory, plus the budget report for the UI.
#[derive(Clone, Debug, PartialEq)]
pub struct TileBgImport {
    /// Bitplane-packed char data, tile 0 (reserved blank) first; write at
    /// `registers.char_base`.
    pub char_words: Vec<u16>,
    /// Screen-ordered tilemap (n 32x32 screens, 0x400 words each); write at
    /// `registers.map_base`.
    pub tilemap_words: Vec<u16>,
    /// (CGRAM index, BGR555) writes; transparent slots are not listed.
    pub cgram: Vec<(u8, u16)>,
    pub registers: ImportRegisters,
    pub report: BudgetReport,
}

/// Convert an RGBA image into authentic tile-BG data: split -> global color
/// budget -> multi-palette region fit -> nearest-remap -> flip-aware dedup ->
/// bitplane pack + screen-ordered tilemap + CGRAM writes + register echo.
/// Pure and deterministic: identical inputs yield identical outputs.
pub fn import_tile_bg(rgba: &[u8], width: u32, height: u32, opts: &ImportOptions) -> TileBgImport {
    let mut overflows = Vec::new();
    let (bpp, cap, pal_stride, palette_count) = match opts.bit_depth {
        2 => (2u8, 3usize, 4usize, 8usize),
        8 => (8u8, 255usize, 256usize, 1usize),
        _ => (4u8, 15usize, 16usize, 8usize),
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
    let (ptiles, cols, rows) = split_tiles(&cropped, w, h);

    // 3. global color budget: sub-palettes x cap colors
    let mut hist: BTreeMap<u16, u32> = BTreeMap::new();
    for t in &ptiles {
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
        Some(median_cut(&hv, budget))
    } else {
        None
    };
    let map_color = |c: u16| global.as_ref().map_or(c, |g| g[nearest(g, c)]);

    // 4. per-tile palettes (sorted unique; median-cut any single tile over cap)
    let tile_pals: Vec<Vec<u16>> = ptiles
        .iter()
        .map(|t| {
            let mut cs: Vec<u16> = t.iter().flatten().map(|&c| map_color(c)).collect();
            cs.sort_unstable();
            cs.dedup();
            if cs.len() > cap {
                let hv: Vec<(u16, u32)> = cs.iter().map(|&c| (c, 1)).collect();
                median_cut(&hv, cap)
            } else {
                cs
            }
        })
        .collect();

    // 5. region fit into the target sub-palette count
    let fit = region_fit(&tile_pals, palette_count, cap);
    if fit.palettes_needed > palette_count {
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

    // 6. index remap + flip-aware dedup; tile 0 reserved blank so padding and
    //    dropped cells are honestly transparent
    let words_per_tile = bpp as usize * 4;
    let max_tiles = ((0x8000 - opts.char_base as usize) / words_per_tile).clamp(1, 1024);
    let mut set = TileSet::new(true);
    set.insert([0u8; 64]);
    let mut cells: Vec<u16> = Vec::with_capacity(ptiles.len());
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

    // 9. CGRAM writes (sub-palette index 0 stays transparent/unwritten)
    let mut cgram = Vec::new();
    for (pi, p) in fit.palettes.iter().enumerate() {
        for (ci, &c) in p.iter().enumerate() {
            cgram.push(((pi * pal_stride + ci + 1) as u8, c));
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
    TileBgImport {
        char_words,
        tilemap_words,
        cgram,
        registers: ImportRegisters {
            map_base: opts.map_base,
            char_base: opts.char_base,
            screen_size,
            tile_size: 8,
        },
        report,
    }
}

/// Memoization key: `(asset slot, upload generation, options)`. The caller
/// (the `source =` wiring) bumps `generation` when a slot is re-uploaded;
/// options carry bit-depth/tile-size/bases (see `ImportOptions` for why the
/// BGMODE value itself is not in the key).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ImportKey {
    pub asset: String,
    pub generation: u64,
    pub options: ImportOptions,
}

/// Import memo: `source =` calls `get_or_import` per frame; the pipeline runs
/// only on a cold key, so re-quantization never happens at 60fps.
#[derive(Default)]
pub struct ImportCache {
    map: std::collections::HashMap<ImportKey, TileBgImport>,
}

impl ImportCache {
    /// Return the cached import for `key`, running the pipeline on a miss.
    pub fn get_or_import(
        &mut self,
        key: ImportKey,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) -> &TileBgImport {
        self.map
            .entry(key)
            .or_insert_with_key(|k| import_tile_bg(rgba, width, height, &k.options))
    }

    /// Drop every cached import of one asset slot (re-upload / slot delete).
    pub fn invalidate_asset(&mut self, asset: &str) {
        self.map.retain(|k, _| k.asset != asset);
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
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
        let out = import_tile_bg(&two_tile_rgba(), 16, 8, &ImportOptions::default());
        // palette 0 sorted: red 0x001f -> index 1, blue 0x7c00 -> index 2
        assert_eq!(out.cgram, vec![(1, 0x001f), (2, 0x7c00)]);
        // tile 0 = reserved blank, tile 1 = all-red, tile 2 = half/half
        assert_eq!(out.char_words.len(), 3 * 16);
        assert_eq!(&out.char_words[0..16], &[0u16; 16]); // blank
        assert_eq!(out.char_words[16], 0x00ff); // all index 1: plane0 full row
        assert_eq!(out.char_words[24], 0x0000); // planes 2/3 empty
        assert_eq!(out.char_words[32], 0x0ff0); // 1111 2222 row
                                                // tilemap: 32x32 screen, cells (0,0)=tile1 (1,0)=tile2, pal 0, rest blank
        assert_eq!(out.tilemap_words.len(), 0x400);
        assert_eq!(out.tilemap_words[0], 0x0001);
        assert_eq!(out.tilemap_words[1], 0x0002);
        assert!(out.tilemap_words[2..].iter().all(|&w| w == 0));
        // registers + report
        assert_eq!(
            (out.registers.map_base, out.registers.char_base),
            (0x0000, 0x1000)
        );
        assert_eq!((out.registers.screen_size, out.registers.tile_size), (0, 8));
        assert_eq!(out.report.palettes_used, 1);
        assert_eq!(out.report.colors_used, 2);
        assert_eq!(out.report.tile_cells, 2);
        assert_eq!(out.report.unique_tiles, 2);
        assert!(out.report.overflows.is_empty());
    }

    #[test]
    fn imports_2bpp_with_4_entry_palette_stride() {
        let opts = ImportOptions {
            bit_depth: 2,
            ..Default::default()
        };
        let out = import_tile_bg(&two_tile_rgba(), 16, 8, &opts);
        assert_eq!(out.cgram, vec![(1, 0x001f), (2, 0x7c00)]); // pal 0 base = 0
        assert_eq!(out.char_words.len(), 3 * 8); // 8 words/tile
        assert_eq!(out.char_words[8], 0x00ff);
        assert_eq!(out.char_words[16], 0x0ff0);
        assert_eq!(out.tilemap_words[1], 0x0002);
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
        let out = import_tile_bg(&v, 16, 8, &ImportOptions::default());
        assert_eq!(out.report.unique_tiles, 1);
        assert_eq!(out.tilemap_words[0], 0x0001);
        assert_eq!(out.tilemap_words[1], 0x0001 | 1 << 14); // H-flip bit
    }

    #[test]
    fn fully_transparent_image_yields_blank_everything() {
        let rgba = vec![0u8; 8 * 8 * 4];
        let out = import_tile_bg(&rgba, 8, 8, &ImportOptions::default());
        assert!(out.cgram.is_empty());
        assert_eq!(out.report.palettes_used, 0);
        assert_eq!(out.report.unique_tiles, 0);
        assert_eq!(out.char_words.len(), 16); // just the reserved blank tile
        assert!(out.tilemap_words.iter().all(|&w| w == 0));
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
        let mut opts = ImportOptions::default();
        let out = import_tile_bg(&v, 264, 8, &opts);
        assert_eq!(out.registers.screen_size, 1); // 64x32
        assert_eq!(out.tilemap_words.len(), 2 * 0x400);
        assert_eq!(out.tilemap_words[0], 0x0001); // red tile in SC0 cell 0
                                                  // column 32 lives in SC1 (words 0x400..)
        assert!(out.tilemap_words[0x400..].iter().all(|&w| w == 0));
        // and the same input imported at a different char_base changes capacity
        // bookkeeping but not content determinism
        opts.char_base = 0x2000;
        let out2 = import_tile_bg(&v, 264, 8, &opts);
        assert_eq!(out2.registers.char_base, 0x2000);
        assert_eq!(out2.char_words, out.char_words);
    }

    #[test]
    fn global_color_overflow_median_cuts_and_reports() {
        // 16x16: 256 pixels, 200 distinct colors > 120 budget
        let mut v = Vec::new();
        for i in 0..256u32 {
            let c = i % 200;
            v.extend_from_slice(&[(c % 32 * 8) as u8, (c / 32 * 8 + 8) as u8, 128, 255]);
        }
        let out = import_tile_bg(&v, 16, 16, &ImportOptions::default());
        assert!(out.report.colors_used <= 120);
        assert!(out
            .report
            .overflows
            .iter()
            .any(|o| matches!(o, Overflow::Colors { budget: 120, .. })));
    }

    #[test]
    fn cache_memoizes_by_key_and_invalidates_per_asset() {
        let rgba = two_tile_rgba();
        let mut cache = ImportCache::default();
        let key = ImportKey {
            asset: "sky".into(),
            generation: 1,
            options: ImportOptions::default(),
        };
        let a = cache.get_or_import(key.clone(), &rgba, 16, 8).clone();
        assert_eq!(cache.len(), 1);
        let b = cache.get_or_import(key.clone(), &rgba, 16, 8).clone();
        assert_eq!(cache.len(), 1); // hit, no re-quantize entry
        assert_eq!(a, b);
        // different bit-depth = different key
        let key2 = ImportKey {
            options: ImportOptions {
                bit_depth: 2,
                ..Default::default()
            },
            ..key.clone()
        };
        cache.get_or_import(key2, &rgba, 16, 8);
        assert_eq!(cache.len(), 2);
        // re-upload bumps generation; invalidate drops all entries for the slot
        cache.invalidate_asset("sky");
        assert_eq!(cache.len(), 0);
    }
}
