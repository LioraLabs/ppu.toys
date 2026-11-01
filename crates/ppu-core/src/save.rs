//! Save data: the battery-backed RAM image a `saveData({...})` record keeps, the same bytes in the
//! preview (luau.lua `saveData`) and on a cartridge, so a `.srm` moves between them.
//!
//! The image is little-endian 16-bit words:
//! - word 0: [`MAGIC`]; word 1: the layout id (a changed record starts from its defaults instead of
//!   misreading an old save); word 2: the checksum, the XOR of every field word;
//! - then each field in name order: a boolean is one word (0 or 1); a `makeBounded(v, lo, hi)` field
//!   keeps `v - lo`, in one word when `hi - lo` fits 16 bits, else two (low word first);
//! - the rest of the image, up to its size (the smallest power of two from 2 KiB that holds the
//!   fields), is zero after a fresh start.
//!
//! A save is used at power-on only when the magic, layout id and checksum match and every field is in
//! range; otherwise the whole image is cleared and the declared defaults are written.

/// "PS", little-endian.
pub const MAGIC: u16 = 0x5350;
/// Magic, layout id, checksum.
pub const HEADER_WORDS: usize = 3;
/// The checksum's word.
pub const CHECKSUM_WORD: usize = 2;
/// The smallest SRAM a cartridge header can declare that boards ship: 2 KiB.
pub const MIN_BYTES: usize = 2048;
/// The largest save: 8 KiB, whose hex fits the preview's 32 KiB `sram` slot with room to spare.
pub const MAX_BYTES: usize = 8192;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Boolean,
    /// A whole number in `lo..=hi`.
    Bounded { lo: i64, hi: i64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub kind: Kind,
    /// First word of the image the field occupies.
    pub word: usize,
    /// 1 or 2.
    pub words: usize,
}

impl Field {
    /// The largest stored value: `hi - lo`, or 1 for a boolean.
    pub fn span(&self) -> i64 {
        match self.kind {
            Kind::Boolean => 1,
            Kind::Bounded { lo, hi } => hi - lo,
        }
    }
    /// The amount stored values are offset by (`lo`; 0 for a boolean).
    pub fn base(&self) -> i64 {
        match self.kind {
            Kind::Boolean => 0,
            Kind::Bounded { lo, .. } => lo,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub fields: Vec<Field>,
    pub id: u16,
    /// Bytes of SRAM: a power of two from [`MIN_BYTES`] to [`MAX_BYTES`].
    pub size: usize,
}

impl Layout {
    /// Words the header and fields occupy.
    pub fn used_words(&self) -> usize {
        self.fields.last().map_or(HEADER_WORDS, |f| f.word + f.words)
    }

    /// The literal the Luau frontend passes to luau.lua's `saveData` (a cartridge build
    /// reads it too): `{size = 2048, id = 4660, {"hiscore", 3, 2, 0, 999999, false}, ...}`.
    pub fn lua(&self) -> String {
        let mut out = format!("{{size = {}, id = {}", self.size, self.id);
        for f in &self.fields {
            let (lo, hi, boolean) = match f.kind {
                Kind::Boolean => (0, 1, true),
                Kind::Bounded { lo, hi } => (lo, hi, false),
            };
            out += &format!(", {{{:?}, {}, {}, {lo}, {hi}, {boolean}}}", f.name, f.word, f.words);
        }
        out + "}"
    }
}

/// The header-declared SRAM size code ($FFD8 in a LoROM header): log2 of the size in KiB.
pub fn size_code(bytes: usize) -> u8 {
    (bytes / 1024).trailing_zeros() as u8
}

/// The layout of a record's fields, in name order. `Err` says why a field cannot be saved.
pub fn layout(fields: &[(String, Kind)]) -> Result<Layout, String> {
    let mut sorted: Vec<&(String, Kind)> = fields.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    if sorted.is_empty() {
        return Err("saveData needs at least one field".into());
    }
    let mut out = vec![];
    let mut word = HEADER_WORDS;
    // The id: a 16-bit hash of each field's name, kind and bounds, so any change to the record changes it.
    let mut id: u16 = 0;
    let mut hash = |text: &str| {
        for b in text.bytes() {
            id = id.wrapping_mul(31).wrapping_add(b as u16);
        }
    };
    for (name, kind) in sorted {
        let words = match *kind {
            Kind::Boolean => {
                hash(&format!("{name}:b;"));
                1
            }
            Kind::Bounded { lo, hi } => {
                if lo > hi {
                    return Err(format!("saveData field {name} needs its minimum no greater than its maximum"));
                }
                if hi - lo > i32::MAX as i64 || lo < i32::MIN as i64 || hi > i32::MAX as i64 {
                    return Err(format!("saveData field {name}'s range {lo}..{hi} is wider than a cartridge integer holds"));
                }
                hash(&format!("{name}:{lo}:{hi};"));
                if hi - lo > 0xffff {
                    2
                } else {
                    1
                }
            }
        };
        out.push(Field { name: name.clone(), kind: *kind, word, words });
        word += words;
    }
    let bytes = word * 2;
    if bytes > MAX_BYTES {
        return Err(format!("saveData needs {bytes} bytes, more than the 8 KiB a save may hold"));
    }
    let size = bytes.next_power_of_two().max(MIN_BYTES);
    Ok(Layout { fields: out, id, size })
}

/// The image a fresh start writes: zeros, the header, and each field's default (stored value per
/// field, in `layout.fields` order: `v - lo`, or 0/1).
pub fn fresh(layout: &Layout, stored: &[i64]) -> Vec<u8> {
    let mut words = vec![0u16; layout.size / 2];
    words[0] = MAGIC;
    words[1] = layout.id;
    for (f, &s) in layout.fields.iter().zip(stored) {
        words[f.word] = s as u16;
        if f.words == 2 {
            words[f.word + 1] = (s >> 16) as u16;
        }
    }
    words[CHECKSUM_WORD] = words[HEADER_WORDS..layout.used_words()].iter().fold(0, |c, w| c ^ w);
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

/// Whether a power-on keeps `image`, by the rules the preview and the cartridge share.
pub fn valid(layout: &Layout, image: &[u8]) -> bool {
    let word = |i: usize| image.get(i * 2..i * 2 + 2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]));
    word(0) == MAGIC
        && word(1) == layout.id
        && (HEADER_WORDS..layout.used_words()).fold(0, |c, i| c ^ word(i)) == word(CHECKSUM_WORD)
        && layout.fields.iter().all(|f| {
            let stored = word(f.word) as i64 | if f.words == 2 { (word(f.word + 1) as i64) << 16 } else { 0 };
            stored <= f.span()
        })
}

/// The `.srm` image in a save slot (the `sram` JSON a host persists): `"ppu.save"` hex padded with zeros to
/// `"ppu.save.size"` bytes. `None` when the slot holds no save image.
pub fn srm_from_slot(json: &str) -> Option<Vec<u8>> {
    let slot: serde_json::Value = serde_json::from_str(json).ok()?;
    let hex = slot.get("ppu.save")?.as_str()?;
    let size = slot.get("ppu.save.size").and_then(|s| s.as_u64()).map_or(hex.len() / 2, |s| s as usize);
    let mut image: Vec<u8> = (0..hex.len() / 2).filter_map(|i| u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok()).collect();
    image.resize(size.max(image.len()), 0);
    Some(image)
}

/// The slot entries for an `.srm` image, as JSON object members: `"ppu.save":"<hex>","ppu.save.size":2048`
/// (trailing zero bytes left out of the hex).
pub fn slot_members(image: &[u8]) -> String {
    let end = image.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
    let hex: String = image[..end].iter().map(|b| format!("{b:02x}")).collect();
    format!("\"ppu.save\":\"{hex}\",\"ppu.save.size\":{}", image.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hiscore() -> Layout {
        layout(&[
            ("stage".into(), Kind::Bounded { lo: 1, hi: 8 }),
            ("hiscore".into(), Kind::Bounded { lo: 0, hi: 999_999 }),
            ("seen_intro".into(), Kind::Boolean),
        ])
        .unwrap()
    }

    #[test]
    fn fields_take_name_order_and_whole_words() {
        let l = hiscore();
        let place: Vec<(&str, usize, usize)> = l.fields.iter().map(|f| (f.name.as_str(), f.word, f.words)).collect();
        assert_eq!(place, [("hiscore", 3, 2), ("seen_intro", 5, 1), ("stage", 6, 1)]);
        assert_eq!(l.size, 2048);
        assert_eq!(size_code(l.size), 1);
        assert_eq!(l.lua(), format!("{{size = 2048, id = {}, {{\"hiscore\", 3, 2, 0, 999999, false}}, {{\"seen_intro\", 5, 1, 0, 1, true}}, {{\"stage\", 6, 1, 1, 8, false}}}}", l.id));
    }

    #[test]
    fn the_id_changes_with_the_record() {
        let a = hiscore();
        let b = layout(&[("hiscore".into(), Kind::Bounded { lo: 0, hi: 99_999 })]).unwrap();
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn a_fresh_image_is_valid_and_a_flipped_bit_is_not() {
        let l = hiscore();
        // hiscore 123456 (0x1E240), seen_intro true, stage 3 (stored 2).
        let mut image = fresh(&l, &[123_456, 1, 2]);
        assert_eq!(image.len(), 2048);
        assert_eq!(&image[..14], &[0x50, 0x53, l.id as u8, (l.id >> 8) as u8, 0x42, 0xe2, 0x40, 0xe2, 0x01, 0x00, 0x01, 0x00, 0x02, 0x00]);
        assert!(valid(&l, &image));
        image[12] ^= 4;
        assert!(!valid(&l, &image));
        // In checksum but out of range: stage stored 9 > 7.
        let mut image = fresh(&l, &[0, 0, 9]);
        assert!(!valid(&l, &image));
        image[0] = 0;
        assert!(!valid(&l, &image));
    }

    #[test]
    fn slots_and_srm_images_convert_both_ways() {
        let image = fresh(&hiscore(), &[7, 0, 0]);
        let json = format!("{{{}}}", slot_members(&image));
        assert!(json.starts_with("{\"ppu.save\":\"5053"), "{json}");
        assert_eq!(srm_from_slot(&json).unwrap(), image);
        assert_eq!(srm_from_slot("{}"), None);
    }

    #[test]
    fn sizes_and_refusals() {
        let many: Vec<(String, Kind)> = (0..1100).map(|i| (format!("f{i:04}"), Kind::Boolean)).collect();
        assert_eq!(layout(&many).unwrap().size, 4096);
        let too_many: Vec<(String, Kind)> = (0..4100).map(|i| (format!("f{i:04}"), Kind::Boolean)).collect();
        assert!(layout(&too_many).unwrap_err().contains("8 KiB"));
        assert!(layout(&[("x".into(), Kind::Bounded { lo: 2, hi: 1 })]).is_err());
        assert!(layout(&[("x".into(), Kind::Bounded { lo: -(1 << 31), hi: 1 << 31 })]).is_err());
        assert!(layout(&[]).is_err());
    }
}
