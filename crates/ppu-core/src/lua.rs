//! piccolo Lua VM + flat-global DSL binding. Runs `frame(t,f)` once to populate
//! frame-wide defaults + CGRAM/OAM, registers `hdma` hooks, then resolves the
//! LineTable by invoking each covering hook per scanline (later call wins).

use std::cell::RefCell;
use std::rc::Rc;

use piccolo::{
    Callback, CallbackReturn, Closure, Executor, Lua, PrototypeError, StashedFunction, StaticError,
    Table, Value,
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

    /// Mutable mirrored memory — used by the wasm shim to insert uploaded
    /// image sources (Memory.sources) without going through the Lua VM.
    pub fn memory_mut(&mut self) -> &mut Memory {
        &mut self.memory
    }

    /// Compile and load a DSL source: builds a fresh VM, installs bindings, runs
    /// the chunk (defining `frame`/`init`/helpers as globals), runs `init()` once
    /// if present. Returns `LuaError{message,line?}` on compile/runtime failure.
    pub fn set_source(&mut self, src: &str) -> Result<(), LuaError> {
        let mut lua = Lua::core();
        lua.enter(install_bindings);

        let load = lua.try_enter(|ctx| {
            let closure = Closure::load(ctx, Some("source"), src.as_bytes())?;
            Ok(ctx.stash(Executor::start(ctx, closure.into(), ())))
        });
        let ex = load.map_err(static_error_to_lua)?;
        lua.execute::<()>(&ex).map_err(static_error_to_lua)?;

        let (frame_fn, init_fn) = lua.enter(|ctx| {
            let frame_fn = match ctx.get_global("frame") {
                Value::Function(f) => Some(ctx.stash(f)),
                _ => None,
            };
            let init_fn = match ctx.get_global("init") {
                Value::Function(f) => Some(ctx.stash(f)),
                _ => None,
            };
            (frame_fn, init_fn)
        });

        self.lua = Rc::new(RefCell::new(lua));
        self.frame_fn = frame_fn;
        self.init_fn = init_fn;
        self.memory = Memory::new();

        if let Some(init) = self.init_fn.clone() {
            let mut l = self.lua.borrow_mut();
            let ex = l.enter(|ctx| {
                let f = ctx.fetch(&init);
                ctx.stash(Executor::start(ctx, f, ()))
            });
            l.execute::<()>(&ex).map_err(static_error_to_lua)?;
        }
        Ok(())
    }

    /// Run one frame: call `frame(t,f)` once (bare assigns -> frame-wide defaults,
    /// `hdma` -> registered hooks), read CGRAM/OAM, then resolve the 224-row
    /// LineTable by applying each covering hook per scanline (later call wins).
    pub fn frame(&mut self, t: f64, f: u32) -> Result<LineTable, LuaError> {
        // Reset the per-frame hook registry, then run frame(t,f) once.
        {
            let mut l = self.lua.borrow_mut();
            l.enter(|ctx| {
                ctx.set_global("__ppu_hooks", Table::new(&ctx)).unwrap();
            });
            if let Some(frame) = self.frame_fn.clone() {
                let ex = l.enter(|ctx| {
                    let func = ctx.fetch(&frame);
                    ctx.stash(Executor::start(ctx, func, (t, f as i64)))
                });
                l.execute::<()>(&ex).map_err(static_error_to_lua)?;
            }
        }

        // Read frame-wide defaults + frame-global memory.
        let defaults = {
            let mut l = self.lua.borrow_mut();
            l.enter(|ctx| {
                read_memory(ctx, &mut self.memory);
                read_state(ctx)
            })
        };

        // Collect registered hooks (stash each fn with its [y0,y1]).
        let hooks: Vec<(usize, usize, StashedFunction)> = {
            let mut l = self.lua.borrow_mut();
            l.enter(|ctx| {
                let mut out = Vec::new();
                if let Value::Table(hk) = ctx.get_global("__ppu_hooks") {
                    let n = hk.length();
                    for idx in 1..=n {
                        if let Value::Table(entry) = hk.get(ctx, idx) {
                            let y0 = entry.get(ctx, 1).to_integer().unwrap_or(0).max(0) as usize;
                            let y1 = entry.get(ctx, 2).to_integer().unwrap_or(0).max(0) as usize;
                            if let Value::Function(func) = entry.get(ctx, 3) {
                                out.push((y0, y1, ctx.stash(func)));
                            }
                        }
                    }
                }
                out
            })
        };

        // Resolve the line table: each hook becomes a closure that re-baselines
        // globals to the working row, runs fn(y), and reads the row back.
        let err_sink: Rc<RefCell<Option<LuaError>>> = Rc::new(RefCell::new(None));
        let mut builder = LineTableBuilder::new(defaults.clone());
        for (y0, y1, sf) in hooks {
            let lua = self.lua.clone();
            let sink = err_sink.clone();
            builder.hdma(y0, y1, move |y, row| {
                if sink.borrow().is_some() {
                    return;
                }
                let mut l = lua.borrow_mut();
                l.enter(|ctx| write_state(ctx, row));
                let ex = l.enter(|ctx| {
                    let func = ctx.fetch(&sf);
                    ctx.stash(Executor::start(ctx, func, (y as i64,)))
                });
                match l.execute::<()>(&ex) {
                    Ok(()) => {
                        *row = l.enter(read_state);
                    }
                    Err(e) => {
                        *sink.borrow_mut() = Some(static_error_to_lua(e));
                    }
                }
            });
        }
        let lt = builder.build(HEIGHT);

        // Restore sticky globals to the frame-wide defaults (hooks mutated them).
        {
            let mut l = self.lua.borrow_mut();
            l.enter(|ctx| write_state(ctx, &defaults));
        }

        if let Some(e) = err_sink.borrow_mut().take() {
            return Err(e);
        }
        Ok(lt)
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
    for (k, v) in [
        ("a", 1.0),
        ("b", 0.0),
        ("c", 0.0),
        ("d", 1.0),
        ("cx", 0.0),
        ("cy", 0.0),
    ] {
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
        for name in [
            "sin", "cos", "tan", "floor", "ceil", "abs", "sqrt", "min", "max",
        ] {
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
            // `LineNumber` is 0-indexed; render it 1-based for the editor.
            PrototypeError::Parser(p) => Some(p.line_number.0 as u32 + 1),
            _ => None,
        })
    } else {
        None
    };
    LuaError {
        message: e.to_string(),
        line,
    }
}

fn value_to_string(v: Value<'_>) -> Option<String> {
    match v {
        Value::String(s) => Some(String::from_utf8_lossy(s.as_bytes()).into_owned()),
        _ => None,
    }
}

/// Read the per-scanline register globals into a `LineTableRow`. Missing globals
/// keep their `LineTableRow::default()` value (sticky semantics).
fn read_state(ctx: piccolo::Context<'_>) -> LineTableRow {
    let mut row = LineTableRow::default();
    if let Some(m) = ctx.get_global("mode").to_integer() {
        row.mode = m.clamp(0, 7) as u8;
    }
    if let Some(b) = ctx.get_global("brightness").to_integer() {
        row.brightness = b.clamp(0, 15) as u8;
    }
    if let Value::Table(bg) = ctx.get_global("bg") {
        for i in 0..4 {
            if let Value::Table(layer) = bg.get(ctx, (i + 1) as i64) {
                if let Value::Table(scroll) = layer.get(ctx, "scroll") {
                    if let Some(x) = scroll.get(ctx, "x").to_number() {
                        row.bg[i].scroll_x = x as f32;
                    }
                    if let Some(y) = scroll.get(ctx, "y").to_number() {
                        row.bg[i].scroll_y = y as f32;
                    }
                }
                row.bg[i].source = value_to_string(layer.get(ctx, "source"));
                row.bg[i].visible = match layer.get(ctx, "visible") {
                    Value::Nil => true,
                    v => v.to_bool(),
                };
            }
        }
    }
    if let Value::Table(m7) = ctx.get_global("m7") {
        if let Some(v) = m7.get(ctx, "a").to_number() {
            row.m7.a = v as f32;
        }
        if let Some(v) = m7.get(ctx, "b").to_number() {
            row.m7.b = v as f32;
        }
        if let Some(v) = m7.get(ctx, "c").to_number() {
            row.m7.c = v as f32;
        }
        if let Some(v) = m7.get(ctx, "d").to_number() {
            row.m7.d = v as f32;
        }
        if let Some(v) = m7.get(ctx, "cx").to_number() {
            row.m7.cx = v as f32;
        }
        if let Some(v) = m7.get(ctx, "cy").to_number() {
            row.m7.cy = v as f32;
        }
    }
    row
}

/// Write a `LineTableRow` back into the per-scanline register globals (used to
/// re-baseline globals before each hook and to restore sticky state after build).
fn write_state(ctx: piccolo::Context<'_>, row: &LineTableRow) {
    ctx.set_global("mode", row.mode as i64).unwrap();
    ctx.set_global("brightness", row.brightness as i64).unwrap();
    if let Value::Table(bg) = ctx.get_global("bg") {
        for i in 0..4 {
            if let Value::Table(layer) = bg.get(ctx, (i + 1) as i64) {
                if let Value::Table(scroll) = layer.get(ctx, "scroll") {
                    scroll.set(ctx, "x", row.bg[i].scroll_x as f64).unwrap();
                    scroll.set(ctx, "y", row.bg[i].scroll_y as f64).unwrap();
                }
                match &row.bg[i].source {
                    Some(s) => {
                        let interned = ctx.intern(s.as_bytes());
                        layer.set(ctx, "source", interned).unwrap();
                    }
                    None => {
                        layer.set(ctx, "source", Value::Nil).unwrap();
                    }
                };
                layer.set(ctx, "visible", row.bg[i].visible).unwrap();
            }
        }
    }
    if let Value::Table(m7) = ctx.get_global("m7") {
        m7.set(ctx, "a", row.m7.a as f64).unwrap();
        m7.set(ctx, "b", row.m7.b as f64).unwrap();
        m7.set(ctx, "c", row.m7.c as f64).unwrap();
        m7.set(ctx, "d", row.m7.d as f64).unwrap();
        m7.set(ctx, "cx", row.m7.cx as f64).unwrap();
        m7.set(ctx, "cy", row.m7.cy as f64).unwrap();
    }
}

/// Read cgram / obj / obj.sheet globals into `Memory` (frame-global, read once).
fn read_memory(ctx: piccolo::Context<'_>, mem: &mut Memory) {
    if let Value::Table(cg) = ctx.get_global("cgram") {
        for i in 0..256 {
            mem.cgram[i] = cg
                .get(ctx, i as i64)
                .to_integer()
                .map(|v| (v as u16) & 0x7fff)
                .unwrap_or(0);
        }
    }
    if let Value::Table(obj) = ctx.get_global("obj") {
        mem.obj_sheet = value_to_string(obj.get(ctx, "sheet"));
        for i in 0..128 {
            if let Value::Table(o) = obj.get(ctx, i as i64) {
                let e = &mut mem.oam[i];
                e.x = o.get(ctx, "x").to_number().unwrap_or(0.0) as f32;
                e.y = o.get(ctx, "y").to_number().unwrap_or(0.0) as f32;
                e.tile = o.get(ctx, "tile").to_integer().unwrap_or(0) as u16;
                e.pal = o.get(ctx, "pal").to_integer().unwrap_or(0) as u8;
                e.prio = o.get(ctx, "prio").to_integer().unwrap_or(0) as u8;
                e.size = o.get(ctx, "size").to_integer().unwrap_or(0) as u8;
                e.flip_x = o.get(ctx, "flip_x").to_bool();
                e.flip_y = o.get(ctx, "flip_y").to_bool();
                e.on = o.get(ctx, "on").to_bool();
            }
        }
    }
}
