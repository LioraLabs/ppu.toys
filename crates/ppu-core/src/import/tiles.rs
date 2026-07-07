//! Tile-grid primitives shared by the importers (tile-BG here; Mode 7 and OBJ
//! import reuse them): 8x8 split with BGR555 quantization and transparency,
//! flip helpers, flip-aware dedup, authentic bitplane packing.

use crate::memory::rgb15;

/// One 8x8 tile of BGR555 pixels; `None` = transparent (source alpha < 128).
pub type PixelTile = [Option<u16>; 64];

/// An 8x8 grid of sub-palette indices (0 = transparent).
pub type IndexTile = [u8; 64];

/// Split RGBA into row-major 8x8 [`PixelTile`]s, quantizing to BGR555 and
/// padding the right/bottom edges with transparent. Returns (tiles, cols, rows).
pub fn split_tiles(rgba: &[u8], width: usize, height: usize) -> (Vec<PixelTile>, usize, usize) {
    let cols = width.div_ceil(8);
    let rows = height.div_ceil(8);
    let mut tiles = Vec::with_capacity(cols * rows);
    for ty in 0..rows {
        for tx in 0..cols {
            let mut t: PixelTile = [None; 64];
            for py in 0..8 {
                for px in 0..8 {
                    let (x, y) = (tx * 8 + px, ty * 8 + py);
                    if x >= width || y >= height {
                        continue;
                    }
                    let i = (y * width + x) * 4;
                    if rgba[i + 3] >= 128 {
                        t[py * 8 + px] = Some(rgb15(rgba[i], rgba[i + 1], rgba[i + 2]));
                    }
                }
            }
            tiles.push(t);
        }
    }
    (tiles, cols, rows)
}

/// Mirror an index tile left-right.
pub fn flip_h(t: &IndexTile) -> IndexTile {
    std::array::from_fn(|i| t[(i / 8) * 8 + (7 - i % 8)])
}

/// Mirror an index tile top-bottom.
pub fn flip_v(t: &IndexTile) -> IndexTile {
    std::array::from_fn(|i| t[(7 - i / 8) * 8 + i % 8])
}

/// Bitplane-pack one [`IndexTile`] into authentic SNES char words.
/// 2bpp: 8 words, word[r] = plane0(r) | plane1(r)<<8.
/// 4bpp: 16 words, rows 0..8 planes 0/1 then rows 0..8 planes 2/3.
/// Leftmost pixel = bit 7 of each plane byte.
pub fn pack_planar(t: &IndexTile, bpp: u8) -> Vec<u16> {
    let plane_byte = |plane: u8, row: usize| -> u16 {
        (0..8).fold(0u16, |acc, x| {
            acc | ((((t[row * 8 + x] >> plane) & 1) as u16) << (7 - x))
        })
    };
    let word = |lo: u8, hi: u8, row: usize| plane_byte(lo, row) | (plane_byte(hi, row) << 8);
    let mut out: Vec<u16> = (0..8).map(|r| word(0, 1, r)).collect();
    if bpp == 4 {
        out.extend((0..8).map(|r| word(2, 3, r)));
    }
    out
}

/// Flip-aware tile dedup: identical tiles — including H/V/HV-mirrored
/// duplicates when `allow_flips` — collapse to one stored char tile.
/// `allow_flips = false` is for tilemaps without flip bits (Mode 7).
/// Deterministic: the map is only ever probed by key, never iterated.
pub struct TileSet {
    tiles: Vec<IndexTile>,
    index: std::collections::HashMap<IndexTile, u16>,
    allow_flips: bool,
}

impl TileSet {
    pub fn new(allow_flips: bool) -> Self {
        TileSet {
            tiles: Vec::new(),
            index: std::collections::HashMap::new(),
            allow_flips,
        }
    }

    /// Insert a tile, returning `(tile#, h_flip, v_flip)` — the stored tile
    /// plus the flip bits that reproduce the input from it.
    pub fn insert(&mut self, t: IndexTile) -> (u16, bool, bool) {
        if let Some(&n) = self.index.get(&t) {
            return (n, false, false);
        }
        if self.allow_flips {
            let h = flip_h(&t);
            if let Some(&n) = self.index.get(&h) {
                return (n, true, false);
            }
            let v = flip_v(&t);
            if let Some(&n) = self.index.get(&v) {
                return (n, false, true);
            }
            if let Some(&n) = self.index.get(&flip_h(&v)) {
                return (n, true, true);
            }
        }
        let n = self.tiles.len() as u16;
        self.tiles.push(t);
        self.index.insert(t, n);
        (n, false, false)
    }

    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    /// Stored unique tiles in insertion order (tile# = slice index).
    pub fn tiles(&self) -> &[IndexTile] {
        &self.tiles
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_quantizes_pads_and_handles_alpha() {
        // 9x1 image: 8 red pixels + 1 blue -> 2 tiles (padded)
        let mut rgba = Vec::new();
        for _ in 0..8 {
            rgba.extend_from_slice(&[255, 0, 0, 255]);
        }
        rgba.extend_from_slice(&[0, 0, 255, 255]);
        let (tiles, cols, rows) = split_tiles(&rgba, 9, 1);
        assert_eq!((cols, rows, tiles.len()), (2, 1, 2));
        assert_eq!(tiles[0][0], Some(0x001f)); // red, BGR555
        assert_eq!(tiles[0][8], None); // padded row below
        assert_eq!(tiles[1][0], Some(0x7c00)); // blue
        assert_eq!(tiles[1][1], None); // padded right
    }

    #[test]
    fn split_treats_low_alpha_as_transparent() {
        let rgba = [10u8, 20, 30, 127]; // alpha < 128
        let (tiles, ..) = split_tiles(&rgba, 1, 1);
        assert_eq!(tiles[0][0], None);
    }

    #[test]
    fn flips_are_involutions_and_move_the_corner() {
        let mut t: IndexTile = [0; 64];
        t[0] = 7; // top-left
        assert_eq!(flip_h(&t)[7], 7);
        assert_eq!(flip_v(&t)[56], 7);
        assert_eq!(flip_h(&flip_h(&t)), t);
        assert_eq!(flip_v(&flip_v(&t)), t);
    }

    #[test]
    fn pack_planar_2bpp_and_4bpp_bit_layout() {
        // row 0: cols 0-3 index 1, cols 4-7 index 2; other rows 0
        let mut t: IndexTile = [0; 64];
        for x in 0..4 {
            t[x] = 1;
        }
        for x in 4..8 {
            t[x] = 2;
        }
        let w2 = pack_planar(&t, 2);
        assert_eq!(w2.len(), 8);
        assert_eq!(w2[0], 0x0ff0); // p0=0b11110000, p1=0b00001111
        assert_eq!(w2[1], 0x0000);
        let w4 = pack_planar(&t, 4);
        assert_eq!(w4.len(), 16);
        assert_eq!(w4[0], 0x0ff0); // planes 0/1
        assert_eq!(w4[8], 0x0000); // planes 2/3 empty
                                   // index 15 pixel exercises planes 2/3
        let mut t2: IndexTile = [0; 64];
        t2[0] = 15;
        let w = pack_planar(&t2, 4);
        assert_eq!(w[0], 0x8080); // p0 bit7 | p1 bit7 << 8
        assert_eq!(w[8], 0x8080); // p2 bit7 | p3 bit7 << 8
    }

    #[test]
    fn tileset_dedups_identity_and_flips_deterministically() {
        let mut s = TileSet::new(true);
        let mut t: IndexTile = [0; 64];
        t[0] = 1; // asymmetric
        assert_eq!(s.insert(t), (0, false, false));
        assert_eq!(s.insert(t), (0, false, false)); // identical
        assert_eq!(s.insert(flip_h(&t)), (0, true, false));
        assert_eq!(s.insert(flip_v(&t)), (0, false, true));
        assert_eq!(s.insert(flip_h(&flip_v(&t))), (0, true, true));
        assert_eq!(s.len(), 1);
        let mut u: IndexTile = [0; 64];
        u[1] = 2; // genuinely new
        assert_eq!(s.insert(u), (1, false, false));
        assert_eq!(s.len(), 2);
        assert_eq!(s.tiles()[0], t); // storage order = insertion order
    }

    #[test]
    fn tileset_without_flips_stores_mirrors_separately() {
        let mut s = TileSet::new(false); // Mode 7 has no flip bits
        let mut t: IndexTile = [0; 64];
        t[0] = 1;
        assert_eq!(s.insert(t), (0, false, false));
        assert_eq!(s.insert(flip_h(&t)), (1, false, false));
        assert_eq!(s.len(), 2);
    }
}
