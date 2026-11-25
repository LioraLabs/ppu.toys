//! Palette rows of indexed art, as pure functions over index bytes: what an editor asks `ppu art
//! rows` and `ppu art palette set` for. Indices are `row * row_size + local`; a unit (a tile or an
//! obj cell) draws all its non-zero pixels from one row, and local 0 is every row's transparent
//! entry. Nothing here reads a file: the CLI passes the pixels and the palette, and applies the result.

/// Index bytes with their geometry: `unit` is the tile or cell in pixels, `rs` the row size.
pub struct Art<'a> {
    pub pixels: &'a [u8],
    pub width: usize,
    pub height: usize,
    pub unit: (usize, usize),
    pub rs: usize,
}

impl Art<'_> {
    fn units(&self) -> (usize, usize) {
        (
            self.width.div_ceil(self.unit.0),
            self.height.div_ceil(self.unit.1),
        )
    }

    /// The unit's pixel indices (into `pixels`).
    fn unit_pixels(&self, ux: usize, uy: usize) -> impl Iterator<Item = usize> + '_ {
        let (x0, y0) = (ux * self.unit.0, uy * self.unit.1);
        let (x1, y1) = (
            (x0 + self.unit.0).min(self.width),
            (y0 + self.unit.1).min(self.height),
        );
        (y0..y1).flat_map(move |y| (x0..x1).map(move |x| y * self.width + x))
    }

    fn what(&self) -> &'static str {
        if self.unit == (8, 8) {
            "tile"
        } else {
            "cell"
        }
    }

    fn check(&self) -> Result<(), String> {
        if self.rs == 256 {
            return Err("this art is one 256-colour block, so it has no palette rows".into());
        }
        if self.pixels.len() != self.width * self.height {
            return Err("the pixels do not fill width x height".into());
        }
        Ok(())
    }
}

/// `art` with every unit the pixel rectangle `(x, y, w, h)` touches moved to palette `row`
/// (`p -> row * rs + p % rs`); `rows` is how many rows the palette has.
pub fn set_row(
    art: &Art,
    rows: usize,
    row: usize,
    (x, y, w, h): (usize, usize, usize, usize),
) -> Result<Vec<u8>, String> {
    art.check()?;
    if row >= rows {
        return Err(format!("row {row} is past the palette's {rows} rows"));
    }
    let inside = |at: usize, len: usize, max: usize| at.checked_add(len).is_some_and(|e| e <= max);
    if w == 0 || h == 0 || !inside(x, w, art.width) || !inside(y, h, art.height) {
        return Err(format!(
            "the rectangle {x},{y},{w},{h} is not inside the {}x{} image",
            art.width, art.height
        ));
    }
    let mut out = art.pixels.to_vec();
    for uy in y / art.unit.1..=(y + h - 1) / art.unit.1 {
        for ux in x / art.unit.0..=(x + w - 1) / art.unit.0 {
            for i in art.unit_pixels(ux, uy) {
                out[i] = (row * art.rs) as u8 + art.pixels[i] % art.rs as u8;
            }
        }
    }
    Ok(out)
}

/// What [`fix_rows`] did.
pub struct Fixed {
    pub pixels: Vec<u8>,
    pub moved: usize,
    /// One line per unit with pixels that had no same-colour entry in its row.
    pub left: Vec<String>,
}

/// Each unit's row is the one most of its non-transparent pixels use (ties: the lowest); a pixel
/// of another row moves to the first entry of that row with exactly its colour. No such entry:
/// it stays, and the unit is listed. `plte` is the art's colours, one per index.
pub fn fix_rows(art: &Art, plte: &[u16]) -> Result<Fixed, String> {
    art.check()?;
    if let Some(&p) = art.pixels.iter().find(|&&p| p as usize >= plte.len()) {
        return Err(format!(
            "a pixel is index {p}, but the palette holds {} entries",
            plte.len()
        ));
    }
    let (rs, mut out) = (art.rs, art.pixels.to_vec());
    let (mut moved, mut left) = (0, vec![]);
    let (cols, rows) = art.units();
    for uy in 0..rows {
        for ux in 0..cols {
            let mut count = vec![0usize; 256usize.div_ceil(rs)];
            for i in art.unit_pixels(ux, uy) {
                if !(art.pixels[i] as usize).is_multiple_of(rs) {
                    count[art.pixels[i] as usize / rs] += 1;
                }
            }
            let (best, top) =
                count
                    .iter()
                    .enumerate()
                    .fold((0, 0), |a, (r, &n)| if n > a.1 { (r, n) } else { a });
            if top == 0 {
                continue;
            }
            let mut stuck = 0;
            for i in art.unit_pixels(ux, uy) {
                let p = art.pixels[i] as usize;
                if p.is_multiple_of(rs) || p / rs == best {
                    continue;
                }
                let found = (1..rs).find(|&l| plte.get(best * rs + l) == Some(&plte[p]));
                match found {
                    Some(l) => {
                        out[i] = (best * rs + l) as u8;
                        moved += 1;
                    }
                    None => stuck += 1,
                }
            }
            if stuck > 0 {
                left.push(format!(
                    "{} {ux},{uy}: {stuck} pixels have no colour in row {best}",
                    art.what()
                ));
            }
        }
    }
    Ok(Fixed {
        pixels: out,
        moved,
        left,
    })
}

/// `#rrggbb` of an RGB555 colour, as an editor shows it.
pub fn hex(c: u16) -> String {
    let [r, g, b, _] = crate::memory::unpack_rgb15(c);
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// `art` repainted for another palette: each unit moves to the lowest row of `new` (flat, `rs`
/// entries a row) holding an exactly equal colour for every non-transparent pixel, and keeps its
/// pixels' colours; its local-0 pixels go to that row's entry 0. `old` is the art's current
/// colours, one per index. Any unit with no such row refuses the whole switch, with a line for
/// each colour `new` lacks (and the units that use it) and each unit whose colours no one row holds.
pub fn switch_palette(art: &Art, old: &[u16], new: &[u16]) -> Result<Vec<u8>, Vec<String>> {
    art.check().map_err(|e| vec![e])?;
    if let Some(&p) = art.pixels.iter().find(|&&p| p as usize >= old.len()) {
        return Err(vec![format!(
            "a pixel is index {p}, but the palette holds {} entries",
            old.len()
        )]);
    }
    let rs = art.rs;
    let new_rows = new.len().div_ceil(rs);
    let mut out = art.pixels.to_vec();
    let (mut missing, mut split): (Vec<(u16, Vec<String>)>, Vec<String>) = (vec![], vec![]);
    let (cols, rows) = art.units();
    for uy in 0..rows {
        for ux in 0..cols {
            let at = format!("{} {ux},{uy}", art.what());
            let mut colours: Vec<u16> = vec![];
            for i in art.unit_pixels(ux, uy) {
                let p = art.pixels[i] as usize;
                if !p.is_multiple_of(rs) && !colours.contains(&old[p]) {
                    colours.push(old[p]);
                }
            }
            let entry = |r: usize, c: u16| (1..rs).find(|&l| new.get(r * rs + l) == Some(&c));
            let Some(row) = (0..new_rows).find(|&r| colours.iter().all(|&c| entry(r, c).is_some()))
            else {
                let mut lacking = false;
                for &c in colours
                    .iter()
                    .filter(|&&c| !(0..new_rows).any(|r| entry(r, c).is_some()))
                {
                    lacking = true;
                    match missing.iter_mut().find(|m| m.0 == c) {
                        Some(m) => m.1.push(at.clone()),
                        None => missing.push((c, vec![at.clone()])),
                    }
                }
                if !lacking {
                    split.push(format!(
                        "{at}: no one row of the palette holds all its colours"
                    ));
                }
                continue;
            };
            for i in art.unit_pixels(ux, uy) {
                let p = art.pixels[i] as usize;
                // Keep a pixel's own local index when the new row has its colour there: duplicate
                // colours are deliberate (cycling, separately editable "same" colours).
                out[i] = (row * rs
                    + if p.is_multiple_of(rs) {
                        0
                    } else if new.get(row * rs + p % rs) == Some(&old[p]) {
                        p % rs
                    } else {
                        entry(row, old[p]).expect("row holds it")
                    }) as u8;
            }
        }
    }
    if missing.is_empty() && split.is_empty() {
        return Ok(out);
    }
    let mut lines: Vec<String> = missing
        .into_iter()
        .map(|(c, units)| {
            format!(
                "{}: no exact match in the palette ({})",
                hex(c),
                units.join("; ")
            )
        })
        .collect();
    lines.extend(split);
    Err(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn art(pixels: &[u8], width: usize, height: usize) -> Art<'_> {
        Art {
            pixels,
            width,
            height,
            unit: (8, 8),
            rs: 4,
        }
    }

    // Two 4-colour rows: row 1 repeats row 0's colours 1 and 2 and adds 9.
    const PLTE: [u16; 8] = [0, 1, 2, 3, 0, 2, 1, 9];

    #[test]
    fn set_row_moves_every_unit_the_rect_touches_and_no_other() {
        let mut px = vec![0u8; 16 * 8];
        px[0] = 1;
        px[8] = 2;
        let out = set_row(&art(&px, 16, 8), 2, 1, (7, 0, 2, 1)).unwrap();
        assert_eq!((out[0], out[8], out[1]), (5, 6, 4));
        let out = set_row(&art(&px, 16, 8), 2, 1, (0, 0, 1, 1)).unwrap();
        assert_eq!((out[0], out[8], out[1]), (5, 2, 4));
        assert!(set_row(&art(&px, 16, 8), 2, 2, (0, 0, 1, 1))
            .unwrap_err()
            .contains("past"));
        assert!(set_row(&art(&px, 16, 8), 2, 1, (15, 0, 2, 1))
            .unwrap_err()
            .contains("not inside"));
    }

    #[test]
    fn a_rect_that_overflows_is_an_error_not_a_panic() {
        let px = vec![0u8; 64];
        let e = set_row(&art(&px, 8, 8), 2, 1, (usize::MAX, 0, 2, 1)).unwrap_err();
        assert!(e.contains("not inside"), "{e}");
        assert!(set_row(&art(&px, 8, 8), 2, 1, (0, 1, 1, usize::MAX)).is_err());
    }

    #[test]
    fn switching_to_an_identical_palette_keeps_duplicate_colours_apart() {
        // row 0: [clear, red, green, red]; the pixels use locals 1 and 3
        let plte: [u16; 4] = [0, 7, 2, 7];
        let mut px = vec![0u8; 64];
        px[..3].copy_from_slice(&[1, 3, 3]);
        let out = switch_palette(&art(&px, 8, 8), &plte, &plte).unwrap();
        assert_eq!(out[..3], [1, 3, 3]);
    }

    #[test]
    fn fix_rows_moves_stray_pixels_to_the_same_colour_in_the_tiles_row() {
        let mut px = vec![0u8; 64];
        px[..3].copy_from_slice(&[1, 2, 2]); // row 0 holds the majority
        px[3] = 6; // row 1, colour 1: row 0 has it at local 1
        px[4] = 7; // row 1, colour 9: row 0 has no such colour
        let f = fix_rows(&art(&px, 8, 8), &PLTE).unwrap();
        assert_eq!((f.pixels[3], f.pixels[4], f.moved), (1, 7, 1));
        assert_eq!(f.left, ["tile 0,0: 1 pixels have no colour in row 0"]);
        assert!(fix_rows(
            &Art {
                rs: 256,
                ..art(&px, 8, 8)
            },
            &PLTE
        )
        .is_err());
    }

    #[test]
    fn switch_palette_remaps_by_exact_colour_and_reports_what_it_cannot() {
        let mut px = vec![0u8; 64];
        px[0] = 1;
        px[1] = 7;
        // new palette: row 0 lacks 9, row 1 = [_, 9, 1, _]
        let new = [0, 1, 2, 3, 0, 9, 1, 3];
        let out = switch_palette(&art(&px, 8, 8), &PLTE, &new).unwrap();
        assert_eq!((out[0], out[1], out[2]), (6, 5, 4));
        let lacking = [0, 1, 2, 3, 0, 4, 5, 3];
        let e = switch_palette(&art(&px, 8, 8), &PLTE, &lacking).unwrap_err();
        assert_eq!(e.len(), 1);
        assert!(e[0].starts_with("#") && e[0].contains("tile 0,0"), "{e:?}");
    }
}
