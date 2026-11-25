//! The piano roll `ppu song roll` writes and read_song's `roll` returns: pitch lanes over time,
//! one colour per sound, so an agent can look at what it wrote.

use crate::song::{Song, SongError};

/// Lane height in pixels: one lane per pitch, then one per unpitched sound.
const LANE: u32 = 6;
/// Engine ticks (4 ms) per pixel column, at the least: 16 px a second.
const TICKS_PER_PX: i64 = 4;
const MAX_WIDTH: i64 = 8192;
const BACKGROUND: [u8; 3] = [16, 16, 24];
const LANE_C: [u8; 3] = [28, 28, 40];
const SLOT_LINE: [u8; 3] = [110, 110, 130];
/// One colour per sound, in the order sounds first play.
const COLOURS: [[u8; 3]; 8] = [
    [240, 90, 80],
    [90, 180, 250],
    [250, 200, 70],
    [120, 220, 120],
    [220, 120, 230],
    [250, 150, 60],
    [80, 220, 210],
    [200, 200, 200],
];

/// An RGB piano roll.
pub struct Roll {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

/// Pitch lanes (highest at the top, C lanes shaded, unpitched sounds below) over time, one
/// colour per sound, a line at each arrangement slot's start; a note cut by the voice budget
/// shows its unplayed tail dimmed. `slots` is an inclusive range of arrangement slots.
pub fn roll(song: &Song, slots: Option<(usize, usize)>) -> Result<Roll, SongError> {
    let an = crate::song_analyze::analyze(song)?;
    let count = song.arrangement.len();
    let (a, b) = slots.unwrap_or((0, count.saturating_sub(1)));
    if !(a <= b && b < count) {
        return Err(SongError(format!(
            "slots {a}..{b} are outside the song's {count} slots (0..{})",
            count.saturating_sub(1)
        )));
    }
    let start_of = |s: usize| {
        an.slot_steps
            .get(s)
            .and_then(|&i| an.step_ticks.get(i as usize))
            .map_or(an.length, |&t| i64::from(t))
    };
    let (from, to) = (start_of(a), start_of(b + 1).max(start_of(a) + 1));
    let events: Vec<_> = an
        .events
        .iter()
        .filter(|e| (a..=b).contains(&(e.slot as usize)))
        .collect();

    // Lanes: each played pitch, high to low, then each unpitched sound.
    let row = |i: u32| &song.rows[i as usize];
    let mut pitches: Vec<u32> = events.iter().filter_map(|e| row(e.row).note).collect();
    pitches.sort_unstable_by(|x, y| y.cmp(x));
    pitches.dedup();
    let mut sounds: Vec<&str> = Vec::new();
    let mut drums: Vec<&str> = Vec::new();
    for e in &events {
        let r = row(e.row);
        if !sounds.contains(&r.sound.as_str()) {
            sounds.push(&r.sound);
        }
        if r.note.is_none() && !drums.contains(&r.sound.as_str()) {
            drums.push(&r.sound);
        }
    }
    let lane = |r: &crate::song::Row| match r.note {
        Some(n) => pitches.iter().position(|&p| p == n).unwrap_or(0),
        None => pitches.len() + drums.iter().position(|&d| d == r.sound).unwrap_or(0),
    };
    let lanes = (pitches.len() + drums.len()).max(1) as u32;

    let per_px = TICKS_PER_PX.max((to - from + MAX_WIDTH - 1) / MAX_WIDTH);
    let width = ((to - from + per_px - 1) / per_px) as u32;
    let height = lanes * LANE;
    let mut rgb = BACKGROUND.repeat((width * height) as usize);
    let mut fill = |x0: i64, x1: i64, y0: u32, y1: u32, c: [u8; 3]| {
        let x0 = ((x0 - from) / per_px).clamp(0, i64::from(width)) as u32;
        let x1 = ((x1 - from + per_px - 1) / per_px).clamp(0, i64::from(width)) as u32;
        for y in y0..y1.min(height) {
            for x in x0..x1.max(x0 + 1).min(width) {
                let i = ((y * width + x) * 3) as usize;
                rgb[i..i + 3].copy_from_slice(&c);
            }
        }
    };
    for (i, &p) in pitches.iter().enumerate() {
        if p % 12 == 0 {
            fill(from, to, i as u32 * LANE, (i as u32 + 1) * LANE, LANE_C);
        }
    }
    for s in a..=b {
        fill(start_of(s), start_of(s) + 1, 0, height, SLOT_LINE);
    }
    for e in &events {
        let r = row(e.row);
        let c = COLOURS[sounds.iter().position(|&s| s == r.sound).unwrap_or(0) % COLOURS.len()];
        let y = lane(r) as u32 * LANE;
        if e.want_end > e.end {
            fill(e.end, e.want_end, y + 2, y + LANE - 2, c.map(|v| v / 3));
        }
        fill(e.start, e.end, y + 1, y + LANE - 1, c);
    }
    Ok(Roll { width, height, rgb })
}

/// The roll as an RGB PNG.
pub fn encode_png(roll: &Roll) -> Vec<u8> {
    let mut out = Vec::new();
    let mut e = png::Encoder::new(&mut out, roll.width, roll.height);
    e.set_color(png::ColorType::Rgb);
    e.set_depth(png::BitDepth::Eight);
    e.write_header()
        .and_then(|mut w| w.write_image_data(&roll.rgb))
        .expect("PNG into memory");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::song::{Note, Pattern, Row};

    /// 120 BPM: a 192-unit bar is 2 s, 500 ticks. Verse holds lead C5 for half a bar, then a
    /// kick; rest is empty.
    fn song() -> Song {
        let row = |sound: &str, note| Row {
            sound: sound.into(),
            note,
            vol: 100,
            pan: 0,
            keep: false,
        };
        let note = |at, row, len| Note {
            at,
            row,
            len,
            vel: 127,
            ..Default::default()
        };
        Song {
            tempo: 12000,
            voice_mask: 0xff,
            rows: vec![
                row("lead", Some(72)),
                row("bass", Some(36)),
                row("kick", None),
            ],
            patterns: vec![
                Pattern {
                    name: "verse".into(),
                    length: 192,
                    notes: vec![note(0, 0, 96), note(96, 2, 48)],
                    ..Default::default()
                },
                Pattern {
                    name: "rest".into(),
                    length: 192,
                    ..Default::default()
                },
            ],
            arrangement: vec![0, 1],
            ..Default::default()
        }
    }
    fn at(r: &Roll, x: u32, y: u32) -> [u8; 3] {
        let i = ((y * r.width + x) * 3) as usize;
        [r.rgb[i], r.rgb[i + 1], r.rgb[i + 2]]
    }

    #[test]
    fn notes_paint_where_they_sit() {
        let r = roll(&song(), None).unwrap();
        // 1000 ticks at 4 a pixel; lanes: C5, then kick.
        assert_eq!((r.width, r.height), (250, 2 * LANE));
        let mid = LANE / 2;
        assert_eq!(
            at(&r, 30, mid),
            COLOURS[0],
            "lead C5 sounds in the first quarter"
        );
        assert_eq!(at(&r, 30, LANE + mid), BACKGROUND, "no kick yet");
        assert_eq!(at(&r, 70, LANE + mid), COLOURS[1], "the kick at beat 3");
        assert_eq!(
            at(&r, 200, mid),
            LANE_C,
            "the rest bar is empty (C lane shading)"
        );
        assert_eq!(at(&r, 125, 0), SLOT_LINE, "slot 1 starts at 2 s");
    }

    #[test]
    fn slots_crop_the_roll() {
        let r = roll(&song(), Some((1, 1))).unwrap();
        assert_eq!((r.width, r.height), (125, LANE));
        assert!(roll(&song(), Some((1, 2))).is_err());
    }
}
