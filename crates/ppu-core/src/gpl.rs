//! `.gpl` (GIMP palette text) to a palette source.
//!
//! ```text
//! GIMP Palette
//! Name: sprites
//! Columns: 16
//! # comment
//! 255 0 0 red
//! ```
//!
//! `Columns:` is the row size (4, 16 or 256; absent means 16): 4- and 16-colour rows are
//! padded to whole rows with each row's colour 0, which is the transparent one placement
//! never writes; a 256 block keeps its real colour count.

use crate::memory::{rgb15, unpack_rgb15};
use crate::source::{PaletteReport, PaletteSource, SourceMeta, SourcePayload, SourceReport};

/// An 8-bit channel as RGB555's 5 bits (memory's [`rgb15`] for one channel).
pub fn to_555(v: u8) -> u16 {
    rgb15(v, 0, 0)
}

/// A 5-bit channel back to 8 bits, so 0 stays 0 and 31 is 255 (memory's [`unpack_rgb15`]).
pub fn from_555(v: u16) -> u8 {
    unpack_rgb15(v & 0x1f)[0]
}

/// Parse `.gpl` text. Every refusal names its line.
pub fn parse(text: &str) -> Result<PaletteSource, String> {
    let mut lines = text.lines().enumerate().map(|(i, l)| (i + 1, l.trim()));
    match lines.next() {
        Some((_, "GIMP Palette")) => {}
        _ => return Err("line 1: a .gpl file starts with 'GIMP Palette'".into()),
    }
    let mut row_size = 16usize;
    let mut colors: Vec<u16> = vec![];
    for (n, line) in lines {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(v) = line.strip_prefix("Columns:") {
            row_size = match v.trim().parse() {
                Ok(c @ (4 | 16 | 256)) => c,
                _ => {
                    return Err(format!(
                        "line {n}: Columns must be 4, 16 or 256 (the colours a palette row holds), got '{}'",
                        v.trim()
                    ))
                }
            };
            continue;
        }
        if line.starts_with("Name:") {
            continue;
        }
        let mut parts = line.split_whitespace();
        let mut channel = || -> Result<u8, String> {
            let p = parts
                .next()
                .ok_or_else(|| format!("line {n}: expected 'R G B [name]', got '{line}'"))?;
            p.parse::<u8>()
                .map_err(|_| format!("line {n}: '{p}' is not a channel value from 0 to 255"))
        };
        let (r, g, b) = (channel()?, channel()?, channel()?);
        if colors.len() == 256 {
            return Err(format!("line {n}: a palette holds at most 256 colours"));
        }
        colors.push(rgb15(r, g, b));
    }
    if colors.is_empty() {
        return Err("the palette has no colours".into());
    }
    let rows = colors
        .chunks(row_size)
        .map(|c| {
            let mut row = c.to_vec();
            if row_size != 256 {
                row.resize(row_size, c[0]);
            }
            row
        })
        .collect();
    Ok(PaletteSource {
        row_size: row_size as u16,
        rows,
    })
}

/// A palette source as `.gpl` text, the inverse of [`parse`]: `Columns:` is the row size and
/// every colour line is padded (`%3d %3d %3d\tname`), because Pixelorama skips lines under 9
/// characters and every later index would shift. Colours are named by their index.
pub fn write(src: &PaletteSource, name: &str) -> String {
    let name = name.replace(['\n', '\r'], " ");
    let mut out = format!("GIMP Palette\nName: {name}\nColumns: {}\n#\n", src.row_size);
    for (i, &c) in src.rows.iter().flatten().enumerate() {
        let [r, g, b, _] = crate::memory::unpack_rgb15(c);
        out.push_str(&format!("{r:3} {g:3} {b:3}\t{i}\n"));
    }
    out
}

/// `text` with its colour lines' values replaced by `colours` (8-bit r, g, b), each snapped to
/// RGB555. Everything else stays as written: header, `Name:`, `Columns:`, comments, blank lines
/// and each colour's name. Every colour line is padded (`%3d %3d %3d\tname`, see [`write`]).
/// Refuses a count that is not the number of colour lines, naming both.
pub fn recolour(text: &str, colours: &[[u8; 3]]) -> Result<String, String> {
    let have = colour_count(text);
    if have != colours.len() {
        return Err(format!(
            "the .gpl has {have} colour lines, but {} colours were given",
            colours.len()
        ));
    }
    let mut next = colours.iter();
    let mut out = String::with_capacity(text.len());
    for (i, seg) in text.split_inclusive('\n').enumerate() {
        // Line endings (LF or CRLF) are kept as each line had them.
        let line = seg.trim_end_matches(['\r', '\n']);
        if i > 0 && is_colour_line(line) {
            let snap = |v: u8| from_555(to_555(v));
            let [r, g, b] = next.next().expect("counted above").map(snap);
            // The name is what follows the third channel.
            let mut rest = line.trim_start();
            for _ in 0..3 {
                rest = rest
                    .trim_start_matches(|c: char| !c.is_whitespace())
                    .trim_start();
            }
            out += &format!("{r:3} {g:3} {b:3}");
            if !rest.is_empty() {
                out += &format!("\t{}", rest.trim_end());
            }
            out += &seg[line.len()..];
        } else {
            out += seg;
        }
    }
    Ok(out)
}

fn is_colour_line(l: &str) -> bool {
    let l = l.trim();
    !(l.is_empty() || l.starts_with('#') || l.starts_with("Columns:") || l.starts_with("Name:"))
}

/// How many colour lines `text` holds: the exact count [`recolour`] (and `ppu art palette write`)
/// expects, which is not the padded rows of [`parse`].
pub fn colour_count(text: &str) -> usize {
    text.lines()
        .enumerate()
        .filter(|&(i, l)| i > 0 && is_colour_line(l))
        .count()
}

/// The `convertPalette` conversion: `.gpl` text to (payload, meta).
pub fn convert_palette(text: &str) -> Result<(SourcePayload, SourceMeta), String> {
    let src = parse(text)?;
    let (rows, row_size) = (src.rows.len(), src.row_size);
    let meta = SourceMeta {
        width: row_size as u32,
        height: rows as u32,
        report: SourceReport::Palette {
            report: PaletteReport { rows, row_size },
        },
        cells: None,
    };
    Ok((SourcePayload::Palette(src), meta))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_5_bit_level_round_trips_through_8_bits() {
        for v in 0..32u16 {
            assert_eq!(to_555(from_555(v)), v);
        }
        assert_eq!((from_555(0), from_555(31)), (0, 255));
    }

    #[test]
    fn a_256_block_keeps_its_real_colour_count() {
        let p = parse("GIMP Palette\nColumns: 256\n0 0 0\n255 0 0\n0 0 255\n").unwrap();
        assert_eq!((p.row_size, p.rows.len()), (256, 1));
        assert_eq!(p.rows[0], [0, 0x001f, 0x7c00]);
    }

    #[test]
    fn rows_pad_with_their_colour_0() {
        let p = parse(
            "GIMP Palette\nColumns: 4\n# c\n255 0 0 a\n0 255 0\n0 0 255\n8 8 8\n16 16 16 x\n",
        )
        .unwrap();
        assert_eq!(p.row_size, 4);
        assert_eq!(p.rows[0], [0x001f, 0x03e0, 0x7c00, 0x0421]);
        assert_eq!(p.rows[1], [0x0842; 4]);
    }

    #[test]
    fn write_pads_every_line_and_round_trips_through_parse() {
        let src = PaletteSource {
            row_size: 4,
            rows: vec![vec![0, 0x001f, 0x03e0, 0x7fff], vec![0, 0x0421, 0, 0x7c00]],
        };
        let text = write(&src, "sprites");
        assert!(text.starts_with("GIMP Palette\nName: sprites\nColumns: 4\n#\n"));
        let lines: Vec<&str> = text.lines().skip(4).collect();
        assert_eq!(lines.len(), 8);
        assert_eq!(lines[1], "255   0   0\t1");
        assert!(lines.iter().all(|l| l.len() >= 9), "{lines:?}");
        assert_eq!(parse(&text).unwrap(), src);
        // a row writes exactly its colours, never a padded 256
        let short = PaletteSource {
            row_size: 256,
            rows: vec![vec![0, 0x001f, 0x03e0]],
        };
        assert_eq!(write(&short, "x").lines().count(), 4 + 3);
    }

    #[test]
    fn recolour_keeps_everything_but_the_values_and_snaps_them() {
        let text = "GIMP Palette\nName: x\nColumns: 4\n# note\n  0   0   0\tclear\n9 9 9 two words\n\n1 2 3\n";
        let out = recolour(text, &[[0, 0, 0], [255, 8, 7], [1, 2, 3]]).unwrap();
        assert_eq!(
            out,
            "GIMP Palette\nName: x\nColumns: 4\n# note\n  0   0   0\tclear\n255   8   0\ttwo words\n\n  0   0   0\n"
        );
        assert_eq!(parse(&out).unwrap().rows[0][1], 0x003f);
        let e = recolour(text, &[[0, 0, 0]]).unwrap_err();
        assert!(
            e.contains("3 colour lines") && e.contains("1 colours"),
            "{e}"
        );
    }

    #[test]
    fn recolour_keeps_crlf_line_endings() {
        let text = "GIMP Palette\r\nName: x\r\nColumns: 4\r\n#\r\n1 2 3\tone\r\n9 9 9";
        let out = recolour(text, &[[0, 0, 0], [8, 8, 8]]).unwrap();
        assert_eq!(
            out,
            "GIMP Palette\r\nName: x\r\nColumns: 4\r\n#\r\n  0   0   0\tone\r\n  8   8   8"
        );
        assert_eq!(colour_count(text), 2);
    }

    #[test]
    fn columns_default_to_16_and_must_be_a_row_size() {
        assert_eq!(parse("GIMP Palette\n1 2 3\n").unwrap().row_size, 16);
        let e = parse("GIMP Palette\nColumns: 8\n1 2 3\n").unwrap_err();
        assert!(e.starts_with("line 2: Columns must be 4, 16 or 256"), "{e}");
    }

    #[test]
    fn refusals_name_their_line() {
        for (text, want) in [
            ("Palette\n", "line 1: a .gpl file starts with"),
            ("GIMP Palette\n\n1 2\n", "line 3: expected 'R G B"),
            ("GIMP Palette\n1 2 256\n", "line 2: '256' is not a channel"),
            ("GIMP Palette\nx 2 3\n", "line 2: 'x' is not a channel"),
            ("GIMP Palette\nName: x\n", "no colours"),
        ] {
            let e = parse(text).unwrap_err();
            assert!(e.contains(want), "{text:?}: {e}");
        }
        let many = format!("GIMP Palette\n{}", "1 2 3\n".repeat(257));
        assert!(parse(&many)
            .unwrap_err()
            .starts_with("line 258: a palette holds at most 256"));
    }
}
