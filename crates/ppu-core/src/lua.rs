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

fn install_bindings(_ctx: piccolo::Context<'_>) {
    // filled in Task 2
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
