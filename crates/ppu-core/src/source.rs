//! Source payloads (M10 · Sources): versioned, position-independent,
//! self-describing binary blobs a graphics source commits to at authoring
//! time. The payload holds RENDER DATA ONLY (palette colors + packed tiles +
//! tilemap); dims/budget/obj-cells travel alongside in `SourceMeta`.
//! Placement (VRAM/CGRAM bases) stays a placement-time concern (`dma()`) — see `place_*`.
//!
//! Byte layout v1 (little-endian):
//!
//! ```text
//! common:  u8 version=4 | u8 kind (0=bg 1=m7 2=obj 3=sheet 4=sample 5=song 6=palette)
//! bg:      u8 bit_depth (2|4|8) | u8 tile_size (8)
//!          u8 pal_count, per palette: u8 len + len*u16 BGR555
//!          u16 tile_count, tile_count*(bit_depth*4)*u16 char words (bitplane-packed, tile 0 blank)
//!          u8 screen_size (0..=3), then n_screens(screen_size)*0x400 u16 tilemap words
//! m7:      u8 opts_len (1 in v2, 3 when the image overflowed 256 tiles) + u8 extbg
//!          [+ u16 distinct tiles in the image, only when over 256]
//!          u8 pal_len + pal_len*u16 BGR555 (flat, CGRAM 0 implicit transparent)
//!          u16 tile_count (<=256), tile_count*64 chunky 8bpp bytes
//!          u8 tiles_w (<=128) | u8 tiles_h (<=128), tiles_w*tiles_h map bytes
//! obj:     u8 cell_size (8|16|32|64), or 0 then u8 cell width, u8 cell height (multiples of 8 up to 128)
//!          u8 pal_count, per palette: u8 len + len*u16 BGR555
//!          u16 tile_count, tile_count*16 u16 char words (4bpp fixed)
//!          u16 cell_count, cell_count*(u16 tile|u8 pal|u8 flip flags)
//! sheet:   u8 bit_depth (2|4|8)                     <- tile size is fixed 8x8
//!          u8 pal_count, per palette: u8 len + len*u16 BGR555
//!          u16 tile_count, tile_count*(bit_depth*4)*u16 char words (sheet order,
//!          NO reserved blank tile). No screen_size, no tilemap: map geometry is
//!          the author's, not the payload's.
//!          v4: u8 has_pals (0|1), then (if 1) tile_count u8 palette rows, char k's
//!          palette (each below pal_count): what a Tiled map drawn from it colours by.
//! sample:  u8 has_loop (0|1), [u16 loop_block, only if has_loop]
//!          u16 block_count (1..=MAX_SAMPLE_BLOCKS), block_count*9 raw BRR
//!          bytes (9 bytes per 16-sample block). PCM, not an image: no
//!          palette, no tiles.
//! song:    PSNG bytes (see `crate::song`), to the end of the payload,
//!          UNDECODED — a bad song is a setup error at `score{}` time, not
//!          an add_source error, so this kind's own decode never parses
//!          them (an empty body is accepted here).
//! palette: u8 row_size (4|16) | u8 row_count (1..=256/row_size), row_count*row_size
//!          u16 BGR555: whole rows with entry 0 of each stored (placement never writes
//!          it: it is the transparent colour); padding repeats the row's colour 0.
//!          Or the 256-colour block, unpadded: u8 0 | u8 count-1 (1..=256 colours,
//!          entry 0 included), count*u16 BGR555; placement writes and claims only
//!          entries 1..count, like an m7 source's own palette.
//! ```
//!
//! Tilemap/map tile numbers are relative to the payload's own char block;
//! palettes are color lists (sub-palette entry 0 transparent, implicit).
//! Decode is strict: unknown version/kind/params reject, trailing bytes reject.

use crate::import::obj::ObjCell;
use crate::import::BudgetReport;
use crate::import_m7::Mode7ImportReport;
use crate::memory::Memory;
use serde::{Deserialize, Serialize};

pub const PAYLOAD_VERSION: u8 = 4;

/// Max BRR blocks a sample payload can carry: 64 KB of sound RAM / 9-byte
/// BRR blocks.
pub const MAX_SAMPLE_BLOCKS: usize = 0x10000 / 9;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    Bg,
    M7,
    Obj,
    Sheet,
    Sample,
    /// PSNG song bytes — see `crate::song`. Not a `convertSource` kind (a
    /// song is authored data, not an image to quantize), so it has no
    /// `FromStr` string and no `convert_source` conversion path.
    Song,
    /// Colours only, from a `.gpl` (see `crate::gpl`): `dma("name", { pal })` writes
    /// them, `dma(art, { palette = "name" })` writes them in place of the art's own.
    Palette,
}

impl SourceKind {
    /// The kind as a user names it, for messages.
    pub fn name(self) -> &'static str {
        match self {
            SourceKind::Bg => "bg",
            SourceKind::M7 => "m7",
            SourceKind::Obj => "obj",
            SourceKind::Sheet => "sheet",
            SourceKind::Sample => "sample",
            SourceKind::Song => "song",
            SourceKind::Palette => "palette",
        }
    }
}

/// The kind strings the JS surface names a source by (`convertSource`).
impl std::str::FromStr for SourceKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "bg" => Ok(SourceKind::Bg),
            "m7" => Ok(SourceKind::M7),
            "obj" => Ok(SourceKind::Obj),
            "sheet" => Ok(SourceKind::Sheet),
            "sample" => Ok(SourceKind::Sample),
            "palette" => Ok(SourceKind::Palette),
            other => Err(format!("unknown source kind '{other}'")),
        }
    }
}

/// Mode 7 payload options, encoded as a length-prefixed block.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct M7Options {
    pub extbg: bool,
    /// Distinct 8x8 tiles the image had when more than the 256 kept (their cells fell back to
    /// tile 0); 0 when every tile fit. Carried so a Tiled tileset can be refused with the count.
    pub overflowed_tiles: u16,
}

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
    /// The cell's width and height in pixels.
    pub cell_size: u8,
    pub cell_height: u8,
    pub palettes: Vec<Vec<u16>>,
    /// 4bpp char words, 16/tile, OBJ name-table order.
    pub char_words: Vec<u16>,
    /// Source cells in row-major order, retained so Lua can address imported
    /// animation frames without redoing the importer's dedup/palette mapping.
    pub cells: Vec<ObjCell>,
}

/// A tilesheet: chars in row-major sheet order with NO reserved blank tile and
/// no tilemap. Char N is the Nth 8x8 cell of the source PNG; map geometry is
/// the author's (`bg[n].map`), not the payload's.
#[derive(Clone, Debug, PartialEq)]
pub struct SheetSource {
    pub bit_depth: u8,
    /// Sub-palettes as BGR555 color lists; sub-palette entry 0 (transparent) implicit.
    pub palettes: Vec<Vec<u16>>,
    /// Bitplane-packed char words, bit_depth*4 words/tile, sheet order.
    pub char_words: Vec<u16>,
    /// The palette row of each char, sheet order (the import's cell assignment); empty when
    /// the payload predates v4 or the sheet is generated, and `setSheetPalettes` says instead.
    pub pals: Vec<u8>,
}

/// A BRR-encoded PCM sample. Unlike the image-derived kinds, this is not
/// quantized/packed from RGBA — see `convert_sample`. Placement (writing
/// `brr` into ARAM, once at compile time) is `LuaEngine::set_sources`'s
/// concern (`crate::lua`); the payload itself carries only what the encoder
/// produced.
#[derive(Clone, Debug, PartialEq)]
pub struct SampleSource {
    /// Raw BRR bytes, 9 per 16-sample block.
    pub brr: Vec<u8>,
    /// Block-aligned loop point, in blocks (`loop_start / 16`); `None` if
    /// the sample doesn't loop.
    pub loop_block: Option<u16>,
}

/// Palette rows as BGR555: `row_size` colours each (4 or 16), every row whole; or, when
/// `row_size` is 256, one row of its real colour count (1..=256, entry 0 included).
#[derive(Clone, Debug, PartialEq)]
pub struct PaletteSource {
    pub row_size: u16,
    pub rows: Vec<Vec<u16>>,
}

impl PaletteSource {
    /// The rows as an art source's sub-palettes: entry 0 of each row is the
    /// transparent one placement never writes, so it is left off.
    pub fn sub_palettes(&self) -> Vec<Vec<u16>> {
        self.rows.iter().map(|r| r[1..].to_vec()).collect()
    }

    /// The inverse of [`sub_palettes`](Self::sub_palettes): whole rows of `row_size` from an art
    /// source's sub-palettes. Entry 0 of every row is chosen here, once (0x0000), and unused
    /// slots repeat it, so a rendered PNG's PLTE and a written `.gpl` never disagree. A 256 row is
    /// the exception: it holds entry 0 and the real colours only, `1 +
    /// sub.len()` long, so it claims no more CGRAM than the art uses. Panics if a sub-palette has
    /// `row_size` or more colours (entry 0 takes a slot).
    pub fn from_sub_palettes(row_size: u16, subs: &[Vec<u16>]) -> PaletteSource {
        let rows = subs
            .iter()
            .map(|sub| {
                assert!(
                    sub.len() < row_size as usize,
                    "from_sub_palettes: {} colours do not fit a row of {row_size} beside entry 0",
                    sub.len()
                );
                let len = if row_size == 256 {
                    1 + sub.len()
                } else {
                    row_size as usize
                };
                let mut row = vec![0u16; len];
                row[1..=sub.len()].copy_from_slice(sub);
                row
            })
            .collect();
        PaletteSource { row_size, rows }
    }
}

/// Write a palette source's rows from CGRAM entry `cgram_base`, one row per
/// `row_size` entries, never entry 0 of a row.
pub fn place_palette(src: &PaletteSource, mem: &mut Memory, cgram_base: usize) {
    for (ri, row) in src.rows.iter().enumerate() {
        for (ci, &c) in row.iter().enumerate().skip(1) {
            mem.cgram[(cgram_base + ri * src.row_size as usize + ci) & 0xff] = c;
        }
    }
}

/// The palette row size (colours) a placed art source takes, or `None` for a
/// kind that has no art palette.
pub fn art_row_size(art: &SourcePayload) -> Option<u16> {
    match art {
        SourcePayload::Bg(s) => row_size_for(SourceKind::Bg, s.bit_depth),
        SourcePayload::Sheet(s) => row_size_for(SourceKind::Sheet, s.bit_depth),
        SourcePayload::Obj(_) => row_size_for(SourceKind::Obj, 4),
        SourcePayload::M7(_) => row_size_for(SourceKind::M7, 8),
        _ => None,
    }
}

/// The palette row size (colours) of an art kind at a bit depth (ignored for obj and Mode 7),
/// or `None` for a kind that has no art palette.
pub fn row_size_for(kind: SourceKind, bit_depth: u8) -> Option<u16> {
    match kind {
        SourceKind::Bg | SourceKind::Sheet => Some(row_size_of(bit_depth)),
        SourceKind::Obj => Some(16),
        SourceKind::M7 => Some(256),
        _ => None,
    }
}

/// How many palette rows a placed art source uses (Mode 7 takes one 256-colour block).
pub fn art_palette_rows(art: &SourcePayload) -> Option<usize> {
    art_sub_palettes(art).map(|subs| subs.len())
}

/// The colours an art payload holds, one list per palette row (Mode 7: its one 256-colour
/// block), or `None` for a kind that has no art palette.
pub fn art_sub_palettes(art: &SourcePayload) -> Option<&[Vec<u16>]> {
    match art {
        SourcePayload::Bg(s) => Some(&s.palettes),
        SourcePayload::Sheet(s) => Some(&s.palettes),
        SourcePayload::Obj(s) => Some(&s.palettes),
        SourcePayload::M7(s) => Some(std::slice::from_ref(&s.palette)),
        _ => None,
    }
}

/// Whether palette `pal` can stand in for `art`'s own colours: the same row size, at least
/// the rows the art uses, and for a 256-colour block at least the colours the art uses. The error
/// names the problem for a caller to prefix with the art and palette names.
pub fn palette_fits(art: &SourcePayload, pal: &PaletteSource) -> Result<(), String> {
    let (Some(want), Some(used)) = (art_row_size(art), art_palette_rows(art)) else {
        return Err(format!("a {} source takes no palette", art.kind().name()));
    };
    if pal.row_size != want {
        return Err(format!(
            "the art takes {want}-colour palette rows but the palette has {}-colour rows",
            pal.row_size
        ));
    }
    if pal.rows.len() < used {
        return Err(format!(
            "the art uses {used} palette rows but the palette has {}",
            pal.rows.len()
        ));
    }
    // A 256 block holds only its real colours, so it must reach every colour the art uses.
    if want == 256 {
        let own = match art {
            SourcePayload::M7(m) => m.palette.len(),
            SourcePayload::Bg(b) => b.palettes.first().map_or(0, Vec::len),
            SourcePayload::Sheet(b) => b.palettes.first().map_or(0, Vec::len),
            _ => 0,
        };
        let (need, have) = (1 + own, pal.rows[0].len());
        if have < need {
            return Err(format!(
                "the art uses {need} colours but the palette has {have}"
            ));
        }
    }
    Ok(())
}

/// The colours one palette row holds at a bit depth: 4 at 2bpp, 256 at 8bpp, else 16.
pub fn row_size_of(bit_depth: u8) -> u16 {
    match bit_depth {
        2 => 4,
        8 => 256,
        _ => 16,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SourcePayload {
    Bg(BgSource),
    M7(M7Source),
    Obj(ObjSource),
    Sheet(SheetSource),
    Sample(SampleSource),
    /// Raw PSNG bytes (`crate::song::encode`'s output), carried undecoded —
    /// see the module doc's `song` byte layout.
    Song(Vec<u8>),
    Palette(PaletteSource),
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

/// What travels ALONGSIDE a payload (never inside it): source dims, the
/// authoring-time budget snapshot, and (obj) the per-cell attribute map that
/// makes source cells addressable via obj[i].
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SourceMeta {
    pub width: u32,
    pub height: u32,
    pub report: SourceReport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cells: Option<Vec<ObjCell>>,
}

/// Budget snapshot, tagged like the ImportReport shape the UI already consumes
/// (minus `layer`, which is a placement-time concept).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "mode")]
pub enum SourceReport {
    #[serde(rename = "tile")]
    Tile { report: BudgetReport },
    #[serde(rename = "m7")]
    Mode7 { report: Mode7ImportReport },
    #[serde(rename = "obj")]
    Obj { report: BudgetReport },
    #[serde(rename = "sheet")]
    Sheet { report: BudgetReport },
    #[serde(rename = "sample")]
    Sample {
        report: crate::import::brr::SampleReport,
    },
    #[serde(rename = "palette")]
    Palette { report: PaletteReport },
}

/// A palette source's shape: whole rows of `row_size` colours.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PaletteReport {
    pub rows: usize,
    pub row_size: u16,
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
    /// Read a palette block, rejecting shapes that would alias CGRAM at
    /// placement: more than `max_pals` sub-palettes, or any sub-palette
    /// longer than `max_len`. Both checks happen before the corresponding
    /// bytes are consumed, so a malformed count/length byte alone is enough
    /// to trigger the rejection.
    fn palettes(&mut self, max_pals: usize, max_len: usize) -> Result<Vec<Vec<u16>>, PayloadError> {
        let n = self.u8()? as usize;
        if n > max_pals {
            return Err(PayloadError::BadParam("palettes"));
        }
        (0..n)
            .map(|_| {
                let len = self.u8()? as usize;
                if len > max_len {
                    return Err(PayloadError::BadParam("palettes"));
                }
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
            SourcePayload::Sheet(_) => SourceKind::Sheet,
            SourcePayload::Sample(_) => SourceKind::Sample,
            SourcePayload::Song(_) => SourceKind::Song,
            SourcePayload::Palette(_) => SourceKind::Palette,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut b = vec![PAYLOAD_VERSION];
        match self {
            SourcePayload::Bg(s) => {
                debug_assert!(s.palettes.len() <= 8, "bg: more than 8 sub-palettes");
                let pal_cap = (1usize << s.bit_depth).min(256) - 1;
                debug_assert!(
                    s.palettes.iter().all(|p| p.len() <= pal_cap),
                    "bg: sub-palette longer than bit_depth allows"
                );
                let wpt = s.bit_depth as usize * 4;
                debug_assert!(
                    s.char_words.len() % wpt == 0,
                    "bg: char_words not a whole number of tiles"
                );
                debug_assert!(s.char_words.len() / wpt <= 1024, "bg: more than 1024 tiles");
                debug_assert!(
                    s.tilemap_words.len() == bg_tilemap_len(s.screen_size),
                    "bg: tilemap_words length doesn't match screen_size"
                );
                b.push(0);
                b.push(s.bit_depth);
                b.push(s.tile_size);
                push_palettes(&mut b, &s.palettes);
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
                debug_assert!(
                    s.palette.len() <= if s.options.extbg { 127 } else { 255 },
                    "m7: palette exceeds pixel index capacity"
                );
                debug_assert!(s.tiles.len() <= 256, "m7: more than 256 tiles");
                debug_assert!(
                    s.tiles_w <= 128 && s.tiles_h <= 128,
                    "m7: map dims larger than 128x128"
                );
                debug_assert!(
                    s.map.len() == s.tiles_w as usize * s.tiles_h as usize,
                    "m7: map length doesn't match tiles_w*tiles_h"
                );
                b.push(1);
                // Payloads that fit 256 tiles stay byte-identical to v2's one-byte options block.
                let over = s.options.overflowed_tiles > 256;
                b.push(if over { 3 } else { 1 });
                b.push(s.options.extbg as u8);
                if over {
                    push_u16(&mut b, s.options.overflowed_tiles);
                }
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
                debug_assert!(s.palettes.len() <= 8, "obj: more than 8 sub-palettes");
                debug_assert!(
                    s.palettes.iter().all(|p| p.len() <= 15),
                    "obj: sub-palette longer than 15 (4bpp)"
                );
                debug_assert!(
                    s.char_words.len() % 16 == 0,
                    "obj: char_words not a whole number of tiles"
                );
                debug_assert!(s.char_words.len() / 16 <= 512, "obj: more than 512 tiles");
                b.push(2);
                if s.cell_size == s.cell_height && matches!(s.cell_size, 8 | 16 | 32 | 64) {
                    b.push(s.cell_size);
                } else {
                    b.extend([0, s.cell_size, s.cell_height]);
                }
                push_palettes(&mut b, &s.palettes);
                push_u16(&mut b, (s.char_words.len() / 16) as u16);
                for &w in &s.char_words {
                    push_u16(&mut b, w);
                }
                push_u16(&mut b, s.cells.len() as u16);
                for c in &s.cells {
                    push_u16(&mut b, c.tile);
                    b.push(c.pal);
                    b.push(c.flip_x as u8 | ((c.flip_y as u8) << 1));
                }
            }
            SourcePayload::Sheet(s) => {
                debug_assert!(s.palettes.len() <= 8, "sheet: more than 8 sub-palettes");
                let pal_cap = (1usize << s.bit_depth).min(256) - 1;
                debug_assert!(
                    s.palettes.iter().all(|p| p.len() <= pal_cap),
                    "sheet: sub-palette longer than bit_depth allows"
                );
                let wpt = s.bit_depth as usize * 4;
                debug_assert!(
                    s.char_words.len() % wpt == 0,
                    "sheet: char_words not a whole number of tiles"
                );
                debug_assert!(
                    s.char_words.len() / wpt <= 1024,
                    "sheet: more than 1024 tiles"
                );
                b.push(3);
                b.push(s.bit_depth);
                push_palettes(&mut b, &s.palettes);
                push_u16(&mut b, (s.char_words.len() / wpt) as u16);
                for &w in &s.char_words {
                    push_u16(&mut b, w);
                }
                debug_assert!(
                    s.pals.is_empty() || s.pals.len() == s.char_words.len() / wpt,
                    "sheet: pals is not one per char"
                );
                b.push(!s.pals.is_empty() as u8);
                b.extend(&s.pals);
            }
            SourcePayload::Sample(s) => {
                debug_assert!(
                    s.brr.len() % 9 == 0,
                    "sample: brr not a whole number of blocks"
                );
                let blocks = s.brr.len() / 9;
                debug_assert!(
                    (1..=MAX_SAMPLE_BLOCKS).contains(&blocks),
                    "sample: block count out of range"
                );
                debug_assert!(
                    s.loop_block.is_none_or(|lb| (lb as usize) < blocks),
                    "sample: loop_block out of range"
                );
                b.push(4);
                match s.loop_block {
                    Some(lb) => {
                        b.push(1);
                        push_u16(&mut b, lb);
                    }
                    None => b.push(0),
                }
                push_u16(&mut b, blocks as u16);
                b.extend_from_slice(&s.brr);
            }
            SourcePayload::Song(bytes) => {
                b.push(5);
                b.extend_from_slice(bytes);
            }
            SourcePayload::Palette(s) => {
                let block = s.row_size == 256;
                debug_assert!(
                    if block {
                        s.rows.len() == 1 && (1..=256).contains(&s.rows[0].len())
                    } else {
                        matches!(s.row_size, 4 | 16)
                            && (1..=256 / s.row_size as usize).contains(&s.rows.len())
                            && s.rows.iter().all(|r| r.len() == s.row_size as usize)
                    },
                    "palette: not whole rows of 4 or 16, nor one block of 1..=256"
                );
                // 256 wraps to 0; the 256 block keeps its real colour count in place
                // of a row count of 1.
                b.push(6);
                b.push(s.row_size as u8);
                b.push(if block {
                    s.rows[0].len() - 1
                } else {
                    s.rows.len()
                } as u8);
                for &c in s.rows.iter().flatten() {
                    push_u16(&mut b, c);
                }
            }
        }
        b
    }

    pub fn decode(bytes: &[u8]) -> Result<SourcePayload, PayloadError> {
        let mut r = Rd { b: bytes, i: 0 };
        let version = r.u8()?;
        if !(1..=PAYLOAD_VERSION).contains(&version) {
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
                // 8bpp has no room for sub-palette selection (one CGRAM-wide
                // palette only); 2/4bpp cap at 8 sub-palettes with a
                // bit_depth-sized color count each, matching the CGRAM band
                // stride place_bg writes into.
                let max_pals = if bit_depth == 8 { 1 } else { 8 };
                let max_len = (1usize << bit_depth).min(256) - 1;
                let palettes = r.palettes(max_pals, max_len)?;
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
                let opts_len = r.u8()?;
                if !matches!(opts_len, 0 | 1 | 3) || (version == 1 && opts_len != 0) {
                    return Err(PayloadError::BadParam("m7_options"));
                }
                let extbg = opts_len >= 1
                    && match r.u8()? {
                        0 => false,
                        1 => true,
                        _ => return Err(PayloadError::BadParam("m7_options")),
                    };
                let overflowed_tiles = if opts_len == 3 { r.u16()? } else { 0 };
                let options = M7Options {
                    extbg,
                    overflowed_tiles,
                };
                let pal_len = r.u8()? as usize;
                if pal_len > if options.extbg { 127 } else { 255 } {
                    return Err(PayloadError::BadParam("palette"));
                }
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
                    options,
                    palette,
                    tiles,
                    tiles_w,
                    tiles_h,
                    map,
                })
            }
            2 => {
                let (cell_size, cell_height) = match r.u8()? {
                    0 => (r.u8()?, r.u8()?),
                    n => (n, n),
                };
                let side = |n: u8| n.is_multiple_of(8) && (8..=128).contains(&n);
                if !side(cell_size) || !side(cell_height) {
                    return Err(PayloadError::BadParam("cell_size"));
                }
                // OBJ palettes are always 4bpp: 8 sub-palettes, 15 colors each.
                let palettes = r.palettes(8, 15)?;
                let tile_count = r.u16()? as usize;
                let char_words = r.u16s(tile_count * 16)?;
                let cells = if version >= 2 {
                    let count = r.u16()? as usize;
                    let mut cells = Vec::with_capacity(count);
                    for _ in 0..count {
                        let tile = r.u16()?;
                        let pal = r.u8()?;
                        let flags = r.u8()?;
                        if tile >= 512 || pal >= 8 || flags & !3 != 0 {
                            return Err(PayloadError::BadParam("obj_cell"));
                        }
                        cells.push(ObjCell {
                            tile,
                            pal,
                            flip_x: flags & 1 != 0,
                            flip_y: flags & 2 != 0,
                        });
                    }
                    cells
                } else {
                    Vec::new()
                };
                SourcePayload::Obj(ObjSource {
                    cell_size,
                    cell_height,
                    palettes,
                    char_words,
                    cells,
                })
            }
            3 => {
                let bit_depth = r.u8()?;
                if !matches!(bit_depth, 2 | 4 | 8) {
                    return Err(PayloadError::BadParam("bit_depth"));
                }
                // Same CGRAM-band caps as the assembled bg kind.
                let max_pals = if bit_depth == 8 { 1 } else { 8 };
                let max_len = (1usize << bit_depth).min(256) - 1;
                let palettes = r.palettes(max_pals, max_len)?;
                let tile_count = r.u16()? as usize;
                // A sheet has no legitimate reason to exceed the 10-bit tilemap
                // tile field — the importer already caps there — so reject
                // before allocating on a corrupt/hostile count.
                if tile_count > 1024 {
                    return Err(PayloadError::BadParam("tile_count"));
                }
                let char_words = r.u16s(tile_count * bit_depth as usize * 4)?;
                let pals = match if version >= 4 { r.u8()? } else { 0 } {
                    0 => vec![],
                    1 => r.bytes(tile_count)?.to_vec(),
                    _ => return Err(PayloadError::BadParam("has_pals")),
                };
                if pals.iter().any(|&p| p as usize >= palettes.len().max(1)) {
                    return Err(PayloadError::BadParam("pals"));
                }
                SourcePayload::Sheet(SheetSource {
                    bit_depth,
                    palettes,
                    char_words,
                    pals,
                })
            }
            4 => {
                let has_loop = r.u8()?;
                if has_loop > 1 {
                    return Err(PayloadError::BadParam("has_loop"));
                }
                let loop_block = if has_loop == 1 { Some(r.u16()?) } else { None };
                let block_count = r.u16()? as usize;
                if !(1..=MAX_SAMPLE_BLOCKS).contains(&block_count) {
                    return Err(PayloadError::BadParam("block_count"));
                }
                if let Some(lb) = loop_block {
                    if lb as usize >= block_count {
                        return Err(PayloadError::BadParam("loop_block"));
                    }
                }
                let brr = r.bytes(block_count * 9)?.to_vec();
                SourcePayload::Sample(SampleSource { brr, loop_block })
            }
            // Structural-only: the PSNG bytes are carried as-is and decoded/
            // validated by `crate::song` at `score{}` setup time, not here —
            // see the module doc's `song` byte layout. An empty body (no
            // bytes left) is valid at THIS layer.
            5 => {
                let rest = r.b.len() - r.i;
                SourcePayload::Song(r.bytes(rest)?.to_vec())
            }
            6 => {
                let row_size = match r.u8()? {
                    4 => 4,
                    16 => 16,
                    0 => 256,
                    _ => return Err(PayloadError::BadParam("row_size")),
                };
                let n = r.u8()? as usize;
                let rows = if row_size == 256 {
                    vec![r.u16s(n + 1)?]
                } else {
                    if !(1..=256 / row_size).contains(&n) {
                        return Err(PayloadError::BadParam("row_count"));
                    }
                    (0..n).map(|_| r.u16s(row_size)).collect::<Result<_, _>>()?
                };
                SourcePayload::Palette(PaletteSource {
                    row_size: row_size as u16,
                    rows,
                })
            }
            k => return Err(PayloadError::BadKind(k)),
        };
        r.done()?;
        Ok(out)
    }
}

/// Write a BG source at the placement's bases. `cgram_base` is the CGRAM index the
/// palette block starts at (the mode-0 per-layer band, else 0); sub-palette
/// entry 0 stays unwritten (transparent).
///
/// `src` must be decode-valid (`SourcePayload::decode` is the gate that
/// enforces palette/tile/tilemap shape caps); an out-of-range struct built
/// by hand may panic or write outside its intended CGRAM/VRAM slice.
pub fn place_bg(
    src: &BgSource,
    mem: &mut Memory,
    map_base: u16,
    char_base: u16,
    cgram_base: usize,
) {
    place_bg_in(src, mem, map_base, char_base, cgram_base, None);
}

/// [`place_bg`] with a named palette's rows in place of the source's own.
pub fn place_bg_in(
    src: &BgSource,
    mem: &mut Memory,
    map_base: u16,
    char_base: u16,
    cgram_base: usize,
    palette: Option<&PaletteSource>,
) {
    let own = palette.map(PaletteSource::sub_palettes);
    let palettes = own.as_deref().unwrap_or(&src.palettes);
    place_chars_and_palettes(
        mem,
        char_base,
        cgram_base,
        src.bit_depth,
        palettes,
        &src.char_words,
    );
    // The tilemap's palette bits count from the source's own first palette;
    // shift them to where the palettes were actually placed. A mode-0 band
    // (32 entries = 8 2bpp palettes) wraps the 3-bit field back to itself,
    // since there the hardware adds the band.
    let shift = match src.bit_depth {
        2 => cgram_base / 4,
        4 => cgram_base / 16,
        _ => 0,
    } as u16;
    for (o, &w) in src.tilemap_words.iter().enumerate() {
        let pal = ((w >> 10) + shift) & 7;
        mem.vram[(map_base as usize + o) & 0x7fff] = (w & !0x1c00) | (pal << 10);
    }
}

/// Char words at `char_base` + sub-palettes at the bit-depth CGRAM stride from
/// `cgram_base`. Shared by the assembled-bg and sheet placements — the only
/// difference between them is that a bg also lays down a tilemap.
fn place_chars_and_palettes(
    mem: &mut Memory,
    char_base: u16,
    cgram_base: usize,
    bit_depth: u8,
    palettes: &[Vec<u16>],
    char_words: &[u16],
) {
    for (o, &w) in char_words.iter().enumerate() {
        mem.vram[(char_base as usize + o) & 0x7fff] = w;
    }
    let stride = row_size_of(bit_depth) as usize;
    for (pi, p) in palettes.iter().enumerate() {
        for (ci, &c) in p.iter().enumerate() {
            mem.cgram[(cgram_base + pi * stride + ci + 1) & 0xff] = c;
        }
    }
}

/// Write a sheet source: chars at `char_base`, palettes into the CGRAM band —
/// and no tilemap. Map geometry belongs to the author (`bg[n].map_base` /
/// `screen_size` / `map`), so placement deliberately leaves the map region and
/// the layer's registers alone.
pub fn place_sheet(src: &SheetSource, mem: &mut Memory, char_base: u16, cgram_base: usize) {
    place_sheet_in(src, mem, char_base, cgram_base, None);
}

/// [`place_sheet`] with a named palette's rows in place of the source's own.
pub fn place_sheet_in(
    src: &SheetSource,
    mem: &mut Memory,
    char_base: u16,
    cgram_base: usize,
    palette: Option<&PaletteSource>,
) {
    let own = palette.map(PaletteSource::sub_palettes);
    let palettes = own.as_deref().unwrap_or(&src.palettes);
    place_chars_and_palettes(
        mem,
        char_base,
        cgram_base,
        src.bit_depth,
        palettes,
        &src.char_words,
    );
}

/// Write a Mode 7 source into the byte-interleaved region (words 0..0x4000):
/// char bytes in the high lane, the 128-wide map in the low lane, palette at
/// CGRAM 1.. . Masked-lane writes assume the frame's zeroed-VRAM bootstrap
/// (frame() zeroes VRAM/CGRAM before the dma replay), composing like the m7 pokes.
///
/// `src` must be decode-valid: an out-of-range struct (e.g. `map` shorter
/// than `tiles_w*tiles_h`) may panic or alias VRAM cells outside its map.
pub fn place_m7(src: &M7Source, mem: &mut Memory) {
    place_m7_in(src, mem, None);
}

/// [`place_m7`] with a named palette's colours 1..count (entry 0 stays transparent) in
/// place of the source's own.
pub fn place_m7_in(src: &M7Source, mem: &mut Memory, palette: Option<&PaletteSource>) {
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
    match palette {
        Some(p) => place_palette(p, mem, 0),
        None => {
            for (i, &c) in src.palette.iter().enumerate() {
                mem.cgram[(i + 1) & 0xff] = c;
            }
        }
    }
}

/// Write an OBJ source: char words at `char_base` (wrapping VRAM), palettes
/// into the OBJ CGRAM half (128..).
///
/// `src` must be decode-valid; an out-of-range struct (too many palettes, or
/// a palette longer than 15 colors) may alias CGRAM entries outside its
/// intended band.
pub fn place_obj(src: &ObjSource, mem: &mut Memory, char_base: u16, pal_base: usize) {
    place_obj_in(src, mem, char_base, pal_base, None);
}

/// [`place_obj`] with a named palette's rows in place of the source's own.
pub fn place_obj_in(
    src: &ObjSource,
    mem: &mut Memory,
    char_base: u16,
    pal_base: usize,
    palette: Option<&PaletteSource>,
) {
    for (i, &w) in src.char_words.iter().enumerate() {
        mem.vram[(char_base as usize + i) & 0x7fff] = w;
    }
    let own = palette.map(PaletteSource::sub_palettes);
    let palettes = own.as_deref().unwrap_or(&src.palettes);
    for (pi, p) in palettes.iter().enumerate() {
        for (ci, &c) in p.iter().enumerate() {
            mem.cgram[(128 + (pal_base + pi) * 16 + ci + 1) & 0xff] = c;
        }
    }
}

/// Client-facing conversion options; per-kind fields, absent = default.
/// Extensible: fields irrelevant to a kind are ignored.
#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct ConvertOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bit_depth: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tile_size: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell_size: Option<u8>,
    /// obj: the cell's height when it is not `cell_size` (a rectangular cell).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell_height: Option<u8>,
    /// Mode 7: reserve pixel bit 7 for an external black/white priority mask.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extbg: Option<bool>,
    /// "none" (default) | "bayer" | "diffusion". bg + obj; ignored for m7.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dither: Option<String>,
    /// Dither intensity 0-100 (default 50).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dither_strength: Option<u8>,
    /// Source alpha >= this is opaque, 0-255 (default 128). bg + obj.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alpha_threshold: Option<u8>,
    /// bg + sheet: sub-palette cap 1-8 (default 8); lower merges harder.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_palettes: Option<u8>,
}

/// Validate + collapse the remap-stage fields of [`ConvertOptions`].
fn remap_options(opts: &ConvertOptions) -> Result<crate::import::dither::RemapOptions, String> {
    use crate::import::dither::DitherMode;
    let dither = match opts.dither.as_deref() {
        None | Some("none") => DitherMode::None,
        Some("bayer") => DitherMode::Bayer,
        Some("diffusion") => DitherMode::Diffusion,
        Some(other) => {
            return Err(format!(
                "dither must be none, bayer or diffusion (got {other})"
            ))
        }
    };
    let strength = opts.dither_strength.unwrap_or(50);
    if strength > 100 {
        return Err(format!("dither_strength must be 0-100 (got {strength})"));
    }
    Ok(crate::import::dither::RemapOptions {
        dither,
        strength,
        alpha_threshold: opts.alpha_threshold.unwrap_or(128),
    })
}

/// Pure conversion: quantize + pack an RGBA image into (payload, meta). No
/// engine state involved — this is the core of the `convertSource` wasm entry.
pub fn convert_source(
    kind: SourceKind,
    opts: &ConvertOptions,
    rgba: &[u8],
    width: u32,
    height: u32,
) -> Result<(SourcePayload, SourceMeta), String> {
    convert_source_with_priority(kind, opts, rgba, width, height, None)
}

/// [`convert_source`] with an optional same-sized RGBA Mode 7 EXTBG priority mask.
pub fn convert_source_with_priority(
    kind: SourceKind,
    opts: &ConvertOptions,
    rgba: &[u8],
    width: u32,
    height: u32,
    priority_rgba: Option<&[u8]>,
) -> Result<(SourcePayload, SourceMeta), String> {
    if width == 0 || height == 0 || rgba.len() != (width as usize) * (height as usize) * 4 {
        return Err("imageData dimensions/buffer mismatch".into());
    }
    match kind {
        SourceKind::Bg => {
            let bit_depth = opts.bit_depth.unwrap_or(4);
            if !matches!(bit_depth, 2 | 4 | 8) {
                return Err(format!("bg bit_depth must be 2, 4 or 8 (got {bit_depth})"));
            }
            let io = crate::import::ImportOptions {
                bit_depth,
                tile_size: opts.tile_size.unwrap_or(8),
                remap: remap_options(opts)?,
                max_palettes: opts.max_palettes.unwrap_or(8),
            };
            let (src, meta) = crate::import::import_tile_bg(rgba, width, height, &io);
            Ok((SourcePayload::Bg(src), meta))
        }
        SourceKind::M7 => {
            let extbg = opts.extbg.unwrap_or(false);
            if extbg != priority_rgba.is_some() {
                return Err(if extbg {
                    "m7 extbg requires a priority mask".into()
                } else {
                    "priority mask requires m7 extbg=true".into()
                });
            }
            let (src, meta) = crate::import_m7::import_mode7_with_priority(
                rgba,
                priority_rgba,
                width as usize,
                height as usize,
            )?;
            Ok((SourcePayload::M7(src), meta))
        }
        SourceKind::Obj => {
            let cell_size = opts.cell_size.unwrap_or(8);
            let cell_height = opts.cell_height.unwrap_or(cell_size);
            for (name, n) in [("cell_size", cell_size), ("cell_height", cell_height)] {
                if !n.is_multiple_of(8) || !(8..=128).contains(&n) {
                    return Err(format!(
                        "obj {name} must be a multiple of 8 from 8 to 128 (got {n})"
                    ));
                }
            }
            let (src, meta) = crate::import::obj::import_obj_sheet_cells(
                rgba,
                width,
                height,
                (cell_size, cell_height),
                &remap_options(opts)?,
            );
            Ok((SourcePayload::Obj(src), meta))
        }
        SourceKind::Sheet => {
            let bit_depth = opts.bit_depth.unwrap_or(4);
            if !matches!(bit_depth, 2 | 4 | 8) {
                return Err(format!(
                    "sheet bit_depth must be 2, 4 or 8 (got {bit_depth})"
                ));
            }
            let io = crate::import::ImportOptions {
                bit_depth,
                tile_size: 8, // sheet cells are fixed 8x8
                remap: remap_options(opts)?,
                max_palettes: opts.max_palettes.unwrap_or(8),
            };
            let (src, meta) = crate::import::import_tile_sheet(rgba, width, height, &io);
            Ok((SourcePayload::Sheet(src), meta))
        }
        SourceKind::Sample => Err("sample sources are PCM, not images: use convertSample".into()),
        SourceKind::Palette => {
            Err("palette sources are .gpl text, not images: use convertPalette".into())
        }
        SourceKind::Song => {
            Err("song sources are PSNG bytes, not images: add them directly".into())
        }
    }
}

/// The cells of a generated obj source: their size in pixels, and each cell's tile (from the
/// source's first) and palette, as `sprites.art` reads them.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct GeneratedCells {
    pub cell_w: u8,
    pub cell_h: u8,
    pub cells: Vec<ObjCell>,
}

/// A source made from tiles already in VRAM (Apply's painted tiles): `char_words` exactly as VRAM
/// holds them, tile 0 first, so placing it at the same char base writes the same words back. No
/// palettes: the colours stay where they are, written as `cgram[i]` lines. A BG `Sheet` takes no
/// cells; an `Obj` source (4bpp, OBJ name-table order) takes `cells`, which `sprites.art` draws.
pub fn generated_source(
    kind: SourceKind,
    bit_depth: u8,
    char_words: &[u16],
    cells: Option<&GeneratedCells>,
) -> Result<(SourcePayload, SourceMeta), String> {
    let (max_tiles, max_depth) = match (kind, cells) {
        (SourceKind::Sheet, None) => (1024, 8),
        (SourceKind::Obj, Some(_)) => (512, 4),
        (SourceKind::Sheet | SourceKind::Obj, _) => {
            return Err(format!(
                "a generated {kind:?} source takes cells only when it is Obj"
            ))
        }
        _ => return Err(format!("Apply makes no generated {kind:?} source")),
    };
    if !matches!(bit_depth, 2 | 4 | 8)
        || bit_depth > max_depth
        || (kind == SourceKind::Obj && bit_depth != 4)
    {
        return Err(format!(
            "a generated {kind:?} source's bit depth cannot be {bit_depth}"
        ));
    }
    let per_tile = bit_depth as usize * 4;
    let tiles = char_words.len() / per_tile;
    if char_words.is_empty() || !char_words.len().is_multiple_of(per_tile) || tiles > max_tiles {
        return Err(format!(
            "a generated source holds 1 to {max_tiles} whole {bit_depth}bpp tiles ({per_tile} words each), not {} words",
            char_words.len()
        ));
    }
    let report = BudgetReport {
        colors_used: 0,
        palettes_used: 0,
        tile_cells: tiles,
        unique_tiles: tiles,
        vram_words: char_words.len(),
        overflows: vec![],
    };
    let cols = tiles.min(16);
    let (width, height) = (cols as u32 * 8, tiles.div_ceil(16) as u32 * 8);
    let Some(c) = cells else {
        let payload = SourcePayload::Sheet(SheetSource {
            bit_depth,
            palettes: vec![],
            char_words: char_words.to_vec(),
            pals: vec![],
        });
        return Ok((
            payload,
            SourceMeta {
                width,
                height,
                report: SourceReport::Sheet { report },
                cells: None,
            },
        ));
    };
    for (name, n) in [("cell width", c.cell_w), ("cell height", c.cell_h)] {
        if n % 8 != 0 || !(8..=128).contains(&n) {
            return Err(format!(
                "a generated obj source's {name} must be a multiple of 8 from 8 to 128 (got {n})"
            ));
        }
    }
    if c.cells.is_empty() || c.cells.len() > 1024 {
        return Err(format!(
            "a generated obj source holds 1 to 1024 cells, not {}",
            c.cells.len()
        ));
    }
    // A cell's last tile: its bottom row, OBJ name-table rows 16 tiles apart.
    let last = |t: &ObjCell| {
        t.tile as usize + (c.cell_h as usize / 8 - 1) * 16 + c.cell_w as usize / 8 - 1
    };
    if let Some(bad) = c.cells.iter().find(|t| {
        last(t) >= tiles || t.pal > 7 || (t.tile as usize % 16) + c.cell_w as usize / 8 > 16
    }) {
        return Err(format!(
            "a generated obj cell at tile {} (palette {}) does not fit its {tiles} tiles, a 16-tile row or palettes 0..7",
            bad.tile, bad.pal
        ));
    }
    let payload = SourcePayload::Obj(ObjSource {
        cell_size: c.cell_w,
        cell_height: c.cell_h,
        palettes: vec![],
        char_words: char_words.to_vec(),
        cells: c.cells.clone(),
    });
    Ok((
        payload,
        SourceMeta {
            width,
            height,
            report: SourceReport::Obj { report },
            cells: Some(c.cells.clone()),
        },
    ))
}

/// Options for [`convert_sample`].

#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct ConvertSampleOptions {
    /// Loop point in input PCM frames; block-aligned by the encoder.
    #[serde(alias = "loopStart")]
    pub loop_start: Option<usize>,
}

/// Pure conversion: BRR-encode mono 32 kHz i16 PCM into (payload, meta). The
/// browser does WAV decode + resampling; the engine only ever sees PCM
/// already at the SPC700's native rate. This is the core of the
/// `convertSample` wasm entry.
pub fn convert_sample(
    pcm: &[i16],
    opts: &ConvertSampleOptions,
) -> Result<(SourcePayload, SourceMeta), String> {
    if pcm.is_empty() {
        return Err("sample has no PCM".into());
    }
    if let Some(loop_start) = opts.loop_start {
        if loop_start >= pcm.len() {
            return Err(format!(
                "loop_start {} is past the end of the sample ({} frames)",
                loop_start,
                pcm.len()
            ));
        }
    }
    // Reject up front: the encoder below is brute force, so a multi-minute
    // WAV must not spend minutes encoding before this same check fires
    // post-encode — see MAX_SAMPLE_BLOCKS.
    let blocks = pcm.len().div_ceil(16);
    if blocks > MAX_SAMPLE_BLOCKS {
        return Err(format!(
            "sample too long for sound RAM ({blocks} blocks, max {MAX_SAMPLE_BLOCKS})"
        ));
    }
    let (brr, report) = crate::import::brr::encode_brr(pcm, opts.loop_start);
    let loop_block = report.loop_start.map(|s| (s / 16) as u16);
    let payload = SourcePayload::Sample(SampleSource { brr, loop_block });
    let meta = SourceMeta {
        width: pcm.len() as u32,
        height: 1,
        report: SourceReport::Sample { report },
        cells: None,
    };
    Ok((payload, meta))
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

    fn sample_sheet() -> SheetSource {
        SheetSource {
            bit_depth: 2,
            palettes: vec![vec![0x001f, 0x7c00], vec![0x03e0]],
            char_words: vec![0xbeefu16; 2 * 8], // 2 tiles at 2bpp
            pals: vec![0, 1],
        }
    }

    // PPU-136: a hand-built two-block BRR blob (18 bytes), distinguishable
    // per-byte so a mis-sliced encode/decode boundary would show up.
    fn sample_brr_two_blocks() -> Vec<u8> {
        (0..18u8).collect()
    }

    fn sample_sample() -> SampleSource {
        SampleSource {
            brr: sample_brr_two_blocks(),
            loop_block: Some(1),
        }
    }

    // PPU-93: sheet payload is bit depth + palettes + char words + each char's palette, nothing else.
    #[test]
    fn sheet_roundtrips_and_carries_no_map_geometry() {
        let p = SourcePayload::Sheet(sample_sheet());
        let b = p.encode();
        assert_eq!(SourcePayload::decode(&b).unwrap(), p);
        // version, kind=3, bit_depth, pal_count, pal0 len, color lo/hi ...
        assert_eq!(&b[..6], &[PAYLOAD_VERSION, 3, 2, 2, 2, 0x1f]);
        // header(2) + bit_depth(1) + pal block(1 + (1+4) + (1+2) = 9) = 12, then u16 tile_count
        assert_eq!(&b[12..14], &[2, 0]);
        // ...then the char words, and the payload ENDS with each char's palette: no screen_size byte, no tilemap.
        assert_eq!(b.len(), 14 + 16 * 2 + 1 + 2);
        assert_eq!(&b[14 + 16 * 2..], &[1, 0, 1]);
        // A sheet that does not say its chars' palettes carries only the flag.
        let none = SourcePayload::Sheet(SheetSource { pals: vec![], ..sample_sheet() });
        assert_eq!(SourcePayload::decode(&none.encode()).unwrap(), none);
        assert_eq!(none.encode().len(), 14 + 16 * 2 + 1);
    }

    // PPU-93: same strict codec discipline as the existing kinds.
    #[test]
    fn sheet_decode_is_strict() {
        assert_eq!(
            SourcePayload::decode(&[1, 3, 3]),
            Err(PayloadError::BadParam("bit_depth"))
        );
        let mut too_many_pals = SourcePayload::Sheet(sample_sheet()).encode();
        too_many_pals[3] = 9;
        assert_eq!(
            SourcePayload::decode(&too_many_pals),
            Err(PayloadError::BadParam("palettes"))
        );
        let mut eight_bpp = SourcePayload::Sheet(sample_sheet()).encode();
        eight_bpp[2] = 8; // 8bpp allows only one sub-palette
        assert_eq!(
            SourcePayload::decode(&eight_bpp),
            Err(PayloadError::BadParam("palettes"))
        );
        let mut trailing = SourcePayload::Sheet(sample_sheet()).encode();
        trailing.push(0);
        assert_eq!(
            SourcePayload::decode(&trailing),
            Err(PayloadError::TrailingBytes)
        );
        let mut short = SourcePayload::Sheet(sample_sheet()).encode();
        short.truncate(short.len() - 1);
        assert_eq!(SourcePayload::decode(&short), Err(PayloadError::Truncated));
        // a corrupt tile_count is rejected before any allocation
        let mut huge = SourcePayload::Sheet(sample_sheet()).encode();
        huge[12..14].copy_from_slice(&1025u16.to_le_bytes());
        assert_eq!(
            SourcePayload::decode(&huge),
            Err(PayloadError::BadParam("tile_count"))
        );
        // A char's palette must be one of the sheet's.
        let mut past = SourcePayload::Sheet(sample_sheet()).encode();
        *past.last_mut().unwrap() = 2;
        assert_eq!(SourcePayload::decode(&past), Err(PayloadError::BadParam("pals")));
        let mut flag = SourcePayload::Sheet(sample_sheet()).encode();
        flag[14 + 16 * 2] = 2;
        assert_eq!(SourcePayload::decode(&flag), Err(PayloadError::BadParam("has_pals")));
    }

    fn sample_obj() -> ObjSource {
        ObjSource {
            cell_size: 8,
            cell_height: 8,
            palettes: vec![vec![0x001f]],
            char_words: vec![0x00ffu16; 2 * 16],
            cells: vec![crate::import::obj::ObjCell {
                tile: 1,
                pal: 0,
                flip_x: false,
                flip_y: false,
            }],
        }
    }

    #[test]
    fn encode_decode_roundtrips_all_kinds() {
        for p in [
            SourcePayload::Bg(sample_bg()),
            SourcePayload::M7(sample_m7()),
            SourcePayload::Obj(sample_obj()),
            SourcePayload::Sample(sample_sample()),
        ] {
            assert_eq!(SourcePayload::decode(&p.encode()).unwrap(), p);
        }
    }

    // Stored payloads from every earlier version keep decoding after a bump.
    #[test]
    fn decode_accepts_every_earlier_payload_version() {
        // Before v4 a sheet ended with its char words and did not say its chars' palettes.
        let old = SourcePayload::Sheet(SheetSource { pals: vec![], ..sample_sheet() });
        for version in [2, 3] {
            let mut b = old.encode();
            b.pop();
            b[0] = version;
            assert_eq!(SourcePayload::decode(&b).unwrap(), old);
        }
        let mut v1 = SourcePayload::Bg(sample_bg()).encode();
        v1[0] = 1;
        assert_eq!(
            SourcePayload::decode(&v1).unwrap(),
            SourcePayload::Bg(sample_bg())
        );
        let mut future = SourcePayload::Bg(sample_bg()).encode();
        future[0] = PAYLOAD_VERSION + 1;
        assert_eq!(
            SourcePayload::decode(&future),
            Err(PayloadError::BadVersion(PAYLOAD_VERSION + 1))
        );
    }

    #[test]
    fn payload_self_describes_version_and_kind() {
        let v = PAYLOAD_VERSION;
        assert_eq!(SourcePayload::Bg(sample_bg()).encode()[..2], [v, 0]);
        assert_eq!(SourcePayload::M7(sample_m7()).encode()[..2], [v, 1]);
        assert_eq!(SourcePayload::Obj(sample_obj()).encode()[..2], [v, 2]);
        assert_eq!(SourcePayload::Sheet(sample_sheet()).encode()[..2], [v, 3]);
        assert_eq!(SourcePayload::Sample(sample_sample()).encode()[..2], [v, 4]);
        assert_eq!(SourcePayload::Song(vec![1, 2, 3]).encode()[..2], [v, 5]);
    }

    // A song source's payload is the PSNG bytes verbatim after the common
    // version+kind header — this layer never parses them (a malformed or
    // even empty body still round-trips; `crate::song::decode` is what
    // validates the PSNG shape, at `score{}` setup time).
    #[test]
    fn song_roundtrips_undecoded_psng_bytes() {
        let psng = b"PSNG\x01not-really-a-valid-body".to_vec();
        let p = SourcePayload::Song(psng.clone());
        let b = p.encode();
        assert_eq!(&b[..2], &[PAYLOAD_VERSION, 5]);
        assert_eq!(&b[2..], &psng[..]);
        assert_eq!(SourcePayload::decode(&b).unwrap(), p);
    }

    #[test]
    fn song_decode_accepts_an_empty_psng_body() {
        let p = SourcePayload::Song(Vec::new());
        let b = p.encode();
        assert_eq!(b, vec![PAYLOAD_VERSION, 5]);
        assert_eq!(SourcePayload::decode(&b).unwrap(), p);
    }

    // PPU-136: a sample is a first-class source payload — encoded as
    // has_loop + (loop_block) + block_count + raw BRR bytes, with and
    // without a loop point.
    #[test]
    fn sample_roundtrips_with_and_without_loop() {
        let looped = sample_sample();
        let p = SourcePayload::Sample(looped.clone());
        let b = p.encode();
        // version, kind=4, has_loop=1, loop_block=1 LE, block_count=2 LE
        assert_eq!(&b[..7], &[PAYLOAD_VERSION, 4, 1, 1, 0, 2, 0]);
        assert_eq!(&b[7..], &looped.brr[..]);
        assert_eq!(SourcePayload::decode(&b).unwrap(), p);

        let unlooped = SampleSource {
            brr: sample_brr_two_blocks(),
            loop_block: None,
        };
        let p2 = SourcePayload::Sample(unlooped.clone());
        let b2 = p2.encode();
        // version, kind=4, has_loop=0, block_count=2 LE
        assert_eq!(&b2[..5], &[PAYLOAD_VERSION, 4, 0, 2, 0]);
        assert_eq!(&b2[5..], &unlooped.brr[..]);
        assert_eq!(SourcePayload::decode(&b2).unwrap(), p2);
    }

    // PPU-136: same strict codec discipline as the existing kinds — every
    // field boundary is checked, block_count is bounds-checked before any
    // allocation, and truncation/trailing bytes reject.
    #[test]
    fn sample_decode_is_strict() {
        let base = SourcePayload::Sample(sample_sample()).encode();

        let mut bad_has_loop = base.clone();
        bad_has_loop[2] = 2;
        assert_eq!(
            SourcePayload::decode(&bad_has_loop),
            Err(PayloadError::BadParam("has_loop"))
        );

        let mut bad_loop_block = base.clone();
        bad_loop_block[3] = 2; // loop_block lo byte: now == block_count (2)
        assert_eq!(
            SourcePayload::decode(&bad_loop_block),
            Err(PayloadError::BadParam("loop_block"))
        );

        assert_eq!(
            SourcePayload::decode(&[3, 4, 0, 0, 0]),
            Err(PayloadError::BadParam("block_count"))
        );

        let mut huge_block_count = vec![3u8, 4, 0];
        huge_block_count.extend_from_slice(&7282u16.to_le_bytes());
        assert_eq!(
            SourcePayload::decode(&huge_block_count),
            Err(PayloadError::BadParam("block_count"))
        );

        let mut truncated = base.clone();
        truncated.truncate(truncated.len() - 1);
        assert_eq!(
            SourcePayload::decode(&truncated),
            Err(PayloadError::Truncated)
        );

        let mut trailing = base;
        trailing.push(0);
        assert_eq!(
            SourcePayload::decode(&trailing),
            Err(PayloadError::TrailingBytes)
        );
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
        assert_eq!(&b[..8], &[PAYLOAD_VERSION, 0, 2, 8, 1, 1, 0x1f, 0x00]);
        assert_eq!(&b[8..10], &[1, 0]); // u16 LE tile_count = 1
        assert_eq!(b[10 + 16], 0); // screen_size byte after 8 char words
        assert_eq!(b.len(), 10 + 16 + 1 + 0x400 * 2);
    }

    #[test]
    fn m7_options_block_carries_extbg_and_v1_stays_readable() {
        let b = SourcePayload::M7(sample_m7()).encode();
        assert_eq!(&b[2..4], &[1, 0]);
        let mut evil = b.clone();
        evil[3] = 2;
        assert_eq!(
            SourcePayload::decode(&evil),
            Err(PayloadError::BadParam("m7_options"))
        );
        let mut v1 = b;
        v1[0] = 1;
        v1[2] = 0;
        v1.remove(3);
        assert_eq!(
            SourcePayload::decode(&v1).unwrap(),
            SourcePayload::M7(sample_m7())
        );
    }

    #[test]
    fn decode_rejects_garbage() {
        assert_eq!(SourcePayload::decode(&[]), Err(PayloadError::Truncated));
        assert_eq!(
            SourcePayload::decode(&[PAYLOAD_VERSION + 1, 0]),
            Err(PayloadError::BadVersion(PAYLOAD_VERSION + 1))
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
    fn decode_rejects_bg_screen_size_out_of_range() {
        let s = BgSource {
            bit_depth: 2,
            tile_size: 8,
            palettes: vec![vec![0x001f]],
            char_words: vec![0u16; 8],
            screen_size: 0,
            tilemap_words: vec![0u16; 0x400],
        };
        let mut b = SourcePayload::Bg(s).encode();
        // Same offset the layout test locks: screen_size sits right after
        // the 8 char words that follow the 10-byte header+palette+tile_count.
        let screen_size_idx = 10 + 16;
        assert_eq!(b[screen_size_idx], 0);
        b[screen_size_idx] = 4;
        assert_eq!(
            SourcePayload::decode(&b),
            Err(PayloadError::BadParam("screen_size"))
        );
    }

    #[test]
    fn decode_rejects_bg_too_many_palettes() {
        let mut b = SourcePayload::Bg(sample_bg()).encode();
        // pal_count byte sits right after bit_depth/tile_size in the header.
        let pal_count_idx = 4;
        assert_eq!(b[pal_count_idx], 2);
        b[pal_count_idx] = 9;
        assert_eq!(
            SourcePayload::decode(&b),
            Err(PayloadError::BadParam("palettes"))
        );
    }

    #[test]
    fn decode_rejects_bg_8bpp_with_more_than_one_palette() {
        let mut b = SourcePayload::Bg(sample_bg()).encode();
        // sample_bg is 2bpp with 2 sub-palettes; bumping bit_depth to 8
        // leaves pal_count=2, which exceeds the 8bpp single-palette cap.
        b[2] = 8;
        assert_eq!(
            SourcePayload::decode(&b),
            Err(PayloadError::BadParam("palettes"))
        );
    }

    #[test]
    fn decode_rejects_obj_palette_longer_than_15() {
        let mut b = SourcePayload::Obj(sample_obj()).encode();
        // header(2) + cell_size(1) + pal_count(1) = byte 4 is pal0's len.
        let pal0_len_idx = 4;
        assert_eq!(b[pal0_len_idx], 1);
        b[pal0_len_idx] = 16;
        assert_eq!(
            SourcePayload::decode(&b),
            Err(PayloadError::BadParam("palettes"))
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
    fn place_bg_shifts_map_palette_bits_to_the_placed_palettes() {
        // A 4bpp source placed at CGRAM 32 starts at palette 2, so its map's
        // palette 1 must become palette 3. Tile, flip and priority bits stay.
        let mut s = sample_bg();
        s.bit_depth = 4;
        s.tilemap_words[0] = 0xe000 | (1 << 10) | 5;
        let mut mem = Memory::new();
        place_bg(&s, &mut mem, 0x0000, 0x1000, 32);
        assert_eq!(mem.vram[0], 0xe000 | (3 << 10) | 5);
        // 2bpp in a mode-0 band (CGRAM 32 = 8 palettes) wraps back to itself:
        // the hardware adds the band, so the map must not.
        let mut s2 = sample_bg();
        s2.tilemap_words[0] = (1 << 10) | 5;
        let mut mem2 = Memory::new();
        place_bg(&s2, &mut mem2, 0x0000, 0x1000, 32);
        assert_eq!(mem2.vram[0], (1 << 10) | 5);
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
    fn place_m7_map_row_stride_is_128() {
        // 2x2 map: a naive row-major write (stride = tiles_w) would land
        // row 1 at vram[2]/vram[3] instead of the fixed 128-word HDMA stride.
        let s = M7Source {
            options: M7Options::default(),
            palette: vec![0x001f],
            tiles: vec![[0u8; 64]; 4],
            tiles_w: 2,
            tiles_h: 2,
            map: vec![0, 1, 1, 0],
        };
        let mut mem = Memory::new();
        place_m7(&s, &mut mem);
        assert_eq!(mem.vram[128] & 0x00ff, 1); // row 1, col 0 = map[2]
        assert_eq!(mem.vram[129] & 0x00ff, 0); // row 1, col 1 = map[3]
        assert_eq!(mem.vram[2] & 0x00ff, 0); // not aliased to a stride-2 write
    }

    #[test]
    fn a_generated_sheet_places_the_words_it_was_made_from() {
        let words: Vec<u16> = (0..40).map(|i| 0x1234u16.wrapping_mul(i + 1)).collect();
        let (payload, meta) = generated_source(SourceKind::Sheet, 2, &words[..32], None).unwrap();
        assert_eq!((meta.width, meta.height), (32, 8));
        let decoded = SourcePayload::decode(&payload.encode()).unwrap();
        assert_eq!(decoded, payload);
        let SourcePayload::Sheet(sheet) = decoded else {
            panic!("a sheet")
        };
        assert!(sheet.palettes.is_empty());
        let mut mem = Memory::new();
        place_sheet(&sheet, &mut mem, 0x2000, 0);
        assert_eq!(&mem.vram[0x2000..0x2020], &words[..32]);
        assert!(
            mem.cgram.iter().all(|&c| c == 0),
            "no palettes: CGRAM untouched"
        );
        let (_, wide) = generated_source(SourceKind::Sheet, 4, &vec![0; 16 * 17], None).unwrap();
        assert_eq!((wide.width, wide.height), (128, 16));
        assert!(generated_source(SourceKind::Sheet, 4, &words[..20], None)
            .unwrap_err()
            .contains("whole 4bpp tiles"));
        assert!(generated_source(SourceKind::Sheet, 3, &words, None).is_err());
        assert!(generated_source(SourceKind::Sheet, 2, &[], None).is_err());
        assert!(generated_source(SourceKind::Obj, 4, &words[..32], None).is_err());
    }

    #[test]
    fn a_generated_obj_source_keeps_its_cells_and_words() {
        let words: Vec<u16> = (0..16 * 19).map(|i| i as u16 * 3 + 1).collect();
        let cell = |tile, pal| ObjCell {
            tile,
            pal,
            flip_x: false,
            flip_y: false,
        };
        let shape = GeneratedCells {
            cell_w: 16,
            cell_h: 16,
            cells: vec![cell(0, 0), cell(1, 5)],
        };
        let (payload, meta) = generated_source(SourceKind::Obj, 4, &words, Some(&shape)).unwrap();
        assert_eq!(SourcePayload::decode(&payload.encode()).unwrap(), payload);
        let SourcePayload::Obj(obj) = payload else {
            panic!("an obj source")
        };
        assert_eq!(
            (obj.cell_size, obj.cell_height, obj.char_words.len()),
            (16, 16, words.len())
        );
        assert!(obj.palettes.is_empty());
        assert_eq!(meta.cells.as_deref(), Some(&shape.cells[..]));
        let too_low = GeneratedCells {
            cells: vec![cell(2, 0)],
            ..shape.clone()
        };
        assert!(generated_source(SourceKind::Obj, 4, &words, Some(&too_low))
            .unwrap_err()
            .contains("does not fit"));
        let wraps = GeneratedCells {
            cells: vec![cell(15, 0)],
            ..shape.clone()
        };
        assert!(
            generated_source(SourceKind::Obj, 4, &vec![0; 16 * 40], Some(&wraps))
                .unwrap_err()
                .contains("16-tile row")
        );
        assert!(generated_source(SourceKind::Obj, 8, &words, Some(&shape)).is_err());
        assert!(generated_source(SourceKind::Sheet, 4, &words, Some(&shape)).is_err());
    }

    // PPU-93: a sheet places chars + palettes and NOTHING else — no tilemap.
    #[test]
    fn place_sheet_writes_chars_and_banded_palette_but_no_map() {
        let s = sample_sheet();
        let mut mem = Memory::new();
        mem.vram[0x0000] = 0xdead; // an author-owned map word
        place_sheet(&s, &mut mem, 0x1000, 0);
        assert_eq!(mem.vram[0x1000], 0xbeef); // char 0 = first sheet cell
        assert_eq!(mem.vram[0x0000], 0xdead); // map region untouched
        assert_eq!(mem.cgram[1], 0x001f); // pal 0 entry 1 (stride 4 at 2bpp)
        assert_eq!(mem.cgram[2], 0x7c00);
        assert_eq!(mem.cgram[5], 0x03e0); // pal 1 entry 1 = 1*4+0+1
        assert_eq!(mem.cgram[0], 0); // transparent slot untouched
        let mut mem2 = Memory::new();
        place_sheet(&s, &mut mem2, 0x1000, 32); // mode-0 band shifts the block
        assert_eq!(mem2.cgram[33], 0x001f);
    }

    #[test]
    fn place_obj_writes_char_at_base_and_obj_palettes() {
        let s = sample_obj();
        let mut mem = Memory::new();
        place_obj(&s, &mut mem, 0x2000, 0);
        assert_eq!(mem.vram[0x2000], 0x00ff);
        assert_eq!(mem.cgram[129], 0x001f);
        assert_eq!(mem.cgram[1], 0); // BG half untouched
    }

    // PPU-93: the kind string the wasm `convertSource` entry parses.
    #[test]
    fn kind_strings_include_sheet_and_reject_the_unknown() {
        assert_eq!("bg".parse(), Ok(SourceKind::Bg));
        assert_eq!("m7".parse(), Ok(SourceKind::M7));
        assert_eq!("obj".parse(), Ok(SourceKind::Obj));
        assert_eq!("sheet".parse(), Ok(SourceKind::Sheet));
        assert_eq!("sample".parse(), Ok(SourceKind::Sample));
        assert_eq!(
            "tilesheet".parse::<SourceKind>(),
            Err("unknown source kind 'tilesheet'".to_string())
        );
    }

    // PPU-136: samples are PCM, not images — convert_source refuses the kind
    // and points callers at convertSample.
    #[test]
    fn convert_source_rejects_sample_kind() {
        let rgba = vec![0u8; 8 * 8 * 4];
        let err = convert_source(SourceKind::Sample, &ConvertOptions::default(), &rgba, 8, 8)
            .unwrap_err();
        assert!(err.contains("convertSample"), "unexpected error: {err}");
    }

    // A song source is authored PSNG data, not an image: `convert_source`
    // refuses the kind rather than trying to quantize it.
    #[test]
    fn convert_source_rejects_song_kind() {
        let rgba = vec![0u8; 8 * 8 * 4];
        let err =
            convert_source(SourceKind::Song, &ConvertOptions::default(), &rgba, 8, 8).unwrap_err();
        assert!(err.contains("PSNG"), "unexpected error: {err}");
    }

    // PPU-93: the pure conversion entry accepts the sheet kind and carries the
    // per-cell attribute map, same shape the OBJ sheet source emits.
    #[test]
    fn convert_source_sheet_emits_sheet_payload_with_cells() {
        let rgba = vec![255u8, 0, 0, 255, 0, 0, 255, 255]; // 2x1
        let (p, meta) =
            convert_source(SourceKind::Sheet, &ConvertOptions::default(), &rgba, 2, 1).unwrap();
        assert!(matches!(&p, SourcePayload::Sheet(s) if s.bit_depth == 4));
        let cells = meta.cells.as_ref().unwrap();
        assert_eq!(cells.len(), 1); // a 2x1 image is one 8x8 cell
        assert_eq!(cells[0].tile, 0);
        assert!(!cells[0].flip_x && !cells[0].flip_y);
        assert!(matches!(meta.report, SourceReport::Sheet { .. }));
        let bad_bd = ConvertOptions {
            bit_depth: Some(3),
            ..Default::default()
        };
        assert!(convert_source(SourceKind::Sheet, &bad_bd, &rgba, 2, 1).is_err());
    }

    #[test]
    fn convert_source_rejects_bad_dims() {
        assert!(convert_source(SourceKind::Bg, &ConvertOptions::default(), &[0; 3], 1, 1).is_err());
        assert!(convert_source(SourceKind::Bg, &ConvertOptions::default(), &[], 0, 0).is_err());
    }

    #[test]
    fn convert_source_bg_defaults_to_4bpp() {
        let rgba = vec![255u8, 0, 0, 255, 0, 0, 255, 255]; // 2x1
        let (p, _m) =
            convert_source(SourceKind::Bg, &ConvertOptions::default(), &rgba, 2, 1).unwrap();
        assert!(matches!(&p, SourcePayload::Bg(s) if s.bit_depth == 4));
    }

    #[test]
    fn convert_source_rejects_bad_bit_depth_and_cell_size() {
        let rgba = vec![0u8; 4];
        let bad_bd = ConvertOptions {
            bit_depth: Some(3),
            ..Default::default()
        };
        assert!(convert_source(SourceKind::Bg, &bad_bd, &rgba, 1, 1).is_err());
        for (w, h) in [(Some(20), None), (Some(136), None), (Some(16), Some(4))] {
            let bad_cs = ConvertOptions {
                cell_size: w,
                cell_height: h,
                ..Default::default()
            };
            assert!(convert_source(SourceKind::Obj, &bad_cs, &rgba, 1, 1).is_err());
        }
        let rect = ConvertOptions {
            cell_size: Some(48),
            cell_height: Some(32),
            ..Default::default()
        };
        let (p, _) = convert_source(SourceKind::Obj, &rect, &rgba, 1, 1).unwrap();
        let bytes = p.encode();
        assert_eq!(&bytes[2..5], &[0, 48, 32]);
        assert_eq!(SourcePayload::decode(&bytes), Ok(p));
    }

    #[test]
    fn convert_source_obj_meta_has_cells() {
        let rgba = vec![255u8, 0, 0, 255]; // 1x1
        let (_p, meta) =
            convert_source(SourceKind::Obj, &ConvertOptions::default(), &rgba, 1, 1).unwrap();
        assert!(meta.cells.is_some());
    }
}
