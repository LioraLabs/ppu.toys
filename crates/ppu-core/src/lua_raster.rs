//! SNES register rows for ports that emit raw register writes; pixels still use the native PPU.
use crate::LineTableRow;
use piccolo::{Callback, CallbackReturn, Context, Value};
use std::{cell::RefCell, collections::HashMap, rc::Rc};
pub(crate) type Raster = Rc<RefCell<Option<Vec<LineTableRow>>>>;

pub(crate) fn install(ctx: Context<'_>, raster: Raster) {
    let callback = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let error = |msg: &str| piccolo::Error::from(Value::String(ctx.intern(msg.as_bytes())));
        let Value::Table(lines) = stack.get(0) else {
            return Err(error("raster requires rows indexed 0..223"));
        };
        let mut rows = Vec::with_capacity(224);
        let mut cache = HashMap::new();
        let mut baseline = [0u16; 256];
        for y in 0..224usize {
            let Value::Table(r) = lines.get(ctx, y as i64) else {
                return Err(error("raster requires 224 register rows"));
            };
            let mut row = if let Some(row) = cache.get(&r) {
                LineTableRow::clone(row)
            } else {
                let mut b = [0u8; 64];
                for (i, v) in b.iter_mut().enumerate() {
                    let n = r
                        .get(ctx, i as i64)
                        .to_integer()
                        .ok_or_else(|| error("raster register bytes must be integers"))?;
                    if !(0..=255).contains(&n) {
                        return Err(error("raster register byte is out of range"));
                    }
                    *v = n as u8;
                }
                let mut row = decode(&b);
                let Value::Table(pal) = r.get(ctx, "palette") else {
                    return Err(error("raster requires a 256-colour palette"));
                };
                for (i, base) in baseline.iter_mut().enumerate() {
                    let value = pal
                        .get(ctx, i as i64)
                        .to_integer()
                        .ok_or_else(|| error("raster palette requires integers"))?;
                    if !(0..=32767).contains(&value) {
                        return Err(error("raster palette is out of range"));
                    }
                    if y == 0 {
                        *base = value as u16;
                    }
                    if value as u16 != *base {
                        row.cgram.push((i as u8, value as u16))
                    }
                }
                cache.insert(r, row.clone());
                row
            };
            // BG scanout starts at SNES line 1; sprite scanout starts at zero.
            for i in 0..4 {
                let block = if row.mosaic_enable[i] {
                    row.mosaic_size as usize + 1
                } else {
                    1
                };
                let phase = ((y + 1) / block - y / block) * block;
                row.bg[i].scroll_y = (row.bg[i].scroll_y as u16).wrapping_add(phase as u16) as f32;
            }
            rows.push(row);
        }
        if let Value::Table(pal) = ctx.get_global("cgram") {
            for (i, value) in baseline.into_iter().enumerate() {
                pal.set(ctx, i as i64, value as i64).unwrap();
            }
        }
        *raster.borrow_mut() = Some(rows);
        stack.clear();
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("raster", callback).unwrap();
}
fn decode(r: &[u8; 64]) -> LineTableRow {
    let word = |i: usize| u16::from_le_bytes([r[i], r[i + 1]]);
    let mut l = LineTableRow::default();
    l.mode = r[4] & 7;
    l.bg3_priority = r[4] & 8 != 0;
    l.brightness = r[0] & 15;
    l.force_blank = r[0] & 128 != 0;
    l.mosaic_size = r[5] >> 4;
    for i in 0..4 {
        l.mosaic_enable[i] = r[5] & (1 << i) != 0;
        l.bg[i].tile_size = if r[4] & (0x10 << i) != 0 { 16 } else { 8 };
        l.bg[i].map_base = ((r[6 + i] & 0xfc) as u32) << 8;
        l.bg[i].screen_size = r[6 + i] & 3;
        l.bg[i].char_base = (((word(10) >> (i * 4)) & 15) as u32) << 12;
        l.bg[i].scroll_x = word(14 + i * 2) as f32;
        l.bg[i].scroll_y = word(22 + i * 2) as f32;
    }
    l.m7.a = word(30) as i16 as f32 / 256.;
    l.m7.b = word(32) as i16 as f32 / 256.;
    l.m7.c = word(34) as i16 as f32 / 256.;
    l.m7.d = word(36) as i16 as f32 / 256.;
    l.m7.cx = word(38) as i16 as f32;
    l.m7.cy = word(40) as i16 as f32;
    l.m7.repeat = r[12] >> 6;
    l.m7.flip_x = r[12] & 1 != 0;
    l.m7.flip_y = r[12] & 2 != 0;
    if l.mode == 7 {
        l.bg[0].scroll_x = word(42) as f32;
        l.bg[0].scroll_y = word(44) as f32;
    }
    l.setini = r[13];
    l.coldata = word(46);
    l.w12sel = r[48];
    l.w34sel = r[49];
    l.wobjsel = r[50];
    l.wh0 = r[52];
    l.wh1 = r[53];
    l.wh2 = r[54];
    l.wh3 = r[55];
    l.wbglog = r[56];
    l.wobjlog = r[57];
    l.tm = r[58];
    l.ts = r[59];
    l.tmw = r[60];
    l.tsw = r[61];
    l.cgadsub = r[62];
    l.cgwsel = r[63];
    l
}
