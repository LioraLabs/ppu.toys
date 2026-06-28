//! piccolo Lua VM + flat-global DSL binding. Runs `frame(t,f)` once to populate
//! frame-wide defaults + CGRAM/OAM, registers `hdma` hooks, then resolves the
//! LineTable by invoking each covering hook per scanline (later call wins).

use std::cell::RefCell;
use std::rc::Rc;

use piccolo::{
    Callback, CallbackReturn, Closure, Executor, Lua, PrototypeError, StashedFunction,
    StaticError, Table, Value,
};

use crate::{rgb15, LineTable, LineTableBuilder, LineTableRow, Memory, HEIGHT};

/// Compile/runtime error surfaced to the editor, matching the TS `LuaError` shape.
#[derive(Debug, Clone, PartialEq)]
pub struct LuaError {
    pub message: String,
    pub line: Option<u32>,
}

/// The embedded Lua VM plus the captured `frame`/`init` entry points and mirrored
/// PPU memory. Globals persist across frames (sticky registers).
pub struct LuaEngine {
    lua: Rc<RefCell<Lua>>,
    frame_fn: Option<StashedFunction>,
    init_fn: Option<StashedFunction>,
    memory: Memory,
}

impl Default for LuaEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl LuaEngine {
    pub fn new() -> Self {
        let mut lua = Lua::core();
        lua.enter(install_bindings);
        LuaEngine {
            lua: Rc::new(RefCell::new(lua)),
            frame_fn: None,
            init_fn: None,
            memory: Memory::new(),
        }
    }

    /// Mirrored PPU memory after the most recent `frame()`.
    pub fn memory(&self) -> &Memory {
        &self.memory
    }
}

fn clamp_u8(v: f64) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    let h = ((h % 360.0) + 360.0) % 360.0;
    let s = s.clamp(0.0, 1.0);
    let l = l.clamp(0.0, 1.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h / 60.0;
    let x = c * (1.0 - ((hp % 2.0) - 1.0).abs());
    let (r1, g1, b1) = match hp as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    (
        clamp_u8((r1 + m) * 255.0),
        clamp_u8((g1 + m) * 255.0),
        clamp_u8((b1 + m) * 255.0),
    )
}

fn install_bindings(ctx: piccolo::Context<'_>) {
    // scalar registers
    ctx.set_global("mode", 1).unwrap();
    ctx.set_global("brightness", 15).unwrap();

    // bg[1..4] = { scroll = {x,y}, source=nil, visible=true }
    let bg = Table::new(&ctx);
    for i in 1..=4i64 {
        let layer = Table::new(&ctx);
        let scroll = Table::new(&ctx);
        scroll.set(ctx, "x", 0.0).unwrap();
        scroll.set(ctx, "y", 0.0).unwrap();
        layer.set(ctx, "scroll", scroll).unwrap();
        layer.set(ctx, "visible", true).unwrap();
        bg.set(ctx, i, layer).unwrap();
    }
    ctx.set_global("bg", bg).unwrap();

    // m7
    let m7 = Table::new(&ctx);
    for (k, v) in [("a", 1.0), ("b", 0.0), ("c", 0.0), ("d", 1.0), ("cx", 0.0), ("cy", 0.0)] {
        m7.set(ctx, k, v).unwrap();
    }
    ctx.set_global("m7", m7).unwrap();

    // cgram (scalar table, written by user)
    ctx.set_global("cgram", Table::new(&ctx)).unwrap();

    // obj[0..127] + obj.sheet
    let obj = Table::new(&ctx);
    for i in 0..128i64 {
        let o = Table::new(&ctx);
        for (k, v) in [("x", 0.0), ("y", 0.0)] {
            o.set(ctx, k, v).unwrap();
        }
        for k in ["tile", "pal", "prio", "size"] {
            o.set(ctx, k, 0).unwrap();
        }
        for k in ["flip_x", "flip_y", "on"] {
            o.set(ctx, k, false).unwrap();
        }
        obj.set(ctx, i, o).unwrap();
    }
    ctx.set_global("obj", obj).unwrap();

    // hidden hook registry
    ctx.set_global("__ppu_hooks", Table::new(&ctx)).unwrap();

    // math aliases as flat globals
    if let Value::Table(m) = ctx.get_global("math") {
        for name in ["sin", "cos", "tan", "floor", "ceil", "abs", "sqrt", "min", "max"] {
            let f = m.get(ctx, name);
            ctx.set_global(name, f).unwrap();
        }
        let pi = m.get(ctx, "pi");
        ctx.set_global("pi", pi).unwrap();
    }

    // rgb(r,g,b) -> packed 15-bit int
    let rgb = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let r = stack.get(0).to_number().unwrap_or(0.0);
        let g = stack.get(1).to_number().unwrap_or(0.0);
        let b = stack.get(2).to_number().unwrap_or(0.0);
        let packed = rgb15(clamp_u8(r), clamp_u8(g), clamp_u8(b)) as i64;
        stack.replace(ctx, packed);
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("rgb", rgb).unwrap();

    // hsl(h,s,l) -> packed 15-bit int
    let hsl = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let h = stack.get(0).to_number().unwrap_or(0.0);
        let s = stack.get(1).to_number().unwrap_or(0.0);
        let l = stack.get(2).to_number().unwrap_or(0.0);
        let (r, g, b) = hsl_to_rgb(h, s, l);
        let packed = rgb15(r, g, b) as i64;
        stack.replace(ctx, packed);
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("hsl", hsl).unwrap();

    // hdma(y0,y1,fn) / scanline alias -> append {y0,y1,fn} to __ppu_hooks
    let hdma = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let y0 = stack.get(0).to_integer().unwrap_or(0);
        let y1 = stack.get(1).to_integer().unwrap_or(0);
        let f = stack.get(2);
        stack.clear();
        if let Value::Table(hooks) = ctx.get_global("__ppu_hooks") {
            let entry = Table::new(&ctx);
            entry.set(ctx, 1, y0).unwrap();
            entry.set(ctx, 2, y1).unwrap();
            entry.set(ctx, 3, f).unwrap();
            let n = hooks.length();
            hooks.set(ctx, n + 1, entry).unwrap();
        }
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("hdma", hdma).unwrap();
    ctx.set_global("scanline", hdma).unwrap();
}

fn static_error_to_lua(e: StaticError) -> LuaError {
    let line = if let StaticError::Runtime(rt) = &e {
        rt.downcast::<PrototypeError>().and_then(|pe| match pe {
            PrototypeError::Parser(p) => Some(p.line_number.0 as u32),
            _ => None,
        })
    } else {
        None
    };
    LuaError { message: e.to_string(), line }
}
