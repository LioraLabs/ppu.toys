//! piccolo Lua VM + flat-global DSL binding. Runs `frame(t,f)` once to populate
//! frame-wide defaults + CGRAM/OAM, registers `hdma` hooks, then resolves the
//! LineTable by invoking each covering hook per scanline (later call wins).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use piccolo::{
    Callback, CallbackReturn, Closure, Executor, Function, Lua, PrototypeError, StashedFunction,
    StaticError, Table, Value,
};

use crate::import::obj::{apply_obj_import, import_obj_sheet, ObjImport};
use crate::import::{BudgetReport, ImportCache, ImportKey, ImportOptions};
use crate::import_m7::{import_mode7, Mode7Import, Mode7ImportReport};
use crate::{rgb15, LineTable, LineTableBuilder, LineTableRow, Memory, HEIGHT};

/// Decoded RGBA staged for the importer, keyed by slot id. App-level (survives
/// recompiles), unlike `Memory` which `set_source` resets. The importer
/// (m4/importer) quantizes it into real VRAM/CGRAM words when a layer binds it.
#[derive(Clone)]
pub struct ImportAsset {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// Bumped on every re-upload; part of the import cache key so a re-upload
    /// re-quantizes.
    pub generation: u64,
}

/// One BG layer's import budget from the most recent `frame()`, surfaced to the
/// UI (m4/inspector). Tagged by importer flavor.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "mode")]
pub enum ImportBudget {
    #[serde(rename = "tile")]
    Tile { layer: usize, report: BudgetReport },
    #[serde(rename = "m7")]
    Mode7 {
        layer: usize,
        report: Mode7ImportReport,
    },
    #[serde(rename = "obj")]
    Obj { report: BudgetReport },
}

/// Compile/runtime error surfaced to the editor, matching the TS `LuaError` shape.
#[derive(Debug, Clone, PartialEq)]
pub struct LuaError {
    pub message: String,
    pub line: Option<u32>,
    /// Source file the error is attributed to (multi-file sketches).
    pub file: Option<String>,
}

impl LuaError {
    /// Tag this error with the source file it belongs to.
    fn in_file(mut self, name: &str) -> LuaError {
        self.file = Some(name.to_string());
        self
    }
}

/// The embedded Lua VM plus the captured `frame`/`init` entry points and mirrored
/// PPU memory. Globals persist across frames (sticky registers).
pub struct LuaEngine {
    lua: Rc<RefCell<Lua>>,
    frame_fn: Option<StashedFunction>,
    init_fn: Option<StashedFunction>,
    /// Defining chunk of `frame_fn`, for runtime error attribution.
    frame_file: Option<String>,
    memory: Memory,
    /// Uploaded image assets, keyed by slot id. Consumed by the `source =`
    /// importer; NOT PPU memory (survives recompiles).
    assets: HashMap<String, ImportAsset>,
    /// Memoized tile-BG imports (m4/importer); keyed by asset+generation+options.
    import_cache: ImportCache,
    /// Memoized Mode 7 imports, keyed by (asset, generation).
    m7_cache: HashMap<(String, u64), Mode7Import>,
    /// Memoized OBJ-sheet imports (m4/importer), keyed by
    /// (asset, generation, snapped char_base) so re-upload / a char-base move
    /// re-quantizes but a hot 60fps key only re-copies words.
    obj_imports: HashMap<(String, u64, u16), ObjImport>,
    /// Monotonic upload counter feeding `ImportAsset::generation`.
    next_generation: u64,
    /// Per-layer import budgets produced by the most recent `frame()`.
    reports: Vec<ImportBudget>,
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
            frame_file: None,
            memory: Memory::new(),
            assets: HashMap::new(),
            import_cache: ImportCache::default(),
            m7_cache: HashMap::new(),
            obj_imports: HashMap::new(),
            next_generation: 0,
            reports: Vec::new(),
        }
    }

    /// Mirrored PPU memory after the most recent `frame()`.
    pub fn memory(&self) -> &Memory {
        &self.memory
    }

    /// Stage a decoded RGBA asset for the importer. Bumps the upload generation
    /// and drops stale cached imports so a re-upload re-quantizes. Malformed
    /// ImageData (zero dims or wrong buffer length) is ignored.
    pub fn upload_asset(&mut self, slot: String, width: u32, height: u32, rgba: Vec<u8>) {
        if width == 0 || height == 0 || rgba.len() != (width * height * 4) as usize {
            return;
        }
        self.next_generation += 1;
        let generation = self.next_generation;
        self.import_cache.invalidate_asset(&slot);
        self.m7_cache.retain(|(s, _), _| s != &slot);
        self.obj_imports.retain(|(s, _, _), _| s != &slot);
        self.assets.insert(
            slot,
            ImportAsset {
                width,
                height,
                rgba,
                generation,
            },
        );
    }

    /// Asset ids + dimensions, sorted by id (stable order for the inspector).
    pub fn assets(&self) -> Vec<(String, u32, u32)> {
        let mut v: Vec<_> = self
            .assets
            .iter()
            .map(|(id, a)| (id.clone(), a.width, a.height))
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    }

    /// Per-layer import budgets from the most recent `frame()` (m4/inspector).
    pub fn import_reports(&self) -> &[ImportBudget] {
        &self.reports
    }

    /// Mutable mirrored memory — used by the wasm shim (e.g. to clear OAM on-flags
    /// for the layer-visibility override).
    pub fn memory_mut(&mut self) -> &mut Memory {
        &mut self.memory
    }

    /// Single-file sugar for [`Self::set_sources`]; the chunk keeps its
    /// historical name `"source"` so existing diagnostics are unchanged.
    pub fn set_source(&mut self, src: &str) -> Result<(), LuaError> {
        self.set_sources(&[("source", src)])
    }

    /// Compile and load a multi-file sketch (PICO-8 scope): builds a fresh VM,
    /// installs bindings, then executes each `(name, source)` chunk **in list
    /// order** into ONE shared global environment, each compiled with its file
    /// name as chunk name. `frame`/`init` are resolved only after every chunk
    /// has run (`main.lua` is convention, not special-cased); `init()` runs
    /// once if present. Errors carry `{file, line?, message}`.
    pub fn set_sources(&mut self, files: &[(&str, &str)]) -> Result<(), LuaError> {
        let mut lua = Lua::core();
        lua.enter(install_bindings);

        for (name, src) in files {
            let load = lua.try_enter(|ctx| {
                let closure = Closure::load(ctx, Some(*name), src.as_bytes())?;
                Ok(ctx.stash(Executor::start(ctx, closure.into(), ())))
            });
            if let Err(e) = load.and_then(|ex| lua.execute::<()>(&ex)) {
                return Err(static_error_to_lua(e).in_file(name));
            }
        }

        let (frame_fn, frame_file, init_fn, init_file) = lua.enter(|ctx| {
            let (frame_fn, frame_file) = match ctx.get_global("frame") {
                Value::Function(f) => (Some(ctx.stash(f)), function_chunk_name(&f)),
                _ => (None, None),
            };
            let (init_fn, init_file) = match ctx.get_global("init") {
                Value::Function(f) => (Some(ctx.stash(f)), function_chunk_name(&f)),
                _ => (None, None),
            };
            (frame_fn, frame_file, init_fn, init_file)
        });

        self.lua = Rc::new(RefCell::new(lua));
        self.frame_fn = frame_fn;
        self.frame_file = frame_file;
        self.init_fn = init_fn;
        self.memory = Memory::new();

        if let Some(init) = self.init_fn.clone() {
            let mut l = self.lua.borrow_mut();
            let ex = l.enter(|ctx| {
                let f = ctx.fetch(&init);
                ctx.stash(Executor::start(ctx, f, ()))
            });
            l.execute::<()>(&ex).map_err(|e| {
                let mut err = static_error_to_lua(e);
                err.file = init_file.clone();
                err
            })?;
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
                l.execute::<()>(&ex).map_err(|e| {
                    let mut err = static_error_to_lua(e);
                    err.file = self.frame_file.clone();
                    err
                })?;
            }
        }

        // Read frame-wide defaults + frame-global memory. VRAM is rebuilt fresh
        // each frame so the flush order is deterministic: zero -> imports
        // (source= bootstrap) -> structured pokes -> raw vram[] (final authority).
        let defaults = {
            let mut l = self.lua.borrow_mut();
            l.enter(|ctx| {
                self.memory.vram = [0u16; 0x8000];
                self.memory.cgram = [0u16; 256];
                apply_imports(
                    ctx,
                    &self.assets,
                    &mut self.import_cache,
                    &mut self.m7_cache,
                    &mut self.reports,
                    &mut self.memory,
                );
                apply_obj_sheet(
                    ctx,
                    &self.assets,
                    &mut self.obj_imports,
                    &mut self.reports,
                    &mut self.memory,
                );
                read_memory(ctx, &mut self.memory);
                read_state(ctx)
            })
        };

        // Collect registered hooks (stash each fn with its [y0,y1]).
        let hooks: Vec<(usize, usize, StashedFunction, Option<String>)> = {
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
                                let file = function_chunk_name(&func);
                                out.push((y0, y1, ctx.stash(func), file));
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
        for (y0, y1, sf, file) in hooks {
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
                        let mut err = static_error_to_lua(e);
                        err.file = file.clone();
                        *sink.borrow_mut() = Some(err);
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
    // MOSAIC ($2106): global block size 0..15; per-layer enable via bg[n].mosaic.
    ctx.set_global("mosaic", 0).unwrap();
    ctx.set_global("direct_color", false).unwrap();
    ctx.set_global("force_blank", false).unwrap();
    // TM/TS main/sub screen designation ($212C/$212D). Playground defaults:
    // all five layers on the main screen (like brightness=15/visible=true),
    // nothing on the sub screen (authentic power-on).
    ctx.set_global("TM", 0x1f).unwrap();
    ctx.set_global("TS", 0x00).unwrap();

    // Window-mask registers ($2123-$212F). Power-on: all zero -> no window
    // enabled, no layer clipped (existing goldens unaffected).
    for name in [
        "WH0", "WH1", "WH2", "WH3", "W12SEL", "W34SEL", "WOBJSEL", "WBGLOG", "WOBJLOG", "TMW",
        "TSW", "CGWSEL", "CGADSUB", "COLDATA",
    ] {
        ctx.set_global(name, 0).unwrap();
    }

    // bg[1..4] = { scroll = {x,y}, source=nil, visible=true }
    let bg = Table::new(&ctx);
    for i in 1..=4i64 {
        let layer = Table::new(&ctx);
        let scroll = Table::new(&ctx);
        scroll.set(ctx, "x", 0.0).unwrap();
        scroll.set(ctx, "y", 0.0).unwrap();
        layer.set(ctx, "scroll", scroll).unwrap();
        layer.set(ctx, "visible", true).unwrap();
        // Binding registers (BGMODE tile-size / BGnSC / BGnNBA), quantize-on-write.
        layer.set(ctx, "tile_size", 8).unwrap();
        layer.set(ctx, "map_base", 0).unwrap();
        layer.set(ctx, "screen_size", 0).unwrap();
        layer.set(ctx, "char_base", 0).unwrap();
        layer.set(ctx, "mosaic", false).unwrap();
        // Per-cell tilemap poke surface: map[col][row] = {tile,pal,prio,flip_x,flip_y}.
        layer.set(ctx, "map", Table::new(&ctx)).unwrap();
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
    // M7SEL binding registers. `wrap` names M7SEL's screen-over field (spec's
    // `m7.repeat`, renamed because `repeat` is a reserved Lua keyword).
    m7.set(ctx, "wrap", 0).unwrap();
    m7.set(ctx, "flip_x", false).unwrap();
    m7.set(ctx, "flip_y", false).unwrap();
    m7.set(ctx, "extbg", false).unwrap();
    // Mode 7 tilemap poke: m7.map[ty][tx] = tile# (low byte of the interleaved word).
    m7.set(ctx, "map", Table::new(&ctx)).unwrap();
    ctx.set_global("m7", m7).unwrap();

    // Hidden Mode 7 char buffer, keyed `__m7char[tile][fy*8+fx] = index`, filled
    // by `m7pixel` and flushed into the high byte lane at frame time.
    ctx.set_global("__m7char", Table::new(&ctx)).unwrap();

    // m7pixel(tile, x, y, index): stage a Mode 7 char pixel (8bpp linear). The
    // flush masks the high VRAM byte lane, leaving the tilemap low byte intact
    // (raw `vram[]` sets both lanes at once; this helper touches one).
    let m7pixel = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let tile = stack.get(0).to_integer().unwrap_or(0);
        let x = stack.get(1).to_integer().unwrap_or(0);
        let y = stack.get(2).to_integer().unwrap_or(0);
        let idx = stack.get(3).to_integer().unwrap_or(0);
        stack.clear();
        if let Value::Table(cb) = ctx.get_global("__m7char") {
            let sub = match cb.get(ctx, tile) {
                Value::Table(t) => t,
                _ => {
                    let t = Table::new(&ctx);
                    cb.set(ctx, tile, t).unwrap();
                    t
                }
            };
            sub.set(ctx, (y & 7) * 8 + (x & 7), idx).unwrap();
        }
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("m7pixel", m7pixel).unwrap();

    // cgram (scalar table, written by user)
    ctx.set_global("cgram", Table::new(&ctx)).unwrap();

    // vram (raw 16-bit word poke table: vram[addr] = word, 0..0x7FFF)
    ctx.set_global("vram", Table::new(&ctx)).unwrap();

    // obj[0..127] + obj.sheet
    let obj = Table::new(&ctx);
    for i in 0..128i64 {
        let o = Table::new(&ctx);
        for (k, v) in [("x", 0.0), ("y", 0.0)] {
            o.set(ctx, k, v).unwrap();
        }
        for k in ["tile", "pal", "prio"] {
            o.set(ctx, k, 0).unwrap();
        }
        for k in ["flip_x", "flip_y", "on", "large"] {
            o.set(ctx, k, false).unwrap();
        }
        obj.set(ctx, i, o).unwrap();
    }
    obj.set(ctx, "priority_rotate", false).unwrap();
    obj.set(ctx, "oam_addr", 0).unwrap();
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

    // coldata(byte): authentic $2132 accumulation. bit5=R, bit6=G, bit7=B select
    // which channels a 5-bit value (bits0-4) overwrites; other channels persist.
    // Reads/writes the COLDATA global so it composes with a direct `COLDATA = rgb(..)`.
    let coldata = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let byte = stack.get(0).to_integer().unwrap_or(0) as u16;
        stack.clear();
        let cur = ctx.get_global("COLDATA").to_integer().unwrap_or(0) as u16 & 0x7fff;
        let v = byte & 0x1f;
        let mut r = cur & 0x1f;
        let mut g = (cur >> 5) & 0x1f;
        let mut b = (cur >> 10) & 0x1f;
        if byte & 0x20 != 0 {
            r = v;
        }
        if byte & 0x40 != 0 {
            g = v;
        }
        if byte & 0x80 != 0 {
            b = v;
        }
        ctx.set_global("COLDATA", ((b << 10) | (g << 5) | r) as i64)
            .unwrap();
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("coldata", coldata).unwrap();

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

    // Friendly color-math namespace over CGWSEL/CGADSUB/COLDATA.
    // `__color_base` records the packed bytes at each (re-)baseline so
    // read_state can tell WHICH side (friendly field vs raw mnemonic) the
    // user moved — see the fold in read_state.
    let color = Table::new(&ctx);
    color.set(ctx, "on", Table::new(&ctx)).unwrap();
    ctx.set_global("color", color).unwrap();
    ctx.set_global("__color_base", Table::new(&ctx)).unwrap();
    sync_color(ctx, 0, 0, 0); // power-on defaults = decode of zeroed registers

    // Friendly screen-designation namespace over TM/TS ($212C/$212D). Same
    // baseline change-detection pattern as `color` above; `__screen_base`
    // records the bytes at each (re-)baseline.
    let screen = Table::new(&ctx);
    screen.set(ctx, "main", Table::new(&ctx)).unwrap();
    screen.set(ctx, "sub", Table::new(&ctx)).unwrap();
    ctx.set_global("screen", screen).unwrap();
    ctx.set_global("__screen_base", Table::new(&ctx)).unwrap();
    sync_screen(ctx, 0x1f, 0x00); // decode of the playground power-on TM/TS

    // Friendly window namespace over WH0-3, W12SEL/W34SEL/WOBJSEL, WBGLOG/
    // WOBJLOG, TMW/TSW. Same baseline change-detection pattern as `color`/
    // `screen` above; `__win_base` records the eleven bytes at each
    // (re-)baseline. Overlap with `color`: win.color.* selects
    // WHICH pixels form the color window (WOBJSEL high nibble + a WOBJLOG
    // slot); WHERE color math is prevented (CGWSEL bits 4-5) stays owned by
    // color.region. `win` never writes CGWSEL, and the color window has no
    // TMW/TSW bit.
    let win = Table::new(&ctx);
    win.set(ctx, "w1", Table::new(&ctx)).unwrap();
    win.set(ctx, "w2", Table::new(&ctx)).unwrap();
    for l in &WIN_LAYERS {
        win.set(ctx, l.name, Table::new(&ctx)).unwrap();
    }
    ctx.set_global("win", win).unwrap();
    ctx.set_global("__win_base", Table::new(&ctx)).unwrap();
    sync_win(ctx, &[0u8; 11]); // power-on: every window register is zero
}

/// Chunk (source file) name a Lua function was compiled from; `None` for
/// native callbacks. Basis of runtime per-file error attribution (piccolo
/// 0.3.3 has no tracebacks, so we attribute to the defining chunk).
fn function_chunk_name(f: &Function<'_>) -> Option<String> {
    match f {
        Function::Closure(c) => {
            Some(String::from_utf8_lossy(c.prototype().chunk_name.as_bytes()).into_owned())
        }
        _ => None,
    }
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
        file: None,
    }
}

fn value_to_string(v: Value<'_>) -> Option<String> {
    match v {
        Value::String(s) => Some(String::from_utf8_lossy(s.as_bytes()).into_owned()),
        _ => None,
    }
}

/// CGWSEL bits owned by the friendly `color` namespace: bit1 addend,
/// bits 4-5 prevent-math region. Bit 0 (direct_color) and bits 6-7
/// (clip-to-black) are NOT color's — leave them to the raw byte.
const COLOR_CGWSEL_MASK: u8 = 0x32;

/// The CGWSEL/CGADSUB/COLDATA bytes as of the last install_bindings/
/// write_state — the baseline the friendly `color` fold diffs against.
fn read_color_base(ctx: piccolo::Context<'_>) -> (u8, u8, u16) {
    match ctx.get_global("__color_base") {
        Value::Table(t) => (
            t.get(ctx, "cgwsel").to_integer().unwrap_or(0) as u8,
            t.get(ctx, "cgadsub").to_integer().unwrap_or(0) as u8,
            t.get(ctx, "coldata").to_integer().unwrap_or(0) as u16,
        ),
        _ => (0, 0, 0),
    }
}

/// Pack the friendly `color` table into (cgwsel-bits, cgadsub, coldata).
/// Any field that is nil/unrecognized takes its bits from `base` (absent
/// friendly -> the raw register keeps those bits, i.e. "no change").
fn pack_color(ctx: piccolo::Context<'_>, base: (u8, u8, u16)) -> (u8, u8, u16) {
    let (base_w, base_a, base_c) = base;
    let Value::Table(color) = ctx.get_global("color") else {
        return base;
    };
    let bit = |v: Value<'_>, mask: u8, base_byte: u8| -> u8 {
        match v {
            Value::Boolean(true) => mask,
            Value::Boolean(false) => 0,
            _ => base_byte & mask,
        }
    };
    let mut a = match value_to_string(color.get(ctx, "op")).as_deref() {
        Some("add") => 0,
        Some("sub") => 0x80,
        _ => base_a & 0x80,
    };
    a |= bit(color.get(ctx, "half"), 0x40, base_a);
    if let Value::Table(on) = color.get(ctx, "on") {
        for (name, mask) in [
            ("bg1", 0x01),
            ("bg2", 0x02),
            ("bg3", 0x04),
            ("bg4", 0x08),
            ("obj", 0x10),
            ("backdrop", 0x20),
        ] {
            a |= bit(on.get(ctx, name), mask, base_a);
        }
    } else {
        a |= base_a & 0x3f;
    }
    let mut w = match value_to_string(color.get(ctx, "addend")).as_deref() {
        Some("sub") => 0x02,
        Some("fixed") => 0,
        _ => base_w & 0x02,
    };
    w |= match value_to_string(color.get(ctx, "region")).as_deref() {
        Some("everywhere") => 0x00,
        Some("inside") => 0x10,  // prevent-math OUTSIDE the window
        Some("outside") => 0x20, // prevent-math INSIDE the window
        Some("never") => 0x30,   // always prevent
        _ => base_w & 0x30,
    };
    let c = match color.get(ctx, "fixed").to_integer() {
        Some(v) => (v as u16) & 0x7fff,
        None => base_c,
    };
    (w, a, c)
}

/// Unpack CGWSEL/CGADSUB/COLDATA into the friendly `color` fields (mirror
/// live values so hooks can read them — HDMA persistence, matching m7) and
/// record the bytes in `__color_base` for read_state's change detection.
fn sync_color(ctx: piccolo::Context<'_>, cgwsel: u8, cgadsub: u8, coldata: u16) {
    if let Value::Table(color) = ctx.get_global("color") {
        let s = |v: &str| ctx.intern(v.as_bytes());
        color
            .set(
                ctx,
                "op",
                s(if cgadsub & 0x80 != 0 { "sub" } else { "add" }),
            )
            .unwrap();
        color.set(ctx, "half", cgadsub & 0x40 != 0).unwrap();
        if let Value::Table(on) = color.get(ctx, "on") {
            for (name, mask) in [
                ("bg1", 0x01u8),
                ("bg2", 0x02),
                ("bg3", 0x04),
                ("bg4", 0x08),
                ("obj", 0x10),
                ("backdrop", 0x20),
            ] {
                on.set(ctx, name, cgadsub & mask != 0).unwrap();
            }
        }
        color
            .set(
                ctx,
                "addend",
                s(if cgwsel & 0x02 != 0 { "sub" } else { "fixed" }),
            )
            .unwrap();
        let region = match (cgwsel >> 4) & 0x03 {
            0 => "everywhere",
            1 => "inside",
            2 => "outside",
            _ => "never",
        };
        color.set(ctx, "region", s(region)).unwrap();
        color.set(ctx, "fixed", (coldata & 0x7fff) as i64).unwrap();
    }
    if let Value::Table(b) = ctx.get_global("__color_base") {
        b.set(ctx, "cgwsel", cgwsel as i64).unwrap();
        b.set(ctx, "cgadsub", cgadsub as i64).unwrap();
        b.set(ctx, "coldata", (coldata & 0x7fff) as i64).unwrap();
    }
}

/// TM/TS bits owned by the friendly `screen` namespace: bits 0-4 =
/// BG1..BG4, OBJ. Bits 5-7 are unused by hardware — leave them to the raw byte.
const SCREEN_MASK: u8 = 0x1f;

/// The five TM/TS layer-enable fields, LSB-first.
const SCREEN_LAYERS: [(&str, u8); 5] = [
    ("bg1", 0x01),
    ("bg2", 0x02),
    ("bg3", 0x04),
    ("bg4", 0x08),
    ("obj", 0x10),
];

/// The TM/TS bytes as of the last install_bindings/write_state — the
/// baseline the friendly `screen` fold diffs against.
fn read_screen_base(ctx: piccolo::Context<'_>) -> (u8, u8) {
    match ctx.get_global("__screen_base") {
        Value::Table(t) => (
            t.get(ctx, "tm").to_integer().unwrap_or(0) as u8,
            t.get(ctx, "ts").to_integer().unwrap_or(0) as u8,
        ),
        _ => (0, 0),
    }
}

/// Pack the friendly `screen` table into (tm, ts). Any field that is
/// nil/non-boolean takes its bits from `base` (absent friendly -> the raw
/// register keeps those bits, i.e. "no change").
fn pack_screen(ctx: piccolo::Context<'_>, base: (u8, u8)) -> (u8, u8) {
    fn side<'gc>(
        ctx: piccolo::Context<'gc>,
        screen: Table<'gc>,
        name: &'static str,
        base_byte: u8,
    ) -> u8 {
        let Value::Table(t) = screen.get(ctx, name) else {
            return base_byte & SCREEN_MASK;
        };
        let mut out = 0u8;
        for (field, mask) in SCREEN_LAYERS {
            out |= match t.get(ctx, field) {
                Value::Boolean(true) => mask,
                Value::Boolean(false) => 0,
                _ => base_byte & mask,
            };
        }
        out
    }
    let (base_tm, base_ts) = base;
    let Value::Table(screen) = ctx.get_global("screen") else {
        return base;
    };
    (
        side(ctx, screen, "main", base_tm),
        side(ctx, screen, "sub", base_ts),
    )
}

/// Unpack TM/TS into the friendly `screen` fields (mirror live values so
/// hooks can read them — HDMA persistence, matching `color`) and record the
/// bytes in `__screen_base` for read_state's change detection.
fn sync_screen(ctx: piccolo::Context<'_>, tm: u8, ts: u8) {
    if let Value::Table(screen) = ctx.get_global("screen") {
        for (name, byte) in [("main", tm), ("sub", ts)] {
            if let Value::Table(t) = screen.get(ctx, name) {
                for (field, mask) in SCREEN_LAYERS {
                    t.set(ctx, field, byte & mask != 0).unwrap();
                }
            }
        }
    }
    if let Value::Table(b) = ctx.get_global("__screen_base") {
        b.set(ctx, "tm", tm as i64).unwrap();
        b.set(ctx, "ts", ts as i64).unwrap();
    }
}

/// Friendly `win` byte order (indexes `WinBytes`, `WIN_BASE_KEYS`,
/// `WIN_MASKS`): WH0-3, W12SEL, W34SEL, WOBJSEL, WBGLOG, WOBJLOG, TMW, TSW.
type WinBytes = [u8; 11];

const WIN_BASE_KEYS: [&str; 11] = [
    "wh0", "wh1", "wh2", "wh3", "w12sel", "w34sel", "wobjsel", "wbglog", "wobjlog", "tmw", "tsw",
];

/// Bits `win` owns per byte. The WH edges, the three SEL bytes and WBGLOG
/// are fully covered by friendly fields; WOBJLOG's bits 4-7 are unused by
/// hardware and TMW/TSW's bits 5-7 likewise — those stay raw-only.
const WIN_MASKS: WinBytes = [
    0xff,
    0xff,
    0xff,
    0xff,
    0xff,
    0xff,
    0xff,
    0xff,
    0x0f,
    SCREEN_MASK,
    SCREEN_MASK,
];

/// SEL-nibble layout, LSB first: W1 invert, W1 enable, W2 invert, W2 enable
/// (mirrors ENABLE_BITS/INVERT_BITS in web inspector/compose/model.ts).
const WIN_W1_ENABLE: u8 = 0x2;
const WIN_W2_ENABLE: u8 = 0x8;
const WIN_INVERT_BITS: u8 = 0x5;

/// WBGLOG/WOBJLOG 2-bit slot values, in order.
const WIN_COMBINE: [&str; 4] = ["OR", "AND", "XOR", "XNOR"];

/// One friendly window layer: its SEL nibble, LOG slot and TMW/TSW bit.
/// `sel`/`log` offset into the WinBytes SEL (4..=6) / LOG (7..=8) bytes.
struct WinLayer {
    name: &'static str,
    sel: usize,
    sel_shift: u8,
    log: usize,
    log_shift: u8,
    /// TMW/TSW bit; None for the color window (no mask-enable bit in hw).
    mask_bit: Option<u8>,
}

#[rustfmt::skip]
const WIN_LAYERS: [WinLayer; 6] = [
    WinLayer { name: "bg1",   sel: 0, sel_shift: 0, log: 0, log_shift: 0, mask_bit: Some(0) },
    WinLayer { name: "bg2",   sel: 0, sel_shift: 4, log: 0, log_shift: 2, mask_bit: Some(1) },
    WinLayer { name: "bg3",   sel: 1, sel_shift: 0, log: 0, log_shift: 4, mask_bit: Some(2) },
    WinLayer { name: "bg4",   sel: 1, sel_shift: 4, log: 0, log_shift: 6, mask_bit: Some(3) },
    WinLayer { name: "obj",   sel: 2, sel_shift: 0, log: 1, log_shift: 0, mask_bit: Some(4) },
    WinLayer { name: "color", sel: 2, sel_shift: 4, log: 1, log_shift: 2, mask_bit: None },
];

/// win.w1/.w2 edge fields in WinBytes order (WH0, WH1, WH2, WH3).
const WIN_EDGES: [(&str, &str); 4] = [("w1", "lo"), ("w1", "hi"), ("w2", "lo"), ("w2", "hi")];

/// The eleven window-register bytes as of the last install_bindings/
/// write_state — the baseline the friendly `win` fold diffs against.
fn read_win_base(ctx: piccolo::Context<'_>) -> WinBytes {
    let mut out = [0u8; 11];
    if let Value::Table(t) = ctx.get_global("__win_base") {
        for (i, k) in WIN_BASE_KEYS.iter().enumerate() {
            out[i] = t.get(ctx, *k).to_integer().unwrap_or(0) as u8;
        }
    }
    out
}

/// Pack the friendly `win` table into the eleven register bytes. Any field
/// that is nil/unrecognized takes its bits from `base` (absent friendly ->
/// the raw register keeps those bits, i.e. "no change"). The shared invert
/// pair is base-aware: decode is lossy (either bit set reads true), so an
/// UNCHANGED bool reproduces the base bits verbatim — only a moved bool
/// expands to both bits / neither.
fn pack_win(ctx: piccolo::Context<'_>, base: &WinBytes) -> WinBytes {
    let mut out = *base;
    let Value::Table(win) = ctx.get_global("win") else {
        return out;
    };
    let bit = |v: Value<'_>, mask: u8, base_bits: u8| -> u8 {
        match v {
            Value::Boolean(true) => mask,
            Value::Boolean(false) => 0,
            _ => base_bits & mask,
        }
    };
    for (i, (w, edge)) in WIN_EDGES.iter().enumerate() {
        if let Value::Table(t) = win.get(ctx, *w) {
            if let Some(v) = t.get(ctx, *edge).to_integer() {
                out[i] = v as u8;
            }
        }
    }
    for l in &WIN_LAYERS {
        let Value::Table(t) = win.get(ctx, l.name) else {
            continue;
        };
        let sel_i = 4 + l.sel;
        let base_nib = (base[sel_i] >> l.sel_shift) & 0xf;
        let mut nib = bit(t.get(ctx, "w1"), WIN_W1_ENABLE, base_nib)
            | bit(t.get(ctx, "w2"), WIN_W2_ENABLE, base_nib);
        nib |= match t.get(ctx, "invert") {
            Value::Boolean(b) if b != (base_nib & WIN_INVERT_BITS != 0) => {
                if b {
                    WIN_INVERT_BITS
                } else {
                    0
                }
            }
            _ => base_nib & WIN_INVERT_BITS,
        };
        out[sel_i] = (out[sel_i] & !(0xf << l.sel_shift)) | (nib << l.sel_shift);
        let log_i = 7 + l.log;
        let base_slot = (base[log_i] >> l.log_shift) & 0x3;
        let slot = value_to_string(t.get(ctx, "combine"))
            .and_then(|s| WIN_COMBINE.iter().position(|c| *c == s))
            .map_or(base_slot, |p| p as u8);
        out[log_i] = (out[log_i] & !(0x3 << l.log_shift)) | (slot << l.log_shift);
        if let Some(b) = l.mask_bit {
            let m = 1u8 << b;
            out[9] = (out[9] & !m) | bit(t.get(ctx, "main"), m, base[9]);
            out[10] = (out[10] & !m) | bit(t.get(ctx, "sub"), m, base[10]);
        }
    }
    out
}

/// Unpack the window registers into the friendly `win` fields (mirror live
/// values so hooks can read them — HDMA persistence, matching `color`/
/// `screen`) and record the bytes in `__win_base` for read_state's change
/// detection. Shared decode: `invert` reads true if EITHER invert bit is set.
fn sync_win(ctx: piccolo::Context<'_>, bytes: &WinBytes) {
    if let Value::Table(win) = ctx.get_global("win") {
        for (i, (w, edge)) in WIN_EDGES.iter().enumerate() {
            if let Value::Table(t) = win.get(ctx, *w) {
                t.set(ctx, *edge, bytes[i] as i64).unwrap();
            }
        }
        for l in &WIN_LAYERS {
            let Value::Table(t) = win.get(ctx, l.name) else {
                continue;
            };
            let nib = (bytes[4 + l.sel] >> l.sel_shift) & 0xf;
            t.set(ctx, "w1", nib & WIN_W1_ENABLE != 0).unwrap();
            t.set(ctx, "w2", nib & WIN_W2_ENABLE != 0).unwrap();
            t.set(ctx, "invert", nib & WIN_INVERT_BITS != 0).unwrap();
            let slot = (bytes[7 + l.log] >> l.log_shift) & 0x3;
            t.set(
                ctx,
                "combine",
                ctx.intern(WIN_COMBINE[slot as usize].as_bytes()),
            )
            .unwrap();
            if let Some(b) = l.mask_bit {
                t.set(ctx, "main", bytes[9] & (1 << b) != 0).unwrap();
                t.set(ctx, "sub", bytes[10] & (1 << b) != 0).unwrap();
            }
        }
    }
    if let Value::Table(base) = ctx.get_global("__win_base") {
        for (i, k) in WIN_BASE_KEYS.iter().enumerate() {
            base.set(ctx, *k, bytes[i] as i64).unwrap();
        }
    }
}

/// The eleven `win`-owned bytes of a row, in WinBytes order.
fn row_win_bytes(row: &LineTableRow) -> WinBytes {
    [
        row.wh0,
        row.wh1,
        row.wh2,
        row.wh3,
        row.w12sel,
        row.w34sel,
        row.wobjsel,
        row.wbglog,
        row.wobjlog,
        row.tmw,
        row.tsw,
    ]
}

/// Read the per-scanline register globals into a `LineTableRow`. Missing globals
/// keep their `LineTableRow::default()` value (sticky semantics).
fn read_state(ctx: piccolo::Context<'_>) -> LineTableRow {
    let mut row = LineTableRow::default();
    if let Some(m) = ctx.get_global("mode").to_integer() {
        row.mode = m as u8; // wrap; quantize::mode masks to 3 bits at build
    }
    if let Some(b) = ctx.get_global("brightness").to_integer() {
        row.brightness = b as u8; // wrap; quantize::brightness masks to 4 bits
    }
    if let Some(v) = ctx.get_global("TM").to_integer() {
        row.tm = v as u8; // wrap; quantize::screen_mask masks to 5 bits at build
    }
    if let Some(v) = ctx.get_global("TS").to_integer() {
        row.ts = v as u8;
    }
    // Friendly `screen.*` fold — same coexistence contract as the `color`
    // fold below: XOR the packed friendly fields against the `__screen_base`
    // baseline recorded by install_bindings/write_state. Bits the user moved
    // via screen.main/.sub are authoritative (set AND clear, and beat a
    // same-cycle raw write); untouched bits keep the raw TM/TS byte, so
    // raw-only scripts (incl. inside hooks) and the both-off power-on state
    // stay byte-identical.
    let sbase = read_screen_base(ctx);
    let (f_tm, f_ts) = pack_screen(ctx, sbase);
    let changed_tm = (f_tm ^ sbase.0) & SCREEN_MASK;
    row.tm = (row.tm & !changed_tm) | (f_tm & changed_tm);
    let changed_ts = (f_ts ^ sbase.1) & SCREEN_MASK;
    row.ts = (row.ts & !changed_ts) | (f_ts & changed_ts);
    if let Some(v) = ctx.get_global("WH0").to_integer() {
        row.wh0 = v as u8;
    }
    if let Some(v) = ctx.get_global("WH1").to_integer() {
        row.wh1 = v as u8;
    }
    if let Some(v) = ctx.get_global("WH2").to_integer() {
        row.wh2 = v as u8;
    }
    if let Some(v) = ctx.get_global("WH3").to_integer() {
        row.wh3 = v as u8;
    }
    if let Some(v) = ctx.get_global("W12SEL").to_integer() {
        row.w12sel = v as u8;
    }
    if let Some(v) = ctx.get_global("W34SEL").to_integer() {
        row.w34sel = v as u8;
    }
    if let Some(v) = ctx.get_global("WOBJSEL").to_integer() {
        row.wobjsel = v as u8;
    }
    if let Some(v) = ctx.get_global("WBGLOG").to_integer() {
        row.wbglog = v as u8;
    }
    if let Some(v) = ctx.get_global("WOBJLOG").to_integer() {
        row.wobjlog = v as u8;
    }
    if let Some(v) = ctx.get_global("TMW").to_integer() {
        row.tmw = v as u8;
    }
    if let Some(v) = ctx.get_global("TSW").to_integer() {
        row.tsw = v as u8;
    }
    // Friendly `win.*` fold — same coexistence contract as the `screen`/
    // `color` folds: XOR the packed friendly bytes against the `__win_base`
    // baseline recorded by install_bindings/write_state; only bits the user
    // moved through `win` override the raw mnemonics (set AND clear, and
    // beat a same-cycle raw write), masked so raw-only bits (WOBJLOG 4-7,
    // TMW/TSW 5-7) pass through untouched. Exception: the WH edges (indices
    // 0..3) are scalar coordinates, not bitfields — a moved friendly edge
    // replaces the byte whole (the COLDATA precedent), never a bitwise blend.
    let wbase = read_win_base(ctx);
    let fwin = pack_win(ctx, &wbase);
    let mut wrow = row_win_bytes(&row);
    for (i, b) in wrow.iter_mut().enumerate() {
        let changed = (fwin[i] ^ wbase[i]) & WIN_MASKS[i];
        if i < 4 {
            // WH edges are scalar coordinates, not bitfields: whole-value
            // change detection (the COLDATA precedent) — a moved friendly
            // edge replaces the byte outright, never bit-blends with a
            // same-frame raw write.
            if changed != 0 {
                *b = fwin[i];
            }
        } else {
            *b = (*b & !changed) | (fwin[i] & changed);
        }
    }
    row.wh0 = wrow[0];
    row.wh1 = wrow[1];
    row.wh2 = wrow[2];
    row.wh3 = wrow[3];
    row.w12sel = wrow[4];
    row.w34sel = wrow[5];
    row.wobjsel = wrow[6];
    row.wbglog = wrow[7];
    row.wobjlog = wrow[8];
    row.tmw = wrow[9];
    row.tsw = wrow[10];
    if let Some(v) = ctx.get_global("CGWSEL").to_integer() {
        row.cgwsel = v as u8;
    }
    // Friendly alias: direct_color=true forces CGWSEL bit 0 (raw CGWSEL still works;
    // OR keeps both authoring styles valid and both-off byte-identical).
    if ctx.get_global("direct_color").to_bool() {
        row.cgwsel |= 0x01;
    }
    row.force_blank = ctx.get_global("force_blank").to_bool();
    if let Some(v) = ctx.get_global("CGADSUB").to_integer() {
        row.cgadsub = v as u8;
    }
    if let Some(v) = ctx.get_global("COLDATA").to_integer() {
        row.coldata = v as u16;
    }
    // Friendly `color.*` fold — the coexistence contract, generalizing the
    // direct_color precedent above to sets-AND-clears: XOR the packed friendly
    // fields against the `__color_base` baseline recorded by install_bindings/
    // write_state. Bits the user moved via the friendly fields this cycle are
    // authoritative (set AND clear, and beat a same-cycle raw write); bits the
    // friendly side did not move keep the raw CGWSEL/CGADSUB/COLDATA byte, so
    // raw-only scripts (incl. inside hooks) and the both-off power-on state
    // stay byte-identical. Known limit: re-assigning a friendly field its
    // current value is indistinguishable from not touching it.
    let base = read_color_base(ctx);
    let (f_w, f_a, f_c) = pack_color(ctx, base);
    let changed_w = (f_w ^ base.0) & COLOR_CGWSEL_MASK;
    row.cgwsel = (row.cgwsel & !changed_w) | (f_w & changed_w);
    let changed_a = f_a ^ base.1;
    row.cgadsub = (row.cgadsub & !changed_a) | (f_a & changed_a);
    if f_c != base.2 {
        row.coldata = f_c;
    }
    if let Some(v) = ctx.get_global("mosaic").to_integer() {
        row.mosaic_size = v as u8; // wrap; quantize::mosaic_size masks to 4 bits at build
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
                // Binding registers (quantize-on-write at RegRow build time).
                if let Some(v) = layer.get(ctx, "tile_size").to_integer() {
                    row.bg[i].tile_size = v as u8;
                }
                if let Some(v) = layer.get(ctx, "map_base").to_integer() {
                    row.bg[i].map_base = v as u32;
                }
                if let Some(v) = layer.get(ctx, "screen_size").to_integer() {
                    row.bg[i].screen_size = v as u8;
                }
                if let Some(v) = layer.get(ctx, "char_base").to_integer() {
                    row.bg[i].char_base = v as u32;
                }
                // MOSAIC per-BG enable; unset/nil -> false (off, matches default).
                row.mosaic_enable[i] = layer.get(ctx, "mosaic").to_bool();
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
        // M7SEL binding registers (`wrap` = spec's `m7.repeat`, keyword-renamed).
        if let Some(v) = m7.get(ctx, "wrap").to_integer() {
            row.m7.repeat = v as u8;
        }
        row.m7.flip_x = m7.get(ctx, "flip_x").to_bool();
        row.m7.flip_y = m7.get(ctx, "flip_y").to_bool();
        // SETINI.6 EXTBG: fold the DSL bool into the register byte's bit 6.
        row.setini = (row.setini & !0x40) | ((m7.get(ctx, "extbg").to_bool() as u8) << 6);
    }
    row
}

/// Write a `LineTableRow` back into the per-scanline register globals (used to
/// re-baseline globals before each hook and to restore sticky state after build).
fn write_state(ctx: piccolo::Context<'_>, row: &LineTableRow) {
    ctx.set_global("mode", row.mode as i64).unwrap();
    ctx.set_global("brightness", row.brightness as i64).unwrap();
    ctx.set_global("TM", row.tm as i64).unwrap();
    ctx.set_global("TS", row.ts as i64).unwrap();
    sync_screen(ctx, row.tm, row.ts);
    ctx.set_global("WH0", row.wh0 as i64).unwrap();
    ctx.set_global("WH1", row.wh1 as i64).unwrap();
    ctx.set_global("WH2", row.wh2 as i64).unwrap();
    ctx.set_global("WH3", row.wh3 as i64).unwrap();
    ctx.set_global("W12SEL", row.w12sel as i64).unwrap();
    ctx.set_global("W34SEL", row.w34sel as i64).unwrap();
    ctx.set_global("WOBJSEL", row.wobjsel as i64).unwrap();
    ctx.set_global("WBGLOG", row.wbglog as i64).unwrap();
    ctx.set_global("WOBJLOG", row.wobjlog as i64).unwrap();
    ctx.set_global("TMW", row.tmw as i64).unwrap();
    ctx.set_global("TSW", row.tsw as i64).unwrap();
    sync_win(ctx, &row_win_bytes(row));
    ctx.set_global("CGWSEL", row.cgwsel as i64).unwrap();
    ctx.set_global("CGADSUB", row.cgadsub as i64).unwrap();
    ctx.set_global("COLDATA", row.coldata as i64).unwrap();
    ctx.set_global("mosaic", row.mosaic_size as i64).unwrap();
    ctx.set_global("direct_color", (row.cgwsel & 0x01) != 0)
        .unwrap();
    sync_color(ctx, row.cgwsel, row.cgadsub, row.coldata);
    ctx.set_global("force_blank", row.force_blank).unwrap();
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
                layer
                    .set(ctx, "tile_size", row.bg[i].tile_size as i64)
                    .unwrap();
                layer
                    .set(ctx, "map_base", row.bg[i].map_base as i64)
                    .unwrap();
                layer
                    .set(ctx, "screen_size", row.bg[i].screen_size as i64)
                    .unwrap();
                layer
                    .set(ctx, "char_base", row.bg[i].char_base as i64)
                    .unwrap();
                layer.set(ctx, "mosaic", row.mosaic_enable[i]).unwrap();
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
        m7.set(ctx, "wrap", row.m7.repeat as i64).unwrap();
        m7.set(ctx, "flip_x", row.m7.flip_x).unwrap();
        m7.set(ctx, "flip_y", row.m7.flip_y).unwrap();
        m7.set(ctx, "extbg", row.setini & 0x40 != 0).unwrap();
    }
}

/// Run the `bg[n].source =` importer for every layer that binds an uploaded
/// asset, honoring the frame-wide `mode`. Writes real VRAM char + tilemap +
/// CGRAM (the bootstrap the poke flush then composes on top of) and echoes the
/// resulting binding registers back into the layer globals so `read_state`, the
/// rasterizer, and the inspector all agree. Cached (m4/importer): a hot key
/// re-copies words, never re-quantizes. Assumes VRAM/CGRAM were zeroed by the
/// caller; refreshes `reports` for the UI.
#[allow(clippy::too_many_arguments)]
fn apply_imports(
    ctx: piccolo::Context<'_>,
    assets: &HashMap<String, ImportAsset>,
    import_cache: &mut ImportCache,
    m7_cache: &mut HashMap<(String, u64), Mode7Import>,
    reports: &mut Vec<ImportBudget>,
    mem: &mut Memory,
) {
    reports.clear();
    let mode = crate::quantize::mode(ctx.get_global("mode").to_integer().unwrap_or(1) as u8);
    let Value::Table(bg) = ctx.get_global("bg") else {
        return;
    };
    for i in 0..4usize {
        let Value::Table(layer) = bg.get(ctx, (i + 1) as i64) else {
            continue;
        };
        let Some(slot) = value_to_string(layer.get(ctx, "source")) else {
            continue;
        };
        let Some(asset) = assets.get(&slot) else {
            continue;
        };
        if mode == 7 {
            // Mode 7 is a single 8bpp BG1 plane over the interleaved region.
            if i != 0 {
                continue;
            }
            let imp = m7_cache
                .entry((slot.clone(), asset.generation))
                .or_insert_with(|| {
                    import_mode7(&asset.rgba, asset.width as usize, asset.height as usize)
                });
            imp.apply(mem);
            reports.push(ImportBudget::Mode7 {
                layer: i,
                report: imp.report.clone(),
            });
        } else {
            // Tile BG (Mode 1): bit-depth from the mode table; only 2/4/8bpp
            // tile layers import.
            let bpp = crate::modes::mode_info(mode).map_or(0, |m| m.bpp[i]);
            if !matches!(bpp, 2 | 4 | 8) {
                continue;
            }
            // Placement bases: honor user-set map_base/char_base, else the
            // importer defaults (map_base 0, char_base 0x1000).
            let char_base = layer.get(ctx, "char_base").to_integer().unwrap_or(0) as u32;
            let opts = ImportOptions {
                bit_depth: bpp,
                tile_size: layer.get(ctx, "tile_size").to_integer().unwrap_or(8) as u8,
                map_base: crate::quantize::bg_map_base(
                    layer.get(ctx, "map_base").to_integer().unwrap_or(0) as u32,
                ),
                char_base: if char_base == 0 {
                    0x1000
                } else {
                    crate::quantize::bg_char_base(char_base)
                },
            };
            let key = ImportKey {
                asset: slot.clone(),
                generation: asset.generation,
                options: opts,
            };
            let imp = import_cache.get_or_import(key, &asset.rgba, asset.width, asset.height);
            let cb = imp.registers.char_base as usize;
            for (o, &w) in imp.char_words.iter().enumerate() {
                mem.vram[(cb + o) & 0x7fff] = w;
            }
            let mb = imp.registers.map_base as usize;
            for (o, &w) in imp.tilemap_words.iter().enumerate() {
                mem.vram[(mb + o) & 0x7fff] = w;
            }
            let mode0_band = if mode == 0 && bpp == 2 { i * 8 * 4 } else { 0 };
            for &(idx, c) in &imp.cgram {
                mem.cgram[mode0_band + idx as usize] = c;
            }
            layer
                .set(ctx, "map_base", imp.registers.map_base as i64)
                .unwrap();
            layer
                .set(ctx, "char_base", imp.registers.char_base as i64)
                .unwrap();
            layer
                .set(ctx, "screen_size", imp.registers.screen_size as i64)
                .unwrap();
            layer
                .set(ctx, "tile_size", imp.registers.tile_size as i64)
                .unwrap();
            reports.push(ImportBudget::Tile {
                layer: i,
                report: imp.report.clone(),
            });
        }
    }
}

/// Run the `obj.sheet =` OBJ-sheet importer if the frame binds an uploaded
/// sheet. The OBJ analog of `apply_imports`: reads `obj.sheet` (asset id) +
/// `obj.char_base` straight from the Lua ctx (NOT `memory.obsel`, which
/// `read_memory` only fills LATER), snaps char_base with `quantize::obj_char_base`
/// to match, memoizes `import_obj_sheet` keyed by asset+generation+char_base
/// (never re-quantizes at 60fps; re-quantizes on re-upload or a char-base move),
/// and lays OBJ char words at char_base + OBJ palettes (CGRAM 128..) into `mem`.
/// Runs in the same bootstrap slot as the BG imports, so the poke flush's
/// structured pokes and raw `vram[]` stay the final authority. Appends its
/// budget to the same `reports` vec (surfaced via `import_reports()`); assumes
/// `apply_imports` already `clear()`ed and pushed the BG reports first.
fn apply_obj_sheet(
    ctx: piccolo::Context<'_>,
    assets: &HashMap<String, ImportAsset>,
    cache: &mut HashMap<(String, u64, u16), ObjImport>,
    reports: &mut Vec<ImportBudget>,
    mem: &mut Memory,
) {
    let Value::Table(obj) = ctx.get_global("obj") else {
        return;
    };
    let Some(slot) = value_to_string(obj.get(ctx, "sheet")) else {
        return;
    };
    let Some(asset) = assets.get(&slot) else {
        return;
    };
    let char_base = crate::quantize::obj_char_base(
        obj.get(ctx, "char_base").to_integer().unwrap_or(0).max(0) as u32,
    );
    let imp = cache
        .entry((slot.clone(), asset.generation, char_base))
        .or_insert_with(|| import_obj_sheet(&asset.rgba, asset.width, asset.height));
    apply_obj_import(mem, imp, char_base);
    reports.push(ImportBudget::Obj {
        report: imp.report.clone(),
    });
}

/// VRAM word address of the tilemap entry for tile column `tx`, row `ty` at a
/// layer's snapped `map_base` and screen size. Mirrors `bg::map_entry_addr`
/// (private there) so a `bg[n].map` poke lands exactly where the rasterizer
/// reads it; the two must stay in lockstep.
fn tilemap_addr(map_base: u16, screen_size: u8, tx: u32, ty: u32) -> usize {
    let screen = match screen_size {
        1 => tx / 32,
        2 => ty / 32,
        3 => (ty / 32) * 2 + tx / 32,
        _ => 0,
    };
    ((map_base as u32 + screen * 0x400 + (ty % 32) * 32 + (tx % 32)) & 0x7fff) as usize
}

/// Flush the DSL memory-poke surfaces into `Memory`. Runs AFTER `apply_imports`
/// has laid down any `source =` bootstrap, so manual pokes compose on top under
/// last-write-wins. Order: structured tilemap/char pokes, then the raw `vram[]`
/// table as the FINAL authority (a raw word write always wins). `cgram[]` is
/// applied as an override on top of the import palette (only set entries).
/// VRAM is NOT zeroed here — `frame()` zeroes it before imports.
fn read_memory(ctx: piccolo::Context<'_>, mem: &mut Memory) {
    // cgram[] overrides the import palette: apply only the entries the user
    // actually set, so a `source =` import's colors survive where unpoked.
    // (mem.cgram was zeroed and any import palette written before this runs.)
    if let Value::Table(cg) = ctx.get_global("cgram") {
        for (k, v) in cg {
            if let (Some(i), Some(c)) = (k.to_integer(), v.to_integer()) {
                if (0..256).contains(&i) {
                    mem.cgram[i as usize] = (c as u16) & 0x7fff;
                }
            }
        }
    }
    if let Value::Table(obj) = ctx.get_global("obj") {
        mem.obj_sheet = value_to_string(obj.get(ctx, "sheet"));
        mem.obsel.char_base = crate::quantize::obj_char_base(
            obj.get(ctx, "char_base").to_integer().unwrap_or(0).max(0) as u32,
        );
        mem.obsel.size_sel =
            crate::quantize::obj_size_sel(obj.get(ctx, "size_sel").to_integer().unwrap_or(0) as u8);
        mem.obsel.name_select = crate::quantize::obj_name_select(
            obj.get(ctx, "name_select").to_integer().unwrap_or(0) as u8,
        );
        mem.priority_rotate = obj.get(ctx, "priority_rotate").to_bool();
        mem.oam_addr = (obj.get(ctx, "oam_addr").to_integer().unwrap_or(0).max(0) as u16) & 0x1ff;
        // Friendly sugar: obj.first = N turns rotation on and points OAMADD at
        // sprite N (word address N<<1). Overrides the raw fields when present.
        if let Some(n) = obj.get(ctx, "first").to_integer() {
            mem.priority_rotate = true;
            mem.oam_addr = ((n.max(0) as u16 & 0x7f) << 1) & 0x1ff;
        }
        for i in 0..128 {
            if let Value::Table(o) = obj.get(ctx, i as i64) {
                let e = &mut mem.oam[i];
                e.x = crate::quantize::sprite_x(o.get(ctx, "x").to_number().unwrap_or(0.0) as f32);
                e.y = crate::quantize::sprite_y(o.get(ctx, "y").to_number().unwrap_or(0.0) as f32);
                e.tile = o.get(ctx, "tile").to_integer().unwrap_or(0) as u16;
                e.pal = o.get(ctx, "pal").to_integer().unwrap_or(0) as u8;
                e.prio = o.get(ctx, "prio").to_integer().unwrap_or(0) as u8;
                e.large = o.get(ctx, "large").to_bool();
                e.flip_x = o.get(ctx, "flip_x").to_bool();
                e.flip_y = o.get(ctx, "flip_y").to_bool();
                e.on = o.get(ctx, "on").to_bool();
            }
        }
    }

    // Structured tilemap pokes: bg[n].map[col][row] = {tile,pal,prio,flip_x,flip_y}
    // packs the real 16-bit entry word into VRAM at the layer's map_base (snapped
    // and screen-size-wrapped exactly as the rasterizer reads it).
    if let Value::Table(bg) = ctx.get_global("bg") {
        for i in 0..4 {
            let Value::Table(layer) = bg.get(ctx, (i + 1) as i64) else {
                continue;
            };
            let map_base = crate::quantize::bg_map_base(
                layer.get(ctx, "map_base").to_integer().unwrap_or(0) as u32,
            );
            let screen_size = crate::quantize::bg_screen_size(
                layer.get(ctx, "screen_size").to_integer().unwrap_or(0) as u8,
            );
            let Value::Table(map) = layer.get(ctx, "map") else {
                continue;
            };
            for (ck, cv) in map {
                let (Some(col), Value::Table(rowt)) = (ck.to_integer(), cv) else {
                    continue;
                };
                for (rk, rv) in rowt {
                    let (Some(row_i), Value::Table(cell)) = (rk.to_integer(), rv) else {
                        continue;
                    };
                    let tile = cell.get(ctx, "tile").to_integer().unwrap_or(0) as u16 & 0x03ff;
                    let pal = cell.get(ctx, "pal").to_integer().unwrap_or(0) as u16 & 0x07;
                    let prio = cell.get(ctx, "prio").to_integer().unwrap_or(0) as u16 & 0x01;
                    let hf = cell.get(ctx, "flip_x").to_bool() as u16;
                    let vf = cell.get(ctx, "flip_y").to_bool() as u16;
                    let word = tile | (pal << 10) | (prio << 13) | (hf << 14) | (vf << 15);
                    let addr = tilemap_addr(map_base, screen_size, col as u32, row_i as u32);
                    mem.vram[addr] = word;
                }
            }
        }
    }

    // Mode 7 structured pokes. Both mask a single byte lane of the interleaved
    // word: map = low byte (tile#), char pixels = high byte (8bpp index).
    if let Value::Table(m7) = ctx.get_global("m7") {
        if let Value::Table(map) = m7.get(ctx, "map") {
            for (yk, yv) in map {
                let (Some(ty), Value::Table(rowt)) = (yk.to_integer(), yv) else {
                    continue;
                };
                for (xk, xv) in rowt {
                    if let (Some(tx), Some(tile)) = (xk.to_integer(), xv.to_integer()) {
                        let i = (ty as usize) * 128 + tx as usize;
                        if i < 0x8000 {
                            mem.vram[i] = (mem.vram[i] & 0xff00) | (tile as u16 & 0x00ff);
                        }
                    }
                }
            }
        }
    }
    if let Value::Table(cb) = ctx.get_global("__m7char") {
        for (tk, tv) in cb {
            let (Some(tile), Value::Table(pix)) = (tk.to_integer(), tv) else {
                continue;
            };
            for (pk, pv) in pix {
                if let (Some(off), Some(idx)) = (pk.to_integer(), pv.to_integer()) {
                    let i = (tile as usize) * 64 + off as usize;
                    if i < 0x8000 {
                        mem.vram[i] = (mem.vram[i] & 0x00ff) | ((idx as u16 & 0xff) << 8);
                    }
                }
            }
        }
    }

    // Raw `vram[addr] = word` pokes — the FINAL authority (applied after imports
    // and structured pokes, so a raw word write always wins). Iterate only the
    // set entries (sparse) rather than scanning 0..0x8000.
    if let Value::Table(vt) = ctx.get_global("vram") {
        for (k, v) in vt {
            if let (Some(addr), Some(word)) = (k.to_integer(), v.to_integer()) {
                if (0..0x8000).contains(&addr) {
                    mem.vram[addr as usize] = word as u16;
                }
            }
        }
    }
}
