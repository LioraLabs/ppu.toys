//! Tiled `.tmj` maps read into a plain map model the preview library and the cart compiler share.
//!
//! [`read_map`] is pure: the caller hands it the map text, a loader for external `.tsj`
//! tilesets, and a binder that answers what source a tileset image names. A tileset binds by its
//! image file name without path or extension (`art/tiles.png` binds the source `tiles`) to a
//! `sheet` source, or to an `m7` source for Mode 7.
//!
//! Cells are 8 px. A 16 px Tiled tile becomes a 2x2 group of cells; a flipped one swaps the
//! group's columns (horizontal) or rows (vertical) as well as flipping each cell. On a sheet
//! layer a cell is the SNES BG map word: char (bits 0-9, relative to the sheet's first char),
//! palette (10-12, relative to the sheet's first palette), priority (13, always 0 here), hflip
//! (14), vflip (15). On a Mode 7 layer it is the 8-bit tile number in the `m7` source. `None`
//! is an empty cell: a sheet has no reserved blank char, so char 0 is real art.
//!
//! Kinds come from Tiled's tile class (`class`, or `type` before Tiled 1.9). Each name is
//! numbered from 1 in order of first appearance in the tilesets (by `firstgid`, then tile id),
//! so painting never renumbers them. A cell with no class, or an empty cell, is kind 0.
//!
//! A tile with a Tiled animation (frames, each with a duration) plays by rewriting its own chars when a frame changes,
//! so every cell drawing it changes at once and no cell is written: see [`Animation`]. Durations round to video frames.
//!
//! Anything the hardware or this reader cannot honour is refused with what to change in Tiled.

use crate::source::{M7Source, SheetSource};
use base64::Engine as _;
use serde::Serialize;
use serde_json::{Map as JsonObject, Value};
use std::collections::BTreeMap;

/// What a tileset image binds to.
#[derive(Clone, Copy, Debug)]
pub enum Binding<'a> {
    /// A `sheet` source. `pals` is the palette of each char (the sheet import's cell
    /// assignment); an empty slice means palette 0 everywhere.
    Sheet {
        source: &'a SheetSource,
        pals: &'a [u8],
    },
    /// An `m7` source. An EXTBG source's priority mask lives in its pixels, so it carries through.
    M7(&'a M7Source),
}

/// Custom properties, by name, with their Tiled values.
pub type Properties = BTreeMap<String, Value>;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Map {
    /// Size in 8 px cells.
    pub width: usize,
    pub height: usize,
    /// Tiled's tile size in pixels: 8 or 16.
    pub tile_size: u32,
    /// Kind `n` is named `kinds[n - 1]`; kind 0 is unclassed.
    pub kinds: Vec<String>,
    pub layers: Vec<TileLayer>,
    pub object_layers: Vec<ObjectLayer>,
    pub properties: Properties,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TileLayer {
    pub name: String,
    /// The bound source's name; `None` for a layer with no tiles.
    pub source: Option<String>,
    pub mode7: bool,
    /// Row-major, `width * height`.
    pub cells: Vec<Option<u16>>,
    /// Row-major kind per cell.
    pub kinds: Vec<u8>,
    /// A sheet layer's tileset, by tile id: each tile's k x k BG words unflipped (row-major in
    /// the tile, `None` past the sheet's chars), what `w:set` writes. Empty for a Mode 7 layer
    /// or one with no tiles.
    #[serde(skip)]
    pub tiles: Vec<Option<u16>>,
    /// A sheet layer's kind per tile id (`tiles`' tile order).
    #[serde(skip)]
    pub tile_kinds: Vec<u8>,
    /// A sheet layer's Tiled tiles, row-major (`width / k` x `height / k`): each its tile id (`tiles`' order) with
    /// hflip in bit 14 and vflip in bit 15, as a BG word carries them; `None` for an empty tile. What `w:set`
    /// layers keep on a cartridge.
    #[serde(skip)]
    pub ids: Vec<Option<u16>>,
    pub properties: Properties,
    /// The tile animations of the layer's tileset (none for a layer with no tiles).
    pub animations: Vec<Animation>,
}

/// A Tiled tile animation on a sheet tileset. It plays by copying each frame's chars over the animated tile's own chars
/// in VRAM when the frame changes. Its clock is the frame counter `f` (from a scene's entry in a program of scenes): at
/// `f` it shows the frame whose span holds `f` modulo the frames' total.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Animation {
    /// The animated tile's chars, relative to the sheet's first char: one row of adjacent chars per 8 px of the tile
    /// (`rows`), so a 16 px tile is two rows of two.
    pub chars: Vec<u16>,
    /// Chars per row of `chars` (1 or 2): each row is one copy of adjacent chars.
    pub row: usize,
    /// Each frame: the chars it copies over `chars`, in the same order, and how many video frames it shows (1 or more).
    pub frames: Vec<(Vec<u16>, u16)>,
}

impl Animation {
    /// The frame shown at frame counter `f`.
    pub fn frame_at(&self, f: u64) -> usize {
        let total: u64 = self.frames.iter().map(|(_, d)| u64::from(*d)).sum();
        let mut at = f % total;
        for (i, (_, d)) in self.frames.iter().enumerate() {
            if at < u64::from(*d) {
                return i;
            }
            at -= u64::from(*d);
        }
        unreachable!("at < total")
    }
}

/// A Tiled frame duration in milliseconds as video frames at 60 a second, rounded, at least one.
pub fn duration_frames(ms: u32) -> u16 {
    ((u64::from(ms) * 60 + 500) / 1000).clamp(1, u64::from(u16::MAX)) as u16
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ObjectLayer {
    pub name: String,
    pub objects: Vec<Object>,
    pub properties: Properties,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Object {
    pub name: String,
    /// Tiled's object class (`class`, or `type` before 1.9).
    #[serde(rename = "type")]
    pub kind: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub props: Properties,
}

const FLIP_H: u32 = 0x8000_0000;
const FLIP_V: u32 = 0x4000_0000;
const FLIP_D: u32 = 0x2000_0000;
const ROTATE_HEX: u32 = 0x1000_0000;

struct Tileset<'a> {
    firstgid: u32,
    name: String,
    columns: u32,
    tilecount: u32,
    /// Chars per sheet row: the image width in 8 px cells.
    image_cells_w: u32,
    source: String,
    binding: Binding<'a>,
    /// Kind per tile id.
    kinds: BTreeMap<u32, u8>,
    animations: Vec<Animation>,
}

/// Read a `.tmj` map. `load_tsj` returns an external tileset's text by the `source` path the
/// map names; `bind` returns the binding for a source name.
pub fn read_map<'a>(
    text: &str,
    load_tsj: impl Fn(&str) -> Result<String, String>,
    bind: impl Fn(&str) -> Option<Binding<'a>>,
) -> Result<Map, String> {
    if text.trim_start().starts_with('<') {
        return Err("this is a .tmx (XML) map: save it as JSON (.tmj) in Tiled".into());
    }
    let map: Value =
        serde_json::from_str(text).map_err(|e| format!("map is not valid JSON: {e}"))?;
    let map = map.as_object().ok_or("map is not a JSON object")?;

    match str_of(map, "orientation") {
        "orthogonal" => {}
        other => {
            return Err(format!(
                "{other} maps are not supported: set Map > Orientation to Orthogonal in Tiled"
            ))
        }
    }
    if map
        .get("infinite")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err("infinite maps are not supported: untick Map > Infinite in Tiled".into());
    }
    let tile_size = uint(map, "tilewidth", "map")?;
    if tile_size != uint(map, "tileheight", "map")? || !matches!(tile_size, 8 | 16) {
        return Err(format!(
            "map tiles are {tile_size}x{} px: set the tile size to 8x8 or 16x16 in Tiled",
            uint(map, "tileheight", "map")?
        ));
    }
    let (tiles_w, tiles_h) = (uint(map, "width", "map")?, uint(map, "height", "map")?);
    let k = (tile_size / 8) as usize;
    let (width, height) = (tiles_w as usize * k, tiles_h as usize * k);

    let mut kinds: Vec<String> = Vec::new();
    let mut tilesets: Vec<Tileset<'a>> = Vec::new();
    let mut sets: Vec<(u32, JsonObject<String, Value>)> = Vec::new();
    for ts in array(map, "tilesets", "map")? {
        let ts = ts.as_object().ok_or("a tileset is not a JSON object")?;
        let firstgid = uint(ts, "firstgid", "tileset")?;
        let body = match ts.get("source").and_then(Value::as_str) {
            Some(path) if path.ends_with(".tsx") => {
                return Err(format!(
                    "tileset {path} is XML: export it as JSON (.tsj) in Tiled and point the map at that"
                ))
            }
            Some(path) => {
                let text = load_tsj(path).map_err(|e| format!("tileset {path}: {e}"))?;
                let v: Value = serde_json::from_str(&text)
                    .map_err(|e| format!("tileset {path} is not valid JSON: {e}"))?;
                v.as_object().ok_or(format!("tileset {path} is not a JSON object"))?.clone()
            }
            None => ts.clone(),
        };
        sets.push((firstgid, body));
    }
    sets.sort_by_key(|(g, _)| *g);
    for (firstgid, ts) in &sets {
        tilesets.push(read_tileset(*firstgid, ts, tile_size, &bind, &mut kinds)?);
    }

    let mut layers = Vec::new();
    let mut object_layers = Vec::new();
    for layer in array(map, "layers", "map")? {
        let layer = layer.as_object().ok_or("a layer is not a JSON object")?;
        let name = str_of(layer, "name").to_string();
        match str_of(layer, "type") {
            "tilelayer" => layers.push(read_tile_layer(
                layer, name, tiles_w, tiles_h, k, &tilesets,
            )?),
            "objectgroup" => object_layers.push(read_object_layer(layer, name)?),
            "group" => {
                return Err(format!(
                    "layer {name} is a group layer: move its layers out of the group in Tiled"
                ))
            }
            "imagelayer" => {
                return Err(format!(
                    "layer {name} is an image layer: draw it as a tile layer, or use a bg source instead"
                ))
            }
            other => return Err(format!("layer {name} has unknown type {other:?}")),
        }
    }

    Ok(Map {
        width,
        height,
        tile_size,
        kinds,
        layers,
        object_layers,
        properties: properties(map)?,
    })
}

fn read_tileset<'a>(
    firstgid: u32,
    ts: &JsonObject<String, Value>,
    tile_size: u32,
    bind: &impl Fn(&str) -> Option<Binding<'a>>,
    kinds: &mut Vec<String>,
) -> Result<Tileset<'a>, String> {
    let name = str_of(ts, "name").to_string();
    let image = ts.get("image").and_then(Value::as_str).ok_or(format!(
        "tileset {name} is a collection of images: make it a single-image tileset in Tiled"
    ))?;
    let (tw, th) = (
        uint(ts, "tilewidth", "tileset")?,
        uint(ts, "tileheight", "tileset")?,
    );
    if tw != tile_size || th != tile_size {
        return Err(format!(
            "tileset {name} has {tw}x{th} px tiles but the map has {tile_size}x{tile_size}: make them match in Tiled"
        ));
    }
    if uint_or0(ts, "margin")? != 0 || uint_or0(ts, "spacing")? != 0 {
        return Err(format!(
            "tileset {name} has a margin or spacing: set both to 0 in Tiled (sources have no gaps between cells)"
        ));
    }
    let columns = uint(ts, "columns", "tileset")?;
    let tilecount = uint(ts, "tilecount", "tileset")?;
    let (iw, ih) = (
        uint(ts, "imagewidth", "tileset")?,
        uint(ts, "imageheight", "tileset")?,
    );
    if iw % 8 != 0 || ih % 8 != 0 || columns == 0 || columns != iw / tile_size {
        return Err(format!(
            "tileset {name}: image {iw}x{ih} px does not divide into {tile_size} px tiles in {columns} columns"
        ));
    }
    let file = image.rsplit(['/', '\\']).next().unwrap_or(image);
    let source = file
        .rsplit_once('.')
        .map_or(file, |(stem, _)| stem)
        .to_string();
    let binding = bind(&source).ok_or(format!(
        "tileset {name}: image {image} matches no sheet or m7 source: add a source named {source}, or rename the image in Tiled"
    ))?;
    match binding {
        Binding::M7(m7) => {
            if m7.options.overflowed_tiles > 0 {
                return Err(format!(
                    "tileset {name}: m7 source {source} has {} distinct 8x8 tiles, and a Mode 7 plane holds 256: reuse tiles in the image until it has 256 or fewer",
                    m7.options.overflowed_tiles
                ));
            }
            if u32::from(m7.tiles_w) != iw / 8 || u32::from(m7.tiles_h) != ih / 8 {
                return Err(format!(
                    "tileset {name}: image is {}x{} cells but m7 source {source} is {}x{}: point the tileset at the image the source was made from",
                    iw / 8, ih / 8, m7.tiles_w, m7.tiles_h
                ));
            }
        }
        Binding::Sheet { source: sheet, .. } => {
            // One char per 8 px cell, so a size mismatch would shift every row onto the wrong chars.
            let chars = sheet.char_words.len() / (sheet.bit_depth as usize * 4);
            let cells = (iw / 8 * (ih / 8)) as usize;
            if chars != cells.min(1024) {
                return Err(format!(
                    "tileset {name}: image is {iw}x{ih} px ({cells} cells) but sheet {source} has {chars} chars: point the tileset at the image the source was made from"
                ));
            }
        }
    }

    let mut tile_kinds = BTreeMap::new();
    let mut tiles: Vec<&JsonObject<String, Value>> = match ts.get("tiles") {
        None => Vec::new(),
        Some(Value::Array(a)) => a
            .iter()
            .map(|t| {
                t.as_object()
                    .ok_or(format!("tileset {name}: a tile is not a JSON object"))
            })
            .collect::<Result<_, _>>()?,
        Some(_) => return Err(format!("tileset {name}: tiles is not an array")),
    };
    tiles.sort_by_key(|t| t.get("id").and_then(Value::as_u64));
    let k = (tile_size / 8) as usize;
    // A tile's chars, row by row, checked against the sheet; a sheet's palette per char, for the frames' check.
    let tile_chars = |id: u32| -> Result<Vec<u16>, String> {
        if id >= tilecount {
            return Err(format!(
                "tileset {name}: an animation frame shows tile {id}, past the tileset's {tilecount} tiles"
            ));
        }
        let (col, row) = ((id % columns) as usize, (id / columns) as usize);
        let mut out = Vec::new();
        for dy in 0..k {
            for dx in 0..k {
                out.push(((row * k + dy) * (iw / 8) as usize + col * k + dx) as u16);
            }
        }
        if let Binding::Sheet { source: sheet, .. } = binding {
            let chars = sheet.char_words.len() / (sheet.bit_depth as usize * 4);
            if let Some(c) = out.iter().find(|&&c| c as usize >= chars.min(1024)) {
                return Err(format!(
                    "tileset {name}: tile {id} is char {c}, past the {chars} chars of sheet {source}: the tileset image is larger than the source"
                ));
            }
        }
        Ok(out)
    };
    let mut animations = Vec::new();
    for t in tiles {
        let id = uint(t, "id", "tile")?;
        if let Some(frames) = t.get("animation") {
            let frames = frames.as_array().ok_or(format!(
                "tileset {name}: tile {id}'s animation is not an array"
            ))?;
            if let Binding::M7(_) = binding {
                if !frames.is_empty() {
                    return Err(format!(
                        "tileset {name} animates tile {id}, but it draws the Mode 7 plane, whose tiles are shared: animate only background tilesets"
                    ));
                }
            }
            let chars = tile_chars(id)?;
            let pal_of = |c: u16| match binding {
                Binding::Sheet { pals, .. } => pals.get(c as usize).copied().unwrap_or(0),
                Binding::M7(_) => 0,
            };
            let mut out = Vec::new();
            for fr in frames {
                let fr = fr.as_object().ok_or(format!(
                    "tileset {name}: a frame of tile {id}'s animation is not a JSON object"
                ))?;
                let shown = uint(fr, "tileid", "animation frame")?;
                let from = tile_chars(shown)?;
                if let Some((a, b)) = chars
                    .iter()
                    .zip(&from)
                    .find(|(a, b)| pal_of(**a) != pal_of(**b))
                {
                    return Err(format!(
                        "tileset {name}: tile {id} animates through tile {shown}, whose colours are palette {} where tile {id}'s are palette {}: a frame keeps the tile's palette, so draw its frames in the same colours",
                        pal_of(*b), pal_of(*a)
                    ));
                }
                out.push((
                    from,
                    duration_frames(uint(fr, "duration", "animation frame")?),
                ));
            }
            if !out.is_empty() {
                animations.push(Animation {
                    chars,
                    row: k,
                    frames: out,
                });
            }
        }
        let class = class_of(t);
        if class.is_empty() {
            continue;
        }
        let n = match kinds.iter().position(|k| k == class) {
            Some(i) => i + 1,
            None => {
                kinds.push(class.to_string());
                kinds.len()
            }
        };
        if n > 255 {
            return Err("more than 255 tile classes: merge some in Tiled".into());
        }
        tile_kinds.insert(id, n as u8);
    }

    Ok(Tileset {
        firstgid,
        name,
        columns,
        tilecount,
        image_cells_w: iw / 8,
        source,
        binding,
        kinds: tile_kinds,
        animations,
    })
}

fn read_tile_layer(
    layer: &JsonObject<String, Value>,
    name: String,
    tiles_w: u32,
    tiles_h: u32,
    k: usize,
    tilesets: &[Tileset],
) -> Result<TileLayer, String> {
    if layer.contains_key("chunks") {
        return Err(format!(
            "layer {name} is chunked: untick Map > Infinite in Tiled"
        ));
    }
    if layer.get("offsetx").and_then(Value::as_f64).unwrap_or(0.) != 0.
        || layer.get("offsety").and_then(Value::as_f64).unwrap_or(0.) != 0.
    {
        return Err(format!(
            "layer {name} has an offset: set its offset to 0,0 in Tiled"
        ));
    }
    if uint(layer, "width", "layer")? != tiles_w || uint(layer, "height", "layer")? != tiles_h {
        return Err(format!(
            "layer {name} is not the size of the map: resize it in Tiled"
        ));
    }
    let gids = layer_gids(layer, &name)?;
    if gids.len() != (tiles_w * tiles_h) as usize {
        return Err(format!(
            "layer {name} has {} tiles, expected {}x{} = {}",
            gids.len(),
            tiles_w,
            tiles_h,
            tiles_w * tiles_h
        ));
    }

    let (width, height) = (tiles_w as usize * k, tiles_h as usize * k);
    let mut cells = vec![None; width * height];
    let mut kinds = vec![0u8; width * height];
    let mut ids = vec![None; gids.len()];
    let mut used: Option<&Tileset> = None;
    for (i, &raw) in gids.iter().enumerate() {
        let (tx, ty) = (i % tiles_w as usize, i / tiles_w as usize);
        if raw & (FLIP_D | ROTATE_HEX) != 0 {
            return Err(format!(
                "layer {name}, tile {tx},{ty} is rotated or flipped diagonally: the SNES has no such flip, so use only horizontal and vertical flips"
            ));
        }
        let gid = raw & !(FLIP_H | FLIP_V | FLIP_D | ROTATE_HEX);
        if gid == 0 {
            continue;
        }
        let ts = tilesets
            .iter()
            .rev()
            .find(|t| t.firstgid <= gid)
            .filter(|t| gid - t.firstgid < t.tilecount)
            .ok_or(format!(
                "layer {name}, tile {tx},{ty}: gid {gid} is in no tileset"
            ))?;
        match used {
            Some(u) if u.firstgid != ts.firstgid => {
                return Err(format!(
                    "layer {name} draws from tilesets {} and {}: a layer draws from one tileset, so split it into two layers",
                    u.name, ts.name
                ))
            }
            _ => used = Some(ts),
        }
        let (hflip, vflip) = (raw & FLIP_H != 0, raw & FLIP_V != 0);
        let id = gid - ts.firstgid;
        let (col, row) = ((id % ts.columns) as usize, (id / ts.columns) as usize);
        let kind = ts.kinds.get(&id).copied().unwrap_or(0);
        ids[i] = Some(id as u16 | (hflip as u16) << 14 | (vflip as u16) << 15);
        for dy in 0..k {
            for dx in 0..k {
                let sx = if hflip { k - 1 - dx } else { dx };
                let sy = if vflip { k - 1 - dy } else { dy };
                let char = (row * k + sy) * ts.image_cells_w as usize + col * k + sx;
                let word = match ts.binding {
                    Binding::Sheet { source, pals } => {
                        let chars = source.char_words.len() / (source.bit_depth as usize * 4);
                        if char >= chars.min(1024) {
                            return Err(format!(
                                "layer {name}, tile {tx},{ty}: char {char} is past the {chars} chars of sheet {}: the tileset image is larger than the source",
                                ts.source
                            ));
                        }
                        let pal = pals.get(char).copied().unwrap_or(0) as u16 & 7;
                        char as u16 | pal << 10 | (hflip as u16) << 14 | (vflip as u16) << 15
                    }
                    Binding::M7(m7) => {
                        if hflip || vflip {
                            return Err(format!(
                                "layer {name}, tile {tx},{ty} is flipped: Mode 7 has no tile flips, so draw it unflipped in Tiled"
                            ));
                        }
                        *m7.map.get(char).ok_or(format!(
                            "layer {name}, tile {tx},{ty}: tile {id} is outside the image of m7 source {}",
                            ts.source
                        ))? as u16
                    }
                };
                let at = (ty * k + dy) * width + tx * k + dx;
                cells[at] = Some(word);
                kinds[at] = kind;
            }
        }
    }

    // The tileset by tile id, as a cell of each would draw unflipped (w:set's tiles).
    let (mut tiles, mut tile_kinds) = (vec![], vec![]);
    if let Some(
        ts @ Tileset {
            binding: Binding::Sheet { source, pals },
            ..
        },
    ) = used
    {
        let chars = (source.char_words.len() / (source.bit_depth as usize * 4)).min(1024);
        for id in 0..ts.tilecount {
            let (col, row) = ((id % ts.columns) as usize, (id / ts.columns) as usize);
            for dy in 0..k {
                for dx in 0..k {
                    let char = (row * k + dy) * ts.image_cells_w as usize + col * k + dx;
                    let pal = pals.get(char).copied().unwrap_or(0) as u16 & 7;
                    tiles.push((char < chars).then_some(char as u16 | pal << 10));
                }
            }
            tile_kinds.push(ts.kinds.get(&id).copied().unwrap_or(0));
        }
    }

    Ok(TileLayer {
        name,
        tiles,
        tile_kinds,
        ids,
        source: used.map(|t| t.source.clone()),
        mode7: used.is_some_and(|t| matches!(t.binding, Binding::M7(_))),
        cells,
        kinds,
        properties: properties(layer)?,
        animations: used.map(|t| t.animations.clone()).unwrap_or_default(),
    })
}

/// The part of tile layer `li` of `map` that a wrapping bind (`w:bind(name, bg[n], {wrap = true})`) repeats: the
/// columns up to its last with a tile and the rows up to its last with a tile, as (columns, rows); `None` when the
/// layer has no tile. The preview's strips and the cartridge's baked words both repeat this.
pub fn painted(map: &Map, li: usize) -> Option<(usize, usize)> {
    let cells = map.layers[li]
        .cells
        .iter()
        .enumerate()
        .filter(|(_, c)| c.is_some());
    cells.fold(None, |a, (i, _)| {
        let (x, y) = (i % map.width + 1, i / map.width + 1);
        Some(a.map_or((x, y), |(w, h): (usize, usize)| (w.max(x), h.max(y))))
    })
}

/// A sheet layer's cells as BG map words, an empty cell drawn as the sheet's first blank char
/// (all pixels transparent), palette 0. A layer with empty cells needs one: a sheet has no
/// reserved blank, so the art must carry it.
pub fn sheet_words(layer: &TileLayer, sheet: &SheetSource) -> Result<Vec<u16>, String> {
    let per = sheet.bit_depth as usize * 4;
    let blank = sheet
        .char_words
        .chunks(per)
        .position(|c| c.iter().all(|&w| w == 0));
    layer
        .cells
        .iter()
        .map(|c| match (c, blank) {
            (Some(w), _) => Ok(*w),
            (None, Some(b)) => Ok(b as u16),
            (None, None) => Err(format!(
                "layer {} has empty cells but sheet {} has no blank tile to draw them with: add a fully transparent 8x8 tile to the tileset image",
                layer.name,
                layer.source.as_deref().unwrap_or("?")
            )),
        })
        .collect()
}

/// The Mode 7 plane's map side: a layer of at most 128x128 cells as 128x128 row-major tile numbers
/// (`m7.map`'s low bytes), the map at the top left; a bigger layer (one that streams through the
/// plane) as its own width x height row-major tile numbers. Empty cells, and the plane past a
/// smaller map, draw the source's first blank tile (all pixels 0, transparent with or without
/// EXTBG), so nothing of the tileset image's own map shows through.
pub fn m7_plane(
    layer: &TileLayer,
    width: usize,
    height: usize,
    m7: &M7Source,
) -> Result<Vec<u8>, String> {
    let (stride, rows) = if width > 128 || height > 128 {
        (width, height)
    } else {
        (128, 128)
    };
    let blank = m7.tiles.iter().position(|t| t.iter().all(|&p| p == 0));
    if blank.is_none() && layer.cells.iter().any(Option::is_none) {
        return Err(format!(
            "layer {} has empty cells but m7 source {} has no blank tile to draw them with: add a fully transparent 8x8 tile to the tileset image",
            layer.name,
            layer.source.as_deref().unwrap_or("?")
        ));
    }
    // shortcut: with no blank tile (a map with no empty cells) the plane past the map keeps tile 0.
    let blank = blank.unwrap_or(0) as u8;
    let mut plane = vec![blank; stride * rows];
    for y in 0..height {
        for x in 0..width {
            if let Some(t) = layer.cells[y * width + x] {
                plane[y * stride + x] = t as u8;
            }
        }
    }
    Ok(plane)
}

/// A tile layer's raw gids, from CSV (a JSON array, or a CSV string) or plain base64.
fn layer_gids(layer: &JsonObject<String, Value>, name: &str) -> Result<Vec<u32>, String> {
    let compression = str_of(layer, "compression");
    if !compression.is_empty() {
        return Err(format!(
            "layer {name} is {compression}-compressed: set the layer format to CSV in Tiled (Map > Map Properties > Tile Layer Format)"
        ));
    }
    let bad = || format!("layer {name}: data is not a list of tile numbers");
    match (str_of(layer, "encoding"), layer.get("data")) {
        ("" | "csv", Some(Value::Array(a))) => a
            .iter()
            .map(|v| {
                v.as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or_else(bad)
            })
            .collect(),
        ("" | "csv", Some(Value::String(s))) => s
            .split(',')
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(|t| t.parse::<u32>().map_err(|_| bad()))
            .collect(),
        ("base64", Some(Value::String(s))) => {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(s.trim())
                .map_err(|e| format!("layer {name}: base64 data does not decode: {e}"))?;
            if bytes.len() % 4 != 0 {
                return Err(format!(
                    "layer {name}: base64 data is not whole 32-bit tiles"
                ));
            }
            Ok(bytes
                .chunks_exact(4)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect())
        }
        (enc, _) if !matches!(enc, "" | "csv" | "base64") => Err(format!(
            "layer {name} has encoding {enc:?}: set the layer format to CSV in Tiled"
        )),
        _ => Err(bad()),
    }
}

fn read_object_layer(
    layer: &JsonObject<String, Value>,
    name: String,
) -> Result<ObjectLayer, String> {
    let objects = array(layer, "objects", "object layer")?
        .iter()
        .map(|o| {
            let o = o.as_object().ok_or(format!("layer {name}: an object is not a JSON object"))?;
            let oname = str_of(o, "name").to_string();
            for (key, what) in [
                ("gid", "a tile object"),
                ("polygon", "a polygon"),
                ("polyline", "a polyline"),
                ("text", "a text object"),
                ("template", "a template instance"),
            ] {
                if o.contains_key(key) {
                    return Err(format!(
                        "layer {name}, object {oname:?} is {what}: use a rectangle or point object instead"
                    ));
                }
            }
            let num = |key: &str| o.get(key).and_then(Value::as_f64).unwrap_or(0.);
            if num("rotation") != 0. {
                return Err(format!(
                    "layer {name}, object {oname:?} is rotated: set its rotation to 0 in Tiled"
                ));
            }
            Ok(Object {
                name: oname,
                kind: class_of(o).to_string(),
                x: num("x"),
                y: num("y"),
                w: num("width"),
                h: num("height"),
                props: properties(o)?,
            })
        })
        .collect::<Result<_, String>>()?;
    Ok(ObjectLayer {
        name,
        objects,
        properties: properties(layer)?,
    })
}

fn properties(o: &JsonObject<String, Value>) -> Result<Properties, String> {
    let Some(list) = o.get("properties") else {
        return Ok(Properties::new());
    };
    list.as_array()
        .ok_or("properties is not an array")?
        .iter()
        .map(|p| {
            let name = p
                .get("name")
                .and_then(Value::as_str)
                .ok_or("a property has no name")?;
            Ok((
                name.to_string(),
                p.get("value").cloned().unwrap_or(Value::Null),
            ))
        })
        .collect()
}

fn class_of(o: &JsonObject<String, Value>) -> &str {
    match str_of(o, "class") {
        "" => str_of(o, "type"),
        c => c,
    }
}

fn str_of<'v>(o: &'v JsonObject<String, Value>, key: &str) -> &'v str {
    o.get(key).and_then(Value::as_str).unwrap_or("")
}

fn uint(o: &JsonObject<String, Value>, key: &str, what: &str) -> Result<u32, String> {
    o.get(key)
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or(format!("{what} has no {key}"))
}

fn uint_or0(o: &JsonObject<String, Value>, key: &str) -> Result<u32, String> {
    if o.contains_key(key) {
        uint(o, key, "tileset")
    } else {
        Ok(0)
    }
}

fn array<'v>(
    o: &'v JsonObject<String, Value>,
    key: &str,
    what: &str,
) -> Result<&'v Vec<Value>, String> {
    o.get(key)
        .and_then(Value::as_array)
        .ok_or(format!("{what} has no {key} list"))
}
