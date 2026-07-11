//! WASM/JS shim over the pure core. Compiled only for wasm32. Each method maps
//! 1:1 to the TS `PpuCore` interface; the JS wrapper assembles `frame()`'s object.
//! Owns a `LuaEngine`; frame() drives Lua -> LineTable -> compositor and caches
//! the framebuffer/registers/cgram/oam for the per-field getters.
use std::collections::HashMap;

use js_sys::{Object, Reflect, Uint8Array, Uint8ClampedArray};
use wasm_bindgen::prelude::*;

use crate::{
    derive_registers, render_frame_view, render_layer_view, trace_bg_screen, trace_bg_tile,
    trace_obj, AssetInfo, LineTable, LuaEngine, LuaErrorView, OamSprite, ObjOverflow, Register,
    SetSourceResult, HEIGHT, WIDTH,
};

#[wasm_bindgen]
pub struct PpuCore {
    engine: LuaEngine,
    framebuffer: Vec<u8>,
    registers: Vec<Register>,
    cgram: Vec<u16>,
    oam: Vec<OamSprite>,
    obj_overflow: ObjOverflow,
    prev_reg: HashMap<u16, i32>,
    /// Layer-visibility overrides: bg[0..3] = bg1..bg4, plus obj. `None` = leave
    /// whatever the Lua program set; `Some(v)` forces the layer on/off.
    bg_visible: [Option<bool>; 4],
    obj_visible: Option<bool>,
    main_screen: Vec<u8>,
    sub_screen: Vec<u8>,
    math_mask: Vec<u8>,
    /// Resolved LineTable of the most recent frame — the register context for
    /// trace queries and layer views.
    last_lt: Option<LineTable>,
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
            obj_overflow: ObjOverflow::default(),
            prev_reg: HashMap::new(),
            bg_visible: [None; 4],
            obj_visible: None,
            main_screen: vec![0; WIDTH * HEIGHT * 4],
            sub_screen: vec![0; WIDTH * HEIGHT * 4],
            math_mask: vec![0; WIDTH * HEIGHT],
            last_lt: None,
        }
    }

    #[wasm_bindgen(js_name = setSource)]
    pub fn set_source(&mut self, src: &str) -> Result<JsValue, JsValue> {
        to_set_source_result(self.engine.set_source(src))
    }

    /// Multi-file sketch: chunks execute in list order into one shared global
    /// scope; errors carry `{file, line?, message}`.
    #[wasm_bindgen(js_name = setSources)]
    pub fn set_sources(&mut self, files: JsValue) -> Result<JsValue, JsValue> {
        let files: Vec<SourceFileIn> = serde_wasm_bindgen::from_value(files)?;
        let pairs: Vec<(&str, &str)> = files
            .iter()
            .map(|f| (f.name.as_str(), f.source.as_str()))
            .collect();
        to_set_source_result(self.engine.set_sources(&pairs))
    }

    pub fn frame(&mut self, t: f64, f: u32) -> Result<(), JsValue> {
        // Build the LineTable + update Memory. On a Lua runtime error, throw the
        // structured LuaError (same shape as setSource) so the JS adapter's
        // safeFrame surfaces it as an editor diagnostic. The cached framebuffer/
        // registers/cgram/oam are left intact -> last good frame survives.
        let mut lt = match self.engine.frame(t, f) {
            Ok(lt) => lt,
            Err(e) => {
                let view: LuaErrorView = e.into();
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
        let view = render_frame_view(&lt, mem);
        self.framebuffer = view.framebuffer;
        self.obj_overflow = view.overflow;
        self.main_screen = view.main;
        self.sub_screen = view.sub;
        self.math_mask = view.math_mask;
        self.cgram = mem.cgram.to_vec();
        self.oam = mem.oam.iter().map(OamSprite::from).collect();

        // Registers + changed flags vs the previous frame, snapshotted from the
        // resolved top scanline (row 0).
        self.registers = derive_registers(&lt.rows[0], &mem.obsel, &self.prev_reg);
        self.prev_reg = self.registers.iter().map(|r| (r.addr, r.value)).collect();
        self.last_lt = Some(lt);
        Ok(())
    }

    pub fn framebuffer(&self) -> Vec<u8> {
        self.framebuffer.clone()
    }

    pub fn cgram(&self) -> Vec<u16> {
        self.cgram.clone()
    }

    pub fn vram(&self) -> Vec<u16> {
        self.engine.memory().vram.to_vec()
    }

    pub fn registers(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.registers).map_err(Into::into)
    }

    pub fn oam(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.oam).map_err(Into::into)
    }

    #[wasm_bindgen(js_name = objOverflow)]
    pub fn obj_overflow(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.obj_overflow).map_err(Into::into)
    }

    #[wasm_bindgen(js_name = listAssets)]
    pub fn list_assets(&self) -> Result<JsValue, JsValue> {
        let assets: Vec<AssetInfo> = self
            .engine
            .assets()
            .into_iter()
            .map(|(id, width, height)| AssetInfo { id, width, height })
            .collect(); // already sorted by id (stable order for the inspector)
        serde_wasm_bindgen::to_value(&assets).map_err(Into::into)
    }

    #[wasm_bindgen(js_name = uploadTexture)]
    pub fn upload_texture(&mut self, slot: String, image_data: JsValue) {
        let get = |k: &str| Reflect::get(&image_data, &JsValue::from_str(k)).ok();
        let width = get("width").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
        let height = get("height").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
        let Some(data) = get("data") else { return };
        let rgba = Uint8ClampedArray::new(&data).to_vec();
        // The engine owns the asset store + import cache and validates malformed
        // ImageData (zero dims / wrong buffer length) internally.
        self.engine.upload_asset(slot, width, height, rgba);
    }

    /// Per-layer import budget reports from the most recent `frame()`
    /// (m4/importer -> m4/inspector).
    #[wasm_bindgen(js_name = importReports)]
    pub fn import_reports(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(self.engine.import_reports()).map_err(Into::into)
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

    #[wasm_bindgen(js_name = mainScreen)]
    pub fn main_screen(&self) -> Vec<u8> {
        self.main_screen.clone()
    }

    #[wasm_bindgen(js_name = subScreen)]
    pub fn sub_screen(&self) -> Vec<u8> {
        self.sub_screen.clone()
    }

    #[wasm_bindgen(js_name = mathMask)]
    pub fn math_mask(&self) -> Vec<u8> {
        self.math_mask.clone()
    }

    /// Single-plane isolation render ("bg1".."bg4", "obj" — the setLayerVisible
    /// ids). Transparent buffer before the first frame / for unknown ids.
    #[wasm_bindgen(js_name = layerView)]
    pub fn layer_view(&self, plane: &str) -> Vec<u8> {
        let plane = match plane {
            "bg1" => 0u8,
            "bg2" => 1,
            "bg3" => 2,
            "bg4" => 3,
            "obj" => 4,
            _ => return vec![0; WIDTH * HEIGHT * 4],
        };
        match &self.last_lt {
            Some(lt) => render_layer_view(lt, self.engine.memory(), plane),
            None => vec![0; WIDTH * HEIGHT * 4],
        }
    }

    /// Trace a BG plane (1..=4) at screen pixel (x, y). null when out of range,
    /// before the first frame, or when the layer is absent in that row's mode.
    #[wasm_bindgen(js_name = traceBgPixel)]
    pub fn trace_bg_pixel(&self, layer: u8, x: u32, y: u32) -> Result<JsValue, JsValue> {
        let Some(lt) = &self.last_lt else {
            return Ok(JsValue::NULL);
        };
        if !(1..=4).contains(&layer) || x as usize >= WIDTH || y as usize >= HEIGHT {
            return Ok(JsValue::NULL);
        }
        let row = &lt.rows[y as usize];
        match trace_bg_screen(
            row,
            self.engine.memory(),
            layer as usize - 1,
            x as usize,
            y as usize,
        ) {
            Some(t) => serde_wasm_bindgen::to_value(&t).map_err(Into::into),
            None => Ok(JsValue::NULL),
        }
    }

    /// Trace a BG plane at tilemap cell (tx, ty); `y` picks the register row.
    #[wasm_bindgen(js_name = traceBgTile)]
    pub fn trace_bg_tile_at(
        &self,
        layer: u8,
        tx: u32,
        ty: u32,
        y: u32,
    ) -> Result<JsValue, JsValue> {
        let Some(lt) = &self.last_lt else {
            return Ok(JsValue::NULL);
        };
        if !(1..=4).contains(&layer) || y as usize >= HEIGHT {
            return Ok(JsValue::NULL);
        }
        let row = &lt.rows[y as usize];
        match trace_bg_tile(row, self.engine.memory(), layer as usize - 1, tx, ty) {
            Some(t) => serde_wasm_bindgen::to_value(&t).map_err(Into::into),
            None => Ok(JsValue::NULL),
        }
    }

    /// Trace OAM sprite `index` (0..=127). null when out of range.
    #[wasm_bindgen(js_name = traceObj)]
    pub fn trace_obj_at(&self, index: u32) -> Result<JsValue, JsValue> {
        match trace_obj(self.engine.memory(), index as usize) {
            Some(t) => serde_wasm_bindgen::to_value(&t).map_err(Into::into),
            None => Ok(JsValue::NULL),
        }
    }

    /// Pure quantize+pack: image -> versioned source payload + meta. No engine
    /// state mutation. Returns `{ payload: Uint8Array, meta: SourceMeta }`.
    #[wasm_bindgen(js_name = convertSource)]
    pub fn convert_source(
        &self,
        kind: &str,
        options: JsValue,
        image_data: JsValue,
    ) -> Result<JsValue, JsValue> {
        let kind = match kind {
            "bg" => crate::SourceKind::Bg,
            "m7" => crate::SourceKind::M7,
            "obj" => crate::SourceKind::Obj,
            other => return Err(JsValue::from_str(&format!("unknown source kind '{other}'"))),
        };
        let opts: crate::ConvertOptions = if options.is_undefined() || options.is_null() {
            Default::default()
        } else {
            serde_wasm_bindgen::from_value(options)?
        };
        let get = |k: &str| Reflect::get(&image_data, &JsValue::from_str(k)).ok();
        let width = get("width").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
        let height = get("height").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
        let rgba = get("data")
            .map(|d| Uint8ClampedArray::new(&d).to_vec())
            .unwrap_or_default();
        let (payload, meta) = crate::convert_source(kind, &opts, &rgba, width, height)
            .map_err(|e| JsValue::from_str(&e))?;
        let out = Object::new();
        Reflect::set(
            &out,
            &"payload".into(),
            &Uint8Array::from(payload.encode().as_slice()).into(),
        )?;
        Reflect::set(&out, &"meta".into(), &serde_wasm_bindgen::to_value(&meta)?)?;
        Ok(out.into())
    }

    /// Decode + register a payload in the source store. Never throws for a bad
    /// payload — returns `{ ok: false, error }` (the structured-diagnostic channel).
    #[wasm_bindgen(js_name = addSource)]
    pub fn add_source(&mut self, name: &str, payload: &[u8]) -> Result<JsValue, JsValue> {
        #[derive(serde::Serialize)]
        struct AddSourceResult {
            ok: bool,
            #[serde(skip_serializing_if = "Option::is_none")]
            error: Option<String>,
        }
        let res = match self.engine.add_source(name, payload) {
            Ok(()) => AddSourceResult {
                ok: true,
                error: None,
            },
            Err(e) => AddSourceResult {
                ok: false,
                error: Some(e.to_string()),
            },
        };
        serde_wasm_bindgen::to_value(&res).map_err(Into::into)
    }
}

impl Default for PpuCore {
    fn default() -> Self {
        Self::new()
    }
}

/// One incoming source file for `setSources`, matching the TS `SourceFile`.
#[derive(serde::Deserialize)]
struct SourceFileIn {
    name: String,
    source: String,
}

fn to_set_source_result(res: Result<(), crate::LuaError>) -> Result<JsValue, JsValue> {
    let view = match res {
        Ok(()) => SetSourceResult {
            ok: true,
            error: None,
        },
        Err(e) => SetSourceResult {
            ok: false,
            error: Some(e.into()),
        },
    };
    serde_wasm_bindgen::to_value(&view).map_err(Into::into)
}
