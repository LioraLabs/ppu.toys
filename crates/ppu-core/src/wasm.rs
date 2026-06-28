//! WASM/JS shim over the pure core. Compiled only for wasm32. Each method maps
//! 1:1 to the TS `PpuCore` interface; the JS wrapper assembles `frame()`'s object.
use crate::{
    placeholder_cgram, placeholder_framebuffer, placeholder_registers, Register,
    SetSourceResult, HEIGHT, WIDTH,
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct PpuCore {
    framebuffer: Vec<u8>,
    registers: Vec<Register>,
    cgram: Vec<u16>,
}

#[wasm_bindgen]
impl PpuCore {
    #[wasm_bindgen(constructor)]
    pub fn new() -> PpuCore {
        PpuCore {
            framebuffer: vec![0; WIDTH * HEIGHT * 4],
            registers: Vec::new(),
            cgram: vec![0; 256],
        }
    }

    #[wasm_bindgen(js_name = setSource)]
    pub fn set_source(&mut self, _src: &str) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&SetSourceResult { ok: true }).map_err(Into::into)
    }

    pub fn frame(&mut self, t: f64, f: u32) {
        self.framebuffer = placeholder_framebuffer(t, f);
        self.registers = placeholder_registers();
        self.cgram = placeholder_cgram();
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

    #[wasm_bindgen(js_name = uploadTexture)]
    pub fn upload_texture(&mut self, _slot: String, _image_data: JsValue) {}

    #[wasm_bindgen(js_name = setLayerVisible)]
    pub fn set_layer_visible(&mut self, _id: String, _visible: bool) {}
}
