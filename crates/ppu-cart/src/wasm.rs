//! `CartSnes` for JS, in the same module as ppu-core's Studio bindings (feature `web`).
use wasm_bindgen::prelude::*;

// Links ppu-core's #[wasm_bindgen] exports into this module.
extern crate ppu_core;

#[wasm_bindgen]
pub struct CartSnes(crate::Snes);

#[wasm_bindgen]
impl CartSnes {
    /// Boot a headerless `.sfc`; throws the console's reason when it cannot.
    #[wasm_bindgen(constructor)]
    pub fn new(rom: &[u8]) -> Result<CartSnes, JsError> {
        crate::Snes::new(rom)
            .map(CartSnes)
            .map_err(|e| JsError::new(&e))
    }
    pub fn run_frame(&mut self) {
        self.0.run_frame()
    }
    /// `port` 0 or 1; `buttons` in `ppu_cart::pad` bits.
    pub fn set_pad(&mut self, port: usize, buttons: u16) {
        self.0.set_pad(port, buttons)
    }
    /// The last frame, RGBA8, 256 wide (224 or 239 rows).
    pub fn framebuffer(&self) -> Vec<u8> {
        self.0.framebuffer().to_vec()
    }
    /// All 128 KB of work RAM ($7E:0000-$7F:FFFF).
    pub fn wram(&self) -> Vec<u8> {
        self.0.wram().to_vec()
    }
    /// The battery-backed RAM (empty without one), as an `.srm` file holds it.
    pub fn sram(&self) -> Vec<u8> {
        self.0.sram().to_vec()
    }
    /// Load an `.srm` image into the battery RAM before the first frame; throws when its size is not the header's.
    pub fn set_sram(&mut self, image: &[u8]) -> Result<(), JsError> {
        self.0.load_sram(image).map_err(|e| JsError::new(&e))
    }
    /// Count master clocks per PC for `take_pc_ticks` (off at power-on: it costs a hash per instruction).
    pub fn set_profile(&mut self, on: bool) {
        self.0.profile = on;
    }
    /// Master clocks per 24-bit PC since the last call, flattened [pc, clocks, pc, clocks, ...].
    pub fn take_pc_ticks(&mut self) -> Vec<f64> {
        self.0
            .take_pc_ticks()
            .into_iter()
            .flat_map(|(pc, n)| [pc as f64, n as f64])
            .collect()
    }
}
