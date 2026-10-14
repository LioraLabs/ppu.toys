//! Compact byte storage for Lua programs that work with raw binary data.
use piccolo::{Callback, CallbackReturn, Context, Table, Value};
use std::{cell::RefCell, rc::Rc};

fn error<'gc>(ctx: Context<'gc>, message: &str) -> piccolo::Error<'gc> {
    Value::String(ctx.intern(message.as_bytes())).into()
}

pub(crate) fn install(ctx: Context<'_>) {
    let constructor = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let size = stack.get(0).to_integer().unwrap_or(0);
        if !(0..=32 * 1024 * 1024).contains(&size) {
            return Err(error(ctx, "bytebuffer size must be 0..32 MiB").into());
        }
        let bytes = Rc::new(RefCell::new(vec![0u8; size as usize]));
        let table = Table::new(&ctx);
        for (name, width) in [("r8", 1usize), ("r16", 2), ("r32", 4), ("r64", 8)] {
            let data = bytes.clone();
            let read = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                let addr = stack.get(0).to_integer().unwrap_or(-1);
                if addr < 0 {
                    return Err(error(ctx, "bytebuffer read address is negative").into());
                }
                let data = data.borrow();
                let mut value = 0u64;
                for i in 0..width {
                    value |= (*data.get(addr as usize + i).unwrap_or(&0) as u64) << (8 * i)
                }
                stack.clear();
                stack.push_back(Value::Integer(value as i64));
                Ok(CallbackReturn::Return)
            });
            table.set(ctx, name, read).unwrap();
        }
        for (name, width) in [("w8", 1usize), ("w16", 2), ("w32", 4), ("w64", 8)] {
            let data = bytes.clone();
            // Writes the low `width` bytes little endian and returns the previous
            // value at that width, so callers can diff old against new.
            let write = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                let addr = stack.get(0).to_integer().unwrap_or(-1);
                let value = stack
                    .get(1)
                    .to_integer()
                    .ok_or_else(|| error(ctx, "bytebuffer write requires an integer"))?
                    as u64;
                if addr < 0 || addr.saturating_add(width as i64) > 32 * 1024 * 1024 {
                    return Err(error(ctx, "bytebuffer write address is out of range").into());
                }
                let mut data = data.borrow_mut();
                let addr = addr as usize;
                if data.len() < addr + width {
                    data.resize(addr + width, 0)
                }
                let mut old = 0u64;
                for i in 0..width {
                    old |= (data[addr + i] as u64) << (8 * i);
                    data[addr + i] = (value >> (8 * i)) as u8;
                }
                stack.clear();
                stack.push_back(Value::Integer(old as i64));
                Ok(CallbackReturn::Return)
            });
            table.set(ctx, name, write).unwrap();
        }
        for name in ["load", "fill", "copy"] {
            let data = bytes.clone();
            let callback = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                let dst = stack.get(0).to_integer().unwrap_or(-1);
                let n = if name == "load" {
                    match stack.get(1) {
                        Value::String(s) => s.as_bytes().len() as i64,
                        _ => return Err(error(ctx, "bytebuffer load requires a string").into()),
                    }
                } else {
                    stack.get(2).to_integer().unwrap_or(-1)
                };
                if dst < 0 || n < 0 || dst.saturating_add(n) > 32 * 1024 * 1024 {
                    return Err(error(ctx, "bytebuffer destination is out of range").into());
                }
                let mut data = data.borrow_mut();
                let dst = dst as usize;
                let n = n as usize;
                if data.len() < dst + n {
                    data.resize(dst + n, 0)
                }
                match name {
                    "load" => {
                        if let Value::String(s) = stack.get(1) {
                            data[dst..dst + n].copy_from_slice(s.as_bytes())
                        }
                    }
                    "fill" => {
                        let v = stack
                            .get(1)
                            .to_integer()
                            .ok_or_else(|| error(ctx, "bytebuffer fill requires an integer"))?;
                        data[dst..dst + n].fill(v as u8)
                    }
                    _ => {
                        let src = stack.get(1).to_integer().unwrap_or(-1);
                        if src < 0 || src.saturating_add(n as i64) > data.len() as i64 {
                            return Err(error(ctx, "bytebuffer source is out of range").into());
                        }
                        data.copy_within(src as usize..src as usize + n, dst);
                    }
                }
                stack.clear();
                Ok(CallbackReturn::Return)
            });
            table.set(ctx, name, callback).unwrap();
        }
        stack.clear();
        stack.push_back(Value::Table(table));
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("bytebuffer", constructor).unwrap();
}
