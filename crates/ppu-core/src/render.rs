//! A source payload as an indexed image, and PNG in and out: the exact two-way map.
//!
//! [`render`] draws a bg, sheet, obj or Mode 7 payload (with its [`SourceMeta`], which holds the
//! dimensions and a sheet's per-tile palette rows) as one byte per pixel: the global PLTE index
//! `row * row_size + local`, where a row is 4 colours at 2bpp, 16 at 4bpp and obj, 256 at 8bpp
//! and Mode 7. The PLTE is the whole palette in rows, entry 0 of each row transparent.
//! [`crate::import::keep`] reads such an image back, keeping its indices, so
//! `import(render(p)) == p` byte for byte.

use crate::import::tiles::{unpack_planar, IndexTile};
use crate::memory::unpack_rgb15;
use crate::source::{
    art_row_size, bg_tilemap_len, BgSource, M7Source, ObjSource, PaletteSource, SheetSource,
    SourceMeta, SourcePayload,
};

/// Pixels as palette indices, with the palette they index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexedImage {
    pub width: u32,
    pub height: u32,
    /// The PLTE index of each pixel, row-major.
    pub pixels: Vec<u8>,
    /// The PLTE as BGR555, whole rows in order.
    pub palette: Vec<u16>,
    /// Colours per palette row (4, 16 or 256); 0 on an image decoded from a PNG, which does not say.
    pub row_size: u16,
    /// Mode 7 EXTBG: PLTE entries 128..=255 mirror 0..=127, and entry 128 is transparent too.
    pub extbg: bool,
}

/// An indexed PNG as decoded: the image and the alpha of each PLTE entry (`tRNS`; 255 where absent).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexedPng {
    pub image: IndexedImage,
    pub alpha: Vec<u8>,
}

/// A decoded PNG: indexed ones keep their indices; any other colour type is RGBA.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodedPng {
    Indexed(IndexedPng),
    Rgba {
        rgba: Vec<u8>,
        width: u32,
        height: u32,
    },
}

/// A palette source's PLTE: its rows, verbatim; under Mode 7 EXTBG the full 256 with the top half
/// mirroring the bottom (EXTBG reads 128 + k as k, bit 7 being priority). The one place a named
/// palette becomes PLTE entries.
pub fn plte_of(pal: &PaletteSource, extbg: bool) -> Vec<u16> {
    let mut plte = pal.rows.concat();
    if extbg {
        mirror_extbg(&mut plte);
    }
    plte
}

/// How many leading entries of [`plte_of`] are the palette's own colours (of its `len`), the rest
/// mirroring them: under Mode 7 EXTBG only the first 128. What an editor may save back to it.
pub fn own_entries(len: usize, extbg: bool) -> usize {
    if extbg {
        len.min(EXTBG_COLOURS)
    } else {
        len
    }
}

/// Mode 7 EXTBG colours are 7-bit: PLTE entry `EXTBG_COLOURS + k` is entry `k`.
const EXTBG_COLOURS: usize = 128;

fn mirror_extbg(plte: &mut Vec<u16>) {
    plte.resize(2 * EXTBG_COLOURS, 0);
    for k in 0..EXTBG_COLOURS {
        plte[EXTBG_COLOURS + k] = plte[k];
    }
}

/// Render a source payload as an indexed image. Palette, sample and song sources are not images.
/// With `palette`, the PLTE is that palette's rows verbatim (it must fit the art, as `dma` asks)
/// instead of the art's own colours; pixels are unchanged.
pub fn render(
    payload: &SourcePayload,
    meta: &SourceMeta,
    palette: Option<&PaletteSource>,
) -> Result<IndexedImage, String> {
    let Some(row_size) = art_row_size(payload) else {
        return Err(match payload {
            SourcePayload::Palette(_) => {
                "a palette source is not an image: write it as .gpl text with gpl::write".into()
            }
            SourcePayload::Sample(_) => "a sample source is audio, not an image".into(),
            _ => "a song source is audio, not an image".into(),
        });
    };
    if meta.width == 0 || meta.height == 0 {
        return Err("the source has no pixels (width or height is 0)".into());
    }
    let mut img = match payload {
        SourcePayload::Bg(s) => render_bg(s, meta, row_size),
        SourcePayload::Sheet(s) => render_sheet(s, meta, row_size),
        SourcePayload::Obj(s) => render_obj(s, meta, row_size),
        SourcePayload::M7(s) => render_m7(s, meta),
        _ => unreachable!("art_row_size is None for every non-image kind"),
    }?;
    if let Some(pal) = palette {
        crate::source::palette_fits(payload, pal)?;
        img.palette = plte_of(pal, img.extbg);
        if !(1..=256).contains(&img.palette.len()) {
            return Err(format!(
                "the palette makes a PLTE of {} entries; a PNG palette holds 1 to 256",
                img.palette.len()
            ));
        }
        if let Some(&p) = img
            .pixels
            .iter()
            .max()
            .filter(|&&p| p as usize >= img.palette.len())
        {
            return Err(format!(
                "a pixel is index {p}, but the palette makes a PLTE of {} entries",
                img.palette.len()
            ));
        }
    }
    Ok(img)
}

/// [`render`] and [`encode_png`].
pub fn render_png(
    payload: &SourcePayload,
    meta: &SourceMeta,
    palette: Option<&PaletteSource>,
) -> Result<Vec<u8>, String> {
    Ok(encode_png(&render(payload, meta, palette)?))
}

/// Write `grid` (local indices) into the image at tile (`tx`, `ty`) in palette row `row`, clipped
/// to the image edge.
fn blit(
    img: &mut [u8],
    (w, h): (usize, usize),
    (x0, y0): (usize, usize),
    grid: &IndexTile,
    row: usize,
    row_size: u16,
) -> Result<(), String> {
    for (i, &local) in grid.iter().enumerate() {
        let (x, y) = (x0 + i % 8, y0 + i / 8);
        if x < w && y < h {
            let global = row * row_size as usize + local as usize;
            img[y * w + x] = u8::try_from(global).map_err(|_| {
                format!("palette row {row} puts a pixel at PLTE index {global}, past 255")
            })?;
        }
    }
    Ok(())
}

/// The char tile `n` of `words` (`bpp * 4` words each).
fn char_tile(words: &[u16], n: usize, bpp: u8) -> Result<IndexTile, String> {
    let per = bpp as usize * 4;
    words
        .get(n * per..(n + 1) * per)
        .map(|w| unpack_planar(w, bpp))
        .ok_or_else(|| {
            format!(
                "a tile names char {n}, but the source holds {}",
                words.len() / per
            )
        })
}

fn flipped(mut t: IndexTile, flip_x: bool, flip_y: bool) -> IndexTile {
    if flip_x {
        t = crate::import::tiles::flip_h(&t);
    }
    if flip_y {
        t = crate::import::tiles::flip_v(&t);
    }
    t
}

/// The image with its PLTE, laid out as [`PaletteSource::from_sub_palettes`]: 4 and 16 colour
/// rows padded whole (entry 0 and unused slots black), a 256 row only `1 + colours` long (extended
/// to the highest pixel index), and rows enough for every pixel and at least one.
fn finish(
    (width, height): (usize, usize),
    pixels: Vec<u8>,
    mut subs: Vec<Vec<u16>>,
    row_size: u16,
) -> Result<IndexedImage, String> {
    if let Some((i, s)) = subs
        .iter()
        .enumerate()
        .find(|(_, s)| s.len() >= row_size as usize)
    {
        return Err(format!(
            "palette row {i} holds {} colours, more than a row of {row_size} beside its transparent entry 0",
            s.len()
        ));
    }
    let max_px = pixels.iter().copied().max().unwrap_or(0) as usize;
    let top = max_px / row_size as usize + 1;
    if subs.len() < top {
        subs.resize(top, vec![]);
    }
    // A 256 block is unpadded, so it must still reach the highest index a pixel uses.
    if row_size == 256 {
        if let Some(sub) = subs.first_mut() {
            if sub.len() < max_px {
                sub.resize(max_px, 0);
            }
        }
    }
    let palette = PaletteSource::from_sub_palettes(row_size, &subs)
        .rows
        .concat();
    if palette.len() > 256 {
        return Err(format!(
            "{} palette rows of {row_size} make a PLTE of {} entries; a PNG palette holds 256",
            subs.len(),
            palette.len()
        ));
    }
    Ok(IndexedImage {
        width: width as u32,
        height: height as u32,
        pixels,
        palette,
        row_size,
        extbg: false,
    })
}

fn render_bg(s: &BgSource, meta: &SourceMeta, row_size: u16) -> Result<IndexedImage, String> {
    let (w, h) = (meta.width.min(512) as usize, meta.height.min(512) as usize);
    let (cols, rows) = (w.div_ceil(8), h.div_ceil(8));
    let (map_cols, map_rows) = (
        if s.screen_size & 1 != 0 { 64 } else { 32 },
        if s.screen_size & 2 != 0 { 64 } else { 32 },
    );
    if s.tilemap_words.len() != bg_tilemap_len(s.screen_size) || cols > map_cols || rows > map_rows
    {
        return Err(format!(
            "a {w}x{h} px image does not fit the source's {map_cols}x{map_rows} tile map"
        ));
    }
    let mut pixels = vec![0u8; w * h];
    for ty in 0..rows {
        for tx in 0..cols {
            let sc = (ty / 32) * (map_cols / 32) + tx / 32;
            let word = s.tilemap_words[sc * 0x400 + (ty % 32) * 32 + tx % 32];
            let tile = char_tile(&s.char_words, (word & 0x3ff) as usize, s.bit_depth)?;
            let grid = flipped(tile, word & 0x4000 != 0, word & 0x8000 != 0);
            let row = ((word >> 10) & 7) as usize;
            blit(&mut pixels, (w, h), (tx * 8, ty * 8), &grid, row, row_size)?;
        }
    }
    finish((w, h), pixels, s.palettes.clone(), row_size)
}

fn render_sheet(s: &SheetSource, meta: &SourceMeta, row_size: u16) -> Result<IndexedImage, String> {
    let (w, h) = (meta.width as usize, meta.height as usize);
    let cols = w.div_ceil(8);
    let n = s.char_words.len() / (s.bit_depth as usize * 4);
    let mut pixels = vec![0u8; w * h];
    for k in 0..n {
        let row = match (&meta.cells, s.palettes.len()) {
            (Some(cells), _) => cells
                .get(k)
                .ok_or_else(|| format!("the sheet has {n} tiles but meta.cells lists {}", cells.len()))?
                .pal as usize,
            (None, 0 | 1) => 0,
            (None, p) => {
                return Err(format!(
                    "a sheet of {p} palettes needs meta.cells: each tile's palette row lives there, not in the payload"
                ))
            }
        };
        let grid = char_tile(&s.char_words, k, s.bit_depth)?;
        blit(
            &mut pixels,
            (w, h),
            (k % cols * 8, k / cols * 8),
            &grid,
            row,
            row_size,
        )?;
    }
    finish((w, h), pixels, s.palettes.clone(), row_size)
}

fn render_obj(s: &ObjSource, meta: &SourceMeta, row_size: u16) -> Result<IndexedImage, String> {
    let (w, h) = (meta.width as usize, meta.height as usize);
    let (cw, ch) = (s.cell_size as usize, s.cell_height as usize);
    let (cols, trows) = (w.div_ceil(8), h.div_ceil(8));
    let (nw, nh) = (cw / 8, ch / 8);
    let big_cols = cols.div_ceil(nw);
    let ncells = big_cols * trows.div_ceil(nh);
    if s.cells.len() != ncells {
        return Err(format!(
            "a {w}x{h} px sheet of {cw}x{ch} cells has {ncells} cells, but the source holds {}",
            s.cells.len()
        ));
    }
    let mut pixels = vec![0u8; w * h];
    for (k, cell) in s.cells.iter().enumerate() {
        let (x0, y0) = (k % big_cols * cw, k / big_cols * ch);
        let row = cell.pal as usize;
        if (nw, nh) == (1, 1) {
            let grid = flipped(
                char_tile(&s.char_words, cell.tile as usize, 4)?,
                cell.flip_x,
                cell.flip_y,
            );
            blit(&mut pixels, (w, h), (x0, y0), &grid, row, row_size)?;
        } else {
            // A block: its subtiles sit in the 16-wide name table at base + sy * 16 + sx.
            for sy in 0..nh {
                for sx in 0..nw {
                    let grid = char_tile(&s.char_words, cell.tile as usize + sy * 16 + sx, 4)?;
                    blit(
                        &mut pixels,
                        (w, h),
                        (x0 + sx * 8, y0 + sy * 8),
                        &grid,
                        row,
                        row_size,
                    )?;
                }
            }
        }
    }
    finish((w, h), pixels, s.palettes.clone(), row_size)
}

fn render_m7(s: &M7Source, meta: &SourceMeta) -> Result<IndexedImage, String> {
    if s.map.len() != s.tiles_w as usize * s.tiles_h as usize {
        return Err("the Mode 7 map does not match its tile grid".into());
    }
    let (w, h) = (
        (meta.width as usize).min(s.tiles_w as usize * 8),
        (meta.height as usize).min(s.tiles_h as usize * 8),
    );
    let mut pixels = vec![0u8; w * h];
    for ty in 0..h.div_ceil(8) {
        for tx in 0..w.div_ceil(8) {
            let id = s.map[ty * s.tiles_w as usize + tx] as usize;
            let tile = s.tiles.get(id).ok_or_else(|| {
                format!(
                    "map cell {tx},{ty} names tile {id}, but the source holds {}",
                    s.tiles.len()
                )
            })?;
            for (i, &p) in tile.iter().enumerate() {
                let (x, y) = (tx * 8 + i % 8, ty * 8 + i / 8);
                if x < w && y < h {
                    pixels[y * w + x] = p;
                }
            }
        }
    }
    let mut img = finish((w, h), pixels, vec![s.palette.clone()], 256)?;
    img.extbg = s.options.extbg;
    if img.extbg {
        // The one PLTE that is not its palette source (see `plte_of`).
        mirror_extbg(&mut img.palette);
    }
    Ok(img)
}

/// Is PLTE entry `k` one the art's rows make transparent: entry 0 of every row, and 128 under
/// Mode 7 EXTBG? The one statement of the rule, shared by the PNG writer and the keep-path reader.
pub fn is_transparent_entry(row_size: usize, extbg: bool, k: usize) -> bool {
    k.is_multiple_of(row_size.max(1)) || (extbg && k == 128)
}

/// The alpha of each PLTE entry: 0 for entry 0 of every row (and Mode 7 EXTBG's entry 128),
/// 255 elsewhere, trailing 255s trimmed (they are `tRNS`'s default).
pub fn alpha_of(len: usize, row_size: usize, extbg: bool) -> Vec<u8> {
    let mut alpha: Vec<u8> = (0..len)
        .map(|i| {
            if is_transparent_entry(row_size, extbg, i) {
                0
            } else {
                255
            }
        })
        .collect();
    while alpha.last() == Some(&255) {
        alpha.pop();
    }
    alpha
}

fn rgb_of(palette: &[u16]) -> Vec<u8> {
    palette
        .iter()
        .flat_map(|&c| {
            let [r, g, b, _] = unpack_rgb15(c);
            [r, g, b]
        })
        .collect()
}

/// The image as an 8-bit indexed PNG: PLTE the whole palette, `tRNS` alpha 0 on entry 0 of each
/// row. Panics if the palette is empty or longer than 256, or a pixel is out of it ([`render`]
/// never makes one).
pub fn encode_png(img: &IndexedImage) -> Vec<u8> {
    assert!(
        (1..=256).contains(&img.palette.len()),
        "a PNG palette holds 1 to 256 entries, got {}",
        img.palette.len()
    );
    assert!(
        img.pixels.len() == img.width as usize * img.height as usize
            && img.pixels.iter().all(|&p| (p as usize) < img.palette.len()),
        "encode_png: pixels must be width * height PLTE indices"
    );
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, img.width, img.height);
    enc.set_color(png::ColorType::Indexed);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_palette(rgb_of(&img.palette));
    let alpha = alpha_of(img.palette.len(), img.row_size as usize, img.extbg);
    if !alpha.is_empty() {
        enc.set_trns(alpha);
    }
    let mut w = enc.write_header().expect("PNG header into memory");
    w.write_image_data(&img.pixels)
        .expect("PNG data into memory");
    w.finish().expect("PNG end into memory");
    out
}

/// `png` with its PLTE and `tRNS` rebuilt from `pal` (`tRNS` as [`encode_png`] writes it, `extbg`
/// being Mode 7 EXTBG's extra transparent entry). Every other chunk, IDAT included, is copied byte
/// for byte, so the pixels are untouched. Refuses a PNG that is not indexed, or whose pixels or bit
/// depth the new palette does not cover.
pub fn replace_palette(png: &[u8], pal: &PaletteSource, extbg: bool) -> Result<Vec<u8>, String> {
    let DecodedPng::Indexed(ip) = decode_png(png)? else {
        return Err("not an indexed PNG: it has no PLTE to replace".into());
    };
    let plte = plte_of(pal, extbg);
    if !(1..=256).contains(&plte.len()) {
        return Err(format!(
            "the palette makes a PLTE of {} entries; a PNG palette holds 1 to 256",
            plte.len()
        ));
    }
    let depth = png[24]; // IHDR bit depth; decode_png has proved the header is there
    if plte.len() > 1 << depth {
        return Err(format!(
            "the PNG is {depth}-bit, which holds {} palette entries; the palette has {}",
            1u32 << depth,
            plte.len()
        ));
    }
    if let Some(&p) = ip
        .image
        .pixels
        .iter()
        .max()
        .filter(|&&p| p as usize >= plte.len())
    {
        return Err(format!(
            "a pixel is index {p}, but the palette holds {} entries",
            plte.len()
        ));
    }
    let alpha = alpha_of(plte.len(), pal.row_size as usize, extbg);
    let mut out = png[..8].to_vec();
    let mut pos = 8;
    while pos + 12 <= png.len() {
        let n = u32::from_be_bytes(png[pos..pos + 4].try_into().unwrap()) as usize;
        let end = (pos + 12 + n).min(png.len());
        match &png[pos + 4..pos + 8] {
            b"PLTE" => {
                push_chunk(&mut out, b"PLTE", &rgb_of(&plte));
                if !alpha.is_empty() {
                    push_chunk(&mut out, b"tRNS", &alpha);
                }
            }
            // tRNS is rebuilt above; bKGD and hIST index the old PLTE.
            b"tRNS" | b"bKGD" | b"hIST" => {}
            _ => out.extend_from_slice(&png[pos..end]),
        }
        pos = end;
    }
    Ok(out)
}

fn push_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = !0u32;
    for &b in kind.iter().chain(data) {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & (crc & 1).wrapping_neg());
        }
    }
    out.extend_from_slice(&(!crc).to_be_bytes());
}

/// An image as RGBA8 (any colour type; 16-bit stripped, palettes expanded with their `tRNS`).
pub(crate) fn rgba_of(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), String> {
    use png::ColorType::*;
    let mut dec = png::Decoder::new(bytes);
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = dec
        .read_info()
        .map_err(|e| format!("not a readable PNG: {e}"))?;
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r
        .next_frame(&mut buf)
        .map_err(|e| format!("not a readable PNG: {e}"))?;
    let px = &buf[..info.buffer_size()];
    let rgba = match info.color_type {
        Rgba => px.to_vec(),
        Rgb => px.chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        GrayscaleAlpha => px
            .chunks(2)
            .flat_map(|c| [c[0], c[0], c[0], c[1]])
            .collect(),
        Grayscale => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        Indexed => return Err("an indexed PNG did not expand".into()),
    };
    Ok((rgba, info.width, info.height))
}

/// Decode a PNG. An indexed one (1, 2, 4 or 8 bits) keeps its indices, PLTE and `tRNS` alphas; a
/// pixel past the PLTE is refused. Any other colour type is RGBA8.
pub fn decode_png(bytes: &[u8]) -> Result<DecodedPng, String> {
    let mut r = png::Decoder::new(bytes)
        .read_info()
        .map_err(|e| format!("not a readable PNG: {e}"))?;
    if r.info().color_type != png::ColorType::Indexed {
        let (rgba, width, height) = rgba_of(bytes)?;
        return Ok(DecodedPng::Rgba {
            rgba,
            width,
            height,
        });
    }
    let (width, height) = (r.info().width, r.info().height);
    let depth = r.info().bit_depth as usize;
    let plte = r
        .info()
        .palette
        .as_ref()
        .ok_or("an indexed PNG without a PLTE chunk")?
        .to_vec();
    let palette: Vec<u16> = plte
        .chunks_exact(3)
        .map(|c| crate::memory::rgb15(c[0], c[1], c[2]))
        .collect();
    let mut alpha = vec![255u8; palette.len()];
    if let Some(t) = r.info().trns.as_ref() {
        for (a, &v) in alpha.iter_mut().zip(t.iter()) {
            *a = v;
        }
    }
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r
        .next_frame(&mut buf)
        .map_err(|e| format!("not a readable PNG: {e}"))?;
    let (w, line) = (width as usize, info.line_size);
    let mask = ((1u16 << depth) - 1) as u8;
    let mut pixels = Vec::with_capacity(w * height as usize);
    for y in 0..height as usize {
        let row = &buf[y * line..(y + 1) * line];
        for x in 0..w {
            let bit = x * depth;
            let shift = 8 - depth - bit % 8;
            let p = (row[bit / 8] >> shift) & mask;
            if p as usize >= palette.len() {
                return Err(format!(
                    "pixel {x},{y} is index {p}, but the PLTE holds {} entries",
                    palette.len()
                ));
            }
            pixels.push(p);
        }
    }
    Ok(DecodedPng::Indexed(IndexedPng {
        image: IndexedImage {
            width,
            height,
            pixels,
            palette,
            row_size: 0,
            extbg: false,
        },
        alpha,
    }))
}
