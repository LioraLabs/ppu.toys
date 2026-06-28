//! WASM/JS shim over the pure core. Compiled only for wasm32. Each method maps
//! 1:1 to the TS `PpuCore` interface; the JS wrapper assembles `frame()`'s object.
//! Owns a `LuaEngine`; frame() drives Lua -> LineTable -> compositor and caches
//! the framebuffer/registers/cgram/oam for the per-field getters.
use std::collections::HashMap;

use js_sys::{Reflect, Uint8ClampedArray};
use wasm_bindgen::prelude::*;

use crate::{
    derive_registers, render_frame, AssetInfo, LuaEngine, LuaErrorView, OamSprite, Register,
    SetSourceResult, Source, HEIGHT, WIDTH,
};

#[wasm_bindgen]
pub struct PpuCore {
    engine: LuaEngine,
    framebuffer: Vec<u8>,
    registers: Vec<Register>,
    cgram: Vec<u16>,
    oam: Vec<OamSprite>,
    prev_reg: HashMap<u16, u8>,
    /// Layer-visibility overrides: bg[0..3] = bg1..bg4, plus obj. `None` = leave
    /// whatever the Lua program set; `Some(v)` forces the layer on/off.
    bg_visible: [Option<bool>; 4],
    obj_visible: Option<bool>,
}

#[wasm_bindgen]
impl PpuCore {
    #[wasm_bindgen(constructor)]
    pub fn new() -> PpuCore {
        PpuCore {
            engine: LuaEngine::new(),
            framebuffer: vec![0; WIDTH * HEIGHT * 4],
            registers: Vec::new(),
            cgram: vec![0; 256],
            oam: Vec::new(),
            prev_reg: HashMap::new(),
            bg_visible: [None; 4],
            obj_visible: None,
        }
    }

    #[wasm_bindgen(js_name = setSource)]
    pub fn set_source(&mut self, src: &str) -> Result<JsValue, JsValue> {
        // Preserve uploaded sources across recompiles (LuaEngine resets Memory).
        let saved = std::mem::take(&mut self.engine.memory_mut().sources);
        let res = match self.engine.set_source(src) {
            Ok(()) => SetSourceResult { ok: true, error: None },
            Err(e) => SetSourceResult {
                ok: false,
                error: Some(LuaErrorView { message: e.message, line: e.line }),
            },
        };
        self.engine.memory_mut().sources = saved;
        serde_wasm_bindgen::to_value(&res).map_err(Into::into)
    }

    pub fn frame(&mut self, t: f64, f: u32) -> Result<(), JsValue> {
        // Build the LineTable + update Memory. On a Lua runtime error, throw the
        // structured LuaError (same shape as setSource) so the JS adapter's
        // safeFrame surfaces it as an editor diagnostic. The cached framebuffer/
        // registers/cgram/oam are left intact -> last good frame survives.
        let mut lt = match self.engine.frame(t, f) {
            Ok(lt) => lt,
            Err(e) => {
                let view = LuaErrorView { message: e.message, line: e.line };
                return Err(serde_wasm_bindgen::to_value(&view)?);
            }
        };

        // Apply layer-visibility overrides: bg per-row, obj by clearing on-flags
        // (Memory is repopulated from Lua next frame, so this mutation is transient).
        for row in lt.rows.iter_mut() {
            for i in 0..4 {
                if let Some(v) = self.bg_visible[i] {
                    row.bg[i].visible = v;
                }
            }
        }
        if self.obj_visible == Some(false) {
            for o in self.engine.memory_mut().oam.iter_mut() {
                o.on = false;
            }
        }

        let mem = self.engine.memory();
        self.framebuffer = render_frame(&lt, mem);
        self.cgram = mem.cgram.to_vec();
        self.oam = mem.oam.iter().map(OamSprite::from).collect();

        // Registers + changed flags vs the previous frame, snapshotted from the
        // resolved top scanline (row 0).
        self.registers = derive_registers(&lt.rows[0], &self.prev_reg);
        self.prev_reg = self.registers.iter().map(|r| (r.addr, r.value)).collect();
        Ok(())
    }

    pub fn framebuffer(&self) -> Vec<u8> {
        self.framebuffer.clone()
    }

    pub fn cgram(&self) -> Vec<u16> {
        self.cgram.clone()
    }

    pub fn registers(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.registers).map_err(Into::into)
    }

    pub fn oam(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.oam).map_err(Into::into)
    }

    #[wasm_bindgen(js_name = listAssets)]
    pub fn list_assets(&self) -> Result<JsValue, JsValue> {
        let mut assets: Vec<AssetInfo> = self
            .engine
            .memory()
            .sources
            .iter()
            .map(|(id, s)| AssetInfo { id: id.clone(), width: s.width, height: s.height })
            .collect();
        assets.sort_by(|a, b| a.id.cmp(&b.id)); // stable order for the inspector
        serde_wasm_bindgen::to_value(&assets).map_err(Into::into)
    }

    #[wasm_bindgen(js_name = uploadTexture)]
    pub fn upload_texture(&mut self, slot: String, image_data: JsValue) {
        let get = |k: &str| Reflect::get(&image_data, &JsValue::from_str(k)).ok();
        let width = get("width").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
        let height = get("height").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
        let Some(data) = get("data") else { return };
        let rgba = Uint8ClampedArray::new(&data).to_vec();
        if width == 0 || height == 0 || rgba.len() != (width * height * 4) as usize {
            return; // malformed ImageData -> ignore
        }
        self.engine
            .memory_mut()
            .sources
            .insert(slot, Source { width, height, rgba });
    }

    #[wasm_bindgen(js_name = setLayerVisible)]
    pub fn set_layer_visible(&mut self, id: String, visible: bool) {
        match id.as_str() {
            "bg1" => self.bg_visible[0] = Some(visible),
            "bg2" => self.bg_visible[1] = Some(visible),
            "bg3" => self.bg_visible[2] = Some(visible),
            "bg4" => self.bg_visible[3] = Some(visible),
            "obj" => self.obj_visible = Some(visible),
            _ => {}
        }
    }
}

impl Default for PpuCore {
    fn default() -> Self {
        Self::new()
    }
}
