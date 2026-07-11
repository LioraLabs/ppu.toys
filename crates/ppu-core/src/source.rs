//! Source payloads (M10 · Sources): versioned, position-independent,
//! self-describing binary blobs a graphics source commits to at authoring
//! time. The payload holds RENDER DATA ONLY (palette colors + packed tiles +
//! tilemap); dims/budget/obj-cells travel alongside in `SourceMeta`.
//! Placement (VRAM/CGRAM bases) stays a bind-time concern — see `place_*`.
//!
//! Byte layout v1 (little-endian):
//!
//! ```text
//! common:  u8 version=1 | u8 kind (0=bg 1=m7 2=obj)
//! bg:      u8 bit_depth (2|4|8) | u8 tile_size (8)
//!          u8 pal_count, per palette: u8 len + len*u16 BGR555
//!          u16 tile_count, tile_count*(bit_depth*4)*u16 char words (bitplane-packed, tile 0 blank)
//!          u8 screen_size (0..=3), then n_screens(screen_size)*0x400 u16 tilemap words
//! m7:      u8 opts_len (0 in v1) + opts_len bytes   <- extensible M7Options block
//!          u8 pal_len + pal_len*u16 BGR555 (flat, CGRAM 0 implicit transparent)
//!          u16 tile_count (<=256), tile_count*64 chunky 8bpp bytes
//!          u8 tiles_w (<=128) | u8 tiles_h (<=128), tiles_w*tiles_h map bytes
//! obj:     u8 cell_size (8|16|32|64)
//!          u8 pal_count, per palette: u8 len + len*u16 BGR555
//!          u16 tile_count, tile_count*16 u16 char words (4bpp fixed)
//! ```
//!
//! Tilemap/map tile numbers are relative to the payload's own char block;
//! palettes are color lists (sub-palette entry 0 transparent, implicit).
//! Decode is strict: unknown version/kind/params reject, trailing bytes reject.

use crate::memory::Memory;

pub const PAYLOAD_VERSION: u8 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    Bg,
    M7,
    Obj,
}

/// Extensible Mode 7 format options. v1 is empty; encoded as a length-prefixed
/// block so a future EXTBG variant (7bpp color + 1bpp priority in bit 7) only
/// appends option bytes + bumps PAYLOAD_VERSION — the shape never breaks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct M7Options {}

#[derive(Clone, Debug, PartialEq)]
pub struct BgSource {
    pub bit_depth: u8,
    pub tile_size: u8,
    /// Sub-palettes as BGR555 color lists; sub-palette entry 0 (transparent) implicit.
    pub palettes: Vec<Vec<u16>>,
    /// Bitplane-packed char words, tile 0 = reserved blank, bit_depth*4 words/tile.
    pub char_words: Vec<u16>,
    pub screen_size: u8,
    /// Screen-ordered tilemap; tile numbers relative to `char_words`, pal fields 0-based.
    pub tilemap_words: Vec<u16>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct M7Source {
    pub options: M7Options,
    /// Flat palette (<=255 colors); CGRAM index 0 stays transparent.
    pub palette: Vec<u16>,
    /// Chunky 8bpp tiles (<=256), 64 bytes each.
    pub tiles: Vec<[u8; 64]>,
    pub tiles_w: u8,
    pub tiles_h: u8,
    /// tiles_w*tiles_h tile-number bytes, relative to `tiles`.
    pub map: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ObjSource {
    pub cell_size: u8,
    pub palettes: Vec<Vec<u16>>,
    /// 4bpp char words, 16/tile, OBJ name-table order.
    pub char_words: Vec<u16>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SourcePayload {
    Bg(BgSource),
    M7(M7Source),
    Obj(ObjSource),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PayloadError {
    Truncated,
    BadVersion(u8),
    BadKind(u8),
    BadParam(&'static str),
    TrailingBytes,
}

impl std::fmt::Display for PayloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PayloadError::Truncated => write!(f, "payload truncated"),
            PayloadError::BadVersion(v) => write!(f, "unsupported payload version {v}"),
            PayloadError::BadKind(k) => write!(f, "unknown source kind {k}"),
            PayloadError::BadParam(p) => write!(f, "invalid payload field: {p}"),
            PayloadError::TrailingBytes => write!(f, "trailing bytes after payload"),
        }
    }
}

/// Tilemap length in words for a BGnSC screen-size field.
pub fn bg_tilemap_len(screen_size: u8) -> usize {
    match screen_size {
        0 => 0x400,
        1 | 2 => 0x800,
        _ => 0x1000,
    }
}

fn push_u16(b: &mut Vec<u8>, v: u16) {
    b.extend_from_slice(&v.to_le_bytes());
}

fn push_palettes(b: &mut Vec<u8>, pals: &[Vec<u16>]) {
    b.push(pals.len() as u8);
    for p in pals {
        b.push(p.len() as u8);
        for &c in p {
            push_u16(b, c);
        }
    }
}

struct Rd<'a> {
    b: &'a [u8],
    i: usize,
}
impl<'a> Rd<'a> {
    fn u8(&mut self) -> Result<u8, PayloadError> {
        let v = *self.b.get(self.i).ok_or(PayloadError::Truncated)?;
        self.i += 1;
        Ok(v)
    }
    fn u16(&mut self) -> Result<u16, PayloadError> {
        let s = self
            .b
            .get(self.i..self.i + 2)
            .ok_or(PayloadError::Truncated)?;
        self.i += 2;
        Ok(u16::from_le_bytes([s[0], s[1]]))
    }
    fn u16s(&mut self, n: usize) -> Result<Vec<u16>, PayloadError> {
        (0..n).map(|_| self.u16()).collect()
    }
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], PayloadError> {
        let s = self
            .b
            .get(self.i..self.i + n)
            .ok_or(PayloadError::Truncated)?;
        self.i += n;
        Ok(s)
    }
    fn palettes(&mut self) -> Result<Vec<Vec<u16>>, PayloadError> {
        let n = self.u8()? as usize;
        (0..n)
            .map(|_| {
                let len = self.u8()? as usize;
                self.u16s(len)
            })
            .collect()
    }
    fn done(&self) -> Result<(), PayloadError> {
        if self.i == self.b.len() {
            Ok(())
        } else {
            Err(PayloadError::TrailingBytes)
        }
    }
}

impl SourcePayload {
    pub fn kind(&self) -> SourceKind {
        match self {
            SourcePayload::Bg(_) => SourceKind::Bg,
            SourcePayload::M7(_) => SourceKind::M7,
            SourcePayload::Obj(_) => SourceKind::Obj,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut b = vec![PAYLOAD_VERSION];
        match self {
            SourcePayload::Bg(s) => {
                b.push(0);
                b.push(s.bit_depth);
                b.push(s.tile_size);
                push_palettes(&mut b, &s.palettes);
                let wpt = s.bit_depth as usize * 4;
                push_u16(&mut b, (s.char_words.len() / wpt) as u16);
                for &w in &s.char_words {
                    push_u16(&mut b, w);
                }
                b.push(s.screen_size);
                for &w in &s.tilemap_words {
                    push_u16(&mut b, w);
                }
            }
            SourcePayload::M7(s) => {
                b.push(1);
                b.push(0); // opts_len: M7Options is empty in v1 (EXTBG room)
                b.push(s.palette.len() as u8);
                for &c in &s.palette {
                    push_u16(&mut b, c);
                }
                push_u16(&mut b, s.tiles.len() as u16);
                for t in &s.tiles {
                    b.extend_from_slice(t);
                }
                b.push(s.tiles_w);
                b.push(s.tiles_h);
                b.extend_from_slice(&s.map);
            }
            SourcePayload::Obj(s) => {
                b.push(2);
                b.push(s.cell_size);
                push_palettes(&mut b, &s.palettes);
                push_u16(&mut b, (s.char_words.len() / 16) as u16);
                for &w in &s.char_words {
                    push_u16(&mut b, w);
                }
            }
        }
        b
    }

    pub fn decode(bytes: &[u8]) -> Result<SourcePayload, PayloadError> {
        let mut r = Rd { b: bytes, i: 0 };
        let version = r.u8()?;
        if version != PAYLOAD_VERSION {
            return Err(PayloadError::BadVersion(version));
        }
        let kind = r.u8()?;
        let out = match kind {
            0 => {
                let bit_depth = r.u8()?;
                if !matches!(bit_depth, 2 | 4 | 8) {
                    return Err(PayloadError::BadParam("bit_depth"));
                }
                let tile_size = r.u8()?;
                if tile_size != 8 {
                    return Err(PayloadError::BadParam("tile_size"));
                }
                let palettes = r.palettes()?;
                let tile_count = r.u16()? as usize;
                let char_words = r.u16s(tile_count * bit_depth as usize * 4)?;
                let screen_size = r.u8()?;
                if screen_size > 3 {
                    return Err(PayloadError::BadParam("screen_size"));
                }
                let tilemap_words = r.u16s(bg_tilemap_len(screen_size))?;
                SourcePayload::Bg(BgSource {
                    bit_depth,
                    tile_size,
                    palettes,
                    char_words,
                    screen_size,
                    tilemap_words,
                })
            }
            1 => {
                // v1 options block must be empty; a future EXTBG bumps the
                // version byte, so nonzero here is an unknown-format error.
                let opts_len = r.u8()?;
                if opts_len != 0 {
                    return Err(PayloadError::BadParam("m7_options"));
                }
                let pal_len = r.u8()? as usize;
                let palette = r.u16s(pal_len)?;
                let tile_count = r.u16()? as usize;
                if tile_count > 256 {
                    return Err(PayloadError::BadParam("tile_count"));
                }
                let mut tiles = Vec::with_capacity(tile_count);
                for _ in 0..tile_count {
                    let s = r.bytes(64)?;
                    let mut t = [0u8; 64];
                    t.copy_from_slice(s);
                    tiles.push(t);
                }
                let tiles_w = r.u8()?;
                let tiles_h = r.u8()?;
                if tiles_w > 128 || tiles_h > 128 {
                    return Err(PayloadError::BadParam("map_dims"));
                }
                let map = r.bytes(tiles_w as usize * tiles_h as usize)?.to_vec();
                SourcePayload::M7(M7Source {
                    options: M7Options::default(),
                    palette,
                    tiles,
                    tiles_w,
                    tiles_h,
                    map,
                })
            }
            2 => {
                let cell_size = r.u8()?;
                if !matches!(cell_size, 8 | 16 | 32 | 64) {
                    return Err(PayloadError::BadParam("cell_size"));
                }
                let palettes = r.palettes()?;
                let tile_count = r.u16()? as usize;
                let char_words = r.u16s(tile_count * 16)?;
                SourcePayload::Obj(ObjSource {
                    cell_size,
                    palettes,
                    char_words,
                })
            }
            k => return Err(PayloadError::BadKind(k)),
        };
        r.done()?;
        Ok(out)
    }
}

/// Write a BG source at bind-time bases. `cgram_base` is the CGRAM index the
/// palette block starts at (the mode-0 per-layer band, else 0); sub-palette
/// entry 0 stays unwritten (transparent).
pub fn place_bg(
    src: &BgSource,
    mem: &mut Memory,
    map_base: u16,
    char_base: u16,
    cgram_base: usize,
) {
    for (o, &w) in src.char_words.iter().enumerate() {
        mem.vram[(char_base as usize + o) & 0x7fff] = w;
    }
    for (o, &w) in src.tilemap_words.iter().enumerate() {
        mem.vram[(map_base as usize + o) & 0x7fff] = w;
    }
    let stride = match src.bit_depth {
        2 => 4,
        8 => 256,
        _ => 16,
    };
    for (pi, p) in src.palettes.iter().enumerate() {
        for (ci, &c) in p.iter().enumerate() {
            mem.cgram[(cgram_base + pi * stride + ci + 1) & 0xff] = c;
        }
    }
}

/// Write a Mode 7 source into the byte-interleaved region (words 0..0x4000):
/// char bytes in the high lane, the 128-wide map in the low lane, palette at
/// CGRAM 1.. . Masked-lane writes assume the frame's zeroed-VRAM bootstrap
/// (frame() zeroes VRAM/CGRAM before imports), composing like the m7 pokes.
pub fn place_m7(src: &M7Source, mem: &mut Memory) {
    for (t, tile) in src.tiles.iter().enumerate() {
        for (j, &px) in tile.iter().enumerate() {
            let i = t * 64 + j;
            mem.vram[i] = (mem.vram[i] & 0x00ff) | ((px as u16) << 8);
        }
    }
    let tw = src.tiles_w as usize;
    for ty in 0..src.tiles_h as usize {
        for tx in 0..tw {
            let i = ty * 128 + tx;
            mem.vram[i] = (mem.vram[i] & 0xff00) | src.map[ty * tw + tx] as u16;
        }
    }
    for (i, &c) in src.palette.iter().enumerate() {
        mem.cgram[(i + 1) & 0xff] = c;
    }
}

/// Write an OBJ source: char words at `char_base` (wrapping VRAM), palettes
/// into the OBJ CGRAM half (128..).
pub fn place_obj(src: &ObjSource, mem: &mut Memory, char_base: u16) {
    for (i, &w) in src.char_words.iter().enumerate() {
        mem.vram[(char_base as usize + i) & 0x7fff] = w;
    }
    for (pi, p) in src.palettes.iter().enumerate() {
        for (ci, &c) in p.iter().enumerate() {
            mem.cgram[(128 + pi * 16 + ci + 1) & 0xff] = c;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_bg() -> BgSource {
        BgSource {
            bit_depth: 2,
            tile_size: 8,
            palettes: vec![vec![0x001f, 0x7c00], vec![0x03e0]],
            char_words: vec![0xbeefu16; 2 * 8], // 2 tiles at 2bpp
            screen_size: 0,
            tilemap_words: vec![0x0001u16; 0x400],
        }
    }

    fn sample_m7() -> M7Source {
        M7Source {
            options: M7Options::default(),
            palette: vec![0x001f, 0x7fff],
            tiles: vec![[7u8; 64], [3u8; 64]],
            tiles_w: 2,
            tiles_h: 1,
            map: vec![0, 1],
        }
    }

    fn sample_obj() -> ObjSource {
        ObjSource {
            cell_size: 8,
            palettes: vec![vec![0x001f]],
            char_words: vec![0x00ffu16; 2 * 16],
        }
    }

    #[test]
    fn encode_decode_roundtrips_all_kinds() {
        for p in [
            SourcePayload::Bg(sample_bg()),
            SourcePayload::M7(sample_m7()),
            SourcePayload::Obj(sample_obj()),
        ] {
            assert_eq!(SourcePayload::decode(&p.encode()).unwrap(), p);
        }
    }

    #[test]
    fn payload_self_describes_version_and_kind() {
        assert_eq!(SourcePayload::Bg(sample_bg()).encode()[..2], [1, 0]);
        assert_eq!(SourcePayload::M7(sample_m7()).encode()[..2], [1, 1]);
        assert_eq!(SourcePayload::Obj(sample_obj()).encode()[..2], [1, 2]);
    }

    #[test]
    fn bg_byte_layout_is_locked() {
        let s = BgSource {
            bit_depth: 2,
            tile_size: 8,
            palettes: vec![vec![0x001f]],
            char_words: vec![0u16; 8],
            screen_size: 0,
            tilemap_words: vec![0u16; 0x400],
        };
        let b = SourcePayload::Bg(s).encode();
        // version, kind, bit_depth, tile_size, pal_count, pal0 len, color lo, hi
        assert_eq!(&b[..8], &[1, 0, 2, 8, 1, 1, 0x1f, 0x00]);
        assert_eq!(&b[8..10], &[1, 0]); // u16 LE tile_count = 1
        assert_eq!(b[10 + 16], 0); // screen_size byte after 8 char words
        assert_eq!(b.len(), 10 + 16 + 1 + 0x400 * 2);
    }

    #[test]
    fn m7_options_block_is_length_prefixed_extbg_room() {
        // Forward-compat assertion (spec): the byte after the m7 kind is the
        // options length — a future EXTBG variant appends option bytes there
        // and bumps PAYLOAD_VERSION, no format break. v1 writes 0.
        let b = SourcePayload::M7(sample_m7()).encode();
        assert_eq!(b[2], 0);
        // and a v1 decoder honestly rejects a nonzero options block
        let mut evil = b.clone();
        evil[2] = 1;
        evil.insert(3, 0xff);
        assert_eq!(
            SourcePayload::decode(&evil),
            Err(PayloadError::BadParam("m7_options"))
        );
    }

    #[test]
    fn decode_rejects_garbage() {
        assert_eq!(SourcePayload::decode(&[]), Err(PayloadError::Truncated));
        assert_eq!(
            SourcePayload::decode(&[2, 0]),
            Err(PayloadError::BadVersion(2))
        );
        assert_eq!(
            SourcePayload::decode(&[1, 9]),
            Err(PayloadError::BadKind(9))
        );
        assert_eq!(SourcePayload::decode(&[1]), Err(PayloadError::Truncated));
        let mut b = SourcePayload::Obj(sample_obj()).encode();
        b.truncate(b.len() - 1);
        assert_eq!(SourcePayload::decode(&b), Err(PayloadError::Truncated));
        let mut b2 = SourcePayload::Obj(sample_obj()).encode();
        b2.push(0);
        assert_eq!(SourcePayload::decode(&b2), Err(PayloadError::TrailingBytes));
        assert_eq!(
            SourcePayload::decode(&[1, 0, 3, 8]),
            Err(PayloadError::BadParam("bit_depth"))
        );
        assert_eq!(
            SourcePayload::decode(&[1, 2, 9]),
            Err(PayloadError::BadParam("cell_size"))
        );
    }

    #[test]
    fn place_bg_writes_char_map_and_banded_palette() {
        let s = sample_bg();
        let mut mem = Memory::new();
        place_bg(&s, &mut mem, 0x0000, 0x1000, 0);
        assert_eq!(mem.vram[0x1000], 0xbeef);
        assert_eq!(mem.vram[0x0000], 0x0001);
        assert_eq!(mem.cgram[1], 0x001f); // pal 0 entry 1 (stride 4 at 2bpp)
        assert_eq!(mem.cgram[2], 0x7c00);
        assert_eq!(mem.cgram[5], 0x03e0); // pal 1 entry 1 = 1*4+0+1
        assert_eq!(mem.cgram[0], 0); // transparent slot untouched
                                     // mode-0 band shifts the whole block
        let mut mem2 = Memory::new();
        place_bg(&s, &mut mem2, 0x0000, 0x1000, 32);
        assert_eq!(mem2.cgram[33], 0x001f);
    }

    #[test]
    fn place_m7_interleaves_lanes_and_flat_palette() {
        let s = sample_m7();
        let mut mem = Memory::new();
        place_m7(&s, &mut mem);
        assert_eq!(mem.vram[0], (7 << 8) | 0); // char high lane | map cell (0,0)=tile 0
        assert_eq!(mem.vram[1], (7 << 8) | 1); // map cell (1,0)=tile 1
        assert_eq!(mem.vram[64], 3 << 8); // tile 1 char bytes
        assert_eq!(mem.cgram[1], 0x001f);
        assert_eq!(mem.cgram[2], 0x7fff);
        assert_eq!(mem.cgram[0], 0);
    }

    #[test]
    fn place_obj_writes_char_at_base_and_obj_palettes() {
        let s = sample_obj();
        let mut mem = Memory::new();
        place_obj(&s, &mut mem, 0x2000);
        assert_eq!(mem.vram[0x2000], 0x00ff);
        assert_eq!(mem.cgram[129], 0x001f);
        assert_eq!(mem.cgram[1], 0); // BG half untouched
    }
}
