//! WASM/JS shim over the pure core. Compiled only for wasm32. Each method maps
//! 1:1 to the TS `PpuCore` interface; the JS wrapper assembles `frame()`'s object.
//! Owns a `LuaEngine`; frame() drives Lua -> LineTable -> compositor and caches
//! the framebuffer/registers/cgram/oam for the per-field getters. advance() is
//! frame() minus the compositor (see its doc for what stays stale).
use std::collections::HashMap;

use js_sys::{Object, Reflect, Uint8Array, Uint8ClampedArray};
use wasm_bindgen::prelude::*;

use crate::{
    derive_registers, render_frame_view, render_layer_view, trace_bg_screen, trace_bg_tile,
    trace_obj, LineTable, LuaEngine, LuaErrorView, OamSprite, ObjOverflow, Register,
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

    /// The Studio's live pokes chunk the engine keeps across loads: top-level `dma()` lines,
    /// `function __ppu_pokes_vram()`, `function __ppu_pokes()` (see `LuaEngine::set_pokes`); ""
    /// drops them. Same `{ok, error?}` shape as `setSources`; errors carry `file: "pokes"`.
    #[wasm_bindgen(js_name = setPokes)]
    pub fn set_pokes(&mut self, chunk: &str) -> Result<JsValue, JsValue> {
        to_set_source_result(self.engine.set_pokes(chunk))
    }

    /// Power-cycle the sound chip (Run from t=0): fresh DSP, zero ARAM,
    /// recompile so placements re-write and timers restart at phase zero.
    /// Same `{ok, error?}` result shape as `setSources`.
    pub fn reset(&mut self) -> Result<JsValue, JsValue> {
        to_set_source_result(self.engine.reset())
    }

    /// The program's scene files (`scenes.add` order; one implied scene, the main file, without
    /// scenes). Changes only with a load: read it after `setSources`/`reset`.
    pub fn scenes(&self) -> Vec<String> {
        self.engine.scenes()
    }

    /// The running scene, an index into `scenes()`; it follows `scenes.go`.
    #[wasm_bindgen(js_name = currentScene)]
    pub fn current_scene(&self) -> u32 {
        self.engine.current_scene() as u32
    }

    /// The scene file the next load (`reset`, `setSources`) boots straight into; none = the
    /// program's own boot. With `preview` it is a Scene view entry that runs the scene's `preview`
    /// before its `init`.
    #[wasm_bindgen(js_name = setStartScene)]
    pub fn set_start_scene(&mut self, file: Option<String>, preview: bool) {
        self.engine.set_start_scene(file, preview);
    }

    /// The project's update target (`targetHz`: 25, 30, 50 or 60; none: a `---@snes frames N` line
    /// decides, else every video frame). At 25 or 30 `frame()` runs Lua only on every second video frame.
    #[wasm_bindgen(js_name = setTargetHz)]
    pub fn set_target_hz(&mut self, hz: Option<u8>) -> Result<(), JsValue> {
        self.engine
            .set_target_hz(hz)
            .map_err(|e| JsValue::from_str(&e))
    }

    /// Video frames per update (30 Hz: 2): a held `scenes.go` is entered this many video frames
    /// after the update that asked, as the cartridge enters it.
    /// The run frame (from power-on) the host's `f = 0` stands for: after `enterPendingScene` the
    /// host counts the scene from 0, and t still rounds as the cart's run does.
    #[wasm_bindgen(js_name = setRunBase)]
    pub fn set_run_base(&mut self, f: u32) {
        self.engine.set_run_base(f);
    }

    #[wasm_bindgen(js_name = updateFrames)]
    pub fn update_frames(&self) -> u32 {
        self.engine.update_frames()
    }

    /// The scene a `scenes.go` asked for, not yet entered (an index into `scenes()`); none while
    /// no switch waits.
    #[wasm_bindgen(js_name = pendingScene)]
    pub fn pending_scene(&self) -> Option<u32> {
        self.engine.pending_scene().map(|k| k as u32)
    }

    /// Enter the pending scene now with its clock from t = 0 (the next frame is `frame(0, 0)`),
    /// no preview. `{ok, error?}` as `setSources`; ok without a pending switch changes nothing.
    #[wasm_bindgen(js_name = enterPendingScene)]
    pub fn enter_pending_scene(&mut self) -> Result<JsValue, JsValue> {
        to_set_source_result(self.engine.enter_pending_scene().map(|_| ()))
    }

    /// The host paused (true) or plays (false): paused, `frame()` again for the same `f` starts from
    /// the state that frame first started from, so a poke redraws without advancing the program.
    #[wasm_bindgen(js_name = setHold)]
    pub fn set_hold(&mut self, on: bool) {
        self.engine.set_hold(on);
    }

    /// Run one frame in full: Lua `frame()`, scanout, DSP/audio, sram, and the
    /// compositor, refreshing every cached snapshot the getters read.
    pub fn frame(&mut self, t: f64, f: u32) -> Result<(), JsValue> {
        self.run(t, f, true)
    }

    /// Simulate one frame without compositing: the same Lua `frame()`, scanout,
    /// DSP/audio, sram and register bookkeeping as [`Self::frame`], but the
    /// compositor does not run. `audio()` is that frame's PCM.
    ///
    /// Skipped: the framebuffer, `objOverflow`, `mainScreen`, `subScreen` and
    /// `mathMask` keep the last composed picture. Updated as in `frame()`:
    /// registers, cgram, oam and the line tables. The line-table getters
    /// (`inspectScanline`, `layerView`, `traceBgPixel`/`traceBgTile`, the
    /// m7/window scanline getters) and the live-memory getters (`traceObj`,
    /// `vram`, `m7MapView`, `dspState`, `scoreState`) both reflect the advanced
    /// frame while the picture getters describe an earlier one, so call
    /// `frame()` before reading inspection getters.
    pub fn advance(&mut self, t: f64, f: u32) -> Result<(), JsValue> {
        self.run(t, f, false)
    }

    fn run(&mut self, t: f64, f: u32, compose: bool) -> Result<(), JsValue> {
        // Build the LineTable + update Memory. On a Lua runtime error (from
        // frame() or advance() alike), throw the structured LuaError (same shape
        // as setSource) so the JS adapter surfaces it as an editor diagnostic.
        // Every cached snapshot is left intact -> last good frame survives.
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
        if compose {
            // obj hidden for this picture only: OAM persists, and a held picture (30 Hz) reruns nothing
            let hidden = self.obj_visible == Some(false);
            let on: Vec<bool> = if hidden {
                self.engine
                    .memory_mut()
                    .oam
                    .iter_mut()
                    .map(|o| std::mem::replace(&mut o.on, false))
                    .collect()
            } else {
                vec![]
            };
            let view = render_frame_view(&lt, self.engine.memory());
            for (o, on) in self.engine.memory_mut().oam.iter_mut().zip(on) {
                o.on = on;
            }
            self.framebuffer = view.framebuffer;
            self.obj_overflow = view.overflow;
            self.main_screen = view.main;
            self.sub_screen = view.sub;
            self.math_mask = view.math_mask;
        }
        let mem = self.engine.memory();
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

    /// Inspect the selected line without advancing or re-rendering the frame.
    #[wasm_bindgen(js_name = inspectScanline)]
    pub fn inspect_scanline(&self, y: u32) -> Result<JsValue, JsValue> {
        let snapshot = self
            .last_lt
            .as_ref()
            .and_then(|lt| crate::inspect_scanline(lt, self.engine.memory(), y as usize));
        serde_wasm_bindgen::to_value(&snapshot).map_err(Into::into)
    }

    pub fn oam(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.oam).map_err(Into::into)
    }

    #[wasm_bindgen(js_name = objOverflow)]
    pub fn obj_overflow(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.obj_overflow).map_err(Into::into)
    }

    /// Every broken contract store since the last `reset`, repeats counted: it moves whenever
    /// `violations()` would differ, and is cheap enough to ask every frame.
    #[wasm_bindgen(js_name = violationCount)]
    pub fn violation_count(&self) -> u32 {
        self.engine.violations().iter().map(|v| v.count).sum()
    }

    /// Broken contract stores since the last `reset`, first seen first, one per file:line and store:
    /// `{store, value, min, max, file, line, scene, frame?, count}` (`frame` absent: during a load).
    pub fn violations(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(self.engine.violations()).map_err(Into::into)
    }

    /// What the last load did not carry over from the running program, one human line per
    /// variable that kept its new initial value. Empty after a power-on.
    #[wasm_bindgen(js_name = notCarried)]
    pub fn not_carried(&self) -> Vec<String> {
        self.engine.not_carried().to_vec()
    }

    /// Another toy opens: the next load powers on instead of carrying the running program's state.
    #[wasm_bindgen(js_name = forgetRun)]
    pub fn forget_run(&mut self) {
        self.engine.forget_run();
    }

    /// Per-layer import budget reports from the most recent `frame()`
    /// (m4/importer -> m4/inspector).
    #[wasm_bindgen(js_name = importReports)]
    pub fn import_reports(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(self.engine.import_reports()).map_err(Into::into)
    }

    /// The last recompile's `dma()` placements and the VRAM/CGRAM spans they
    /// claim (DMA panel memory map).
    #[wasm_bindgen(js_name = memoryMap)]
    pub fn memory_map(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.engine.memory_map()).map_err(Into::into)
    }

    #[wasm_bindgen(js_name = setAudioMix)]
    pub fn set_audio_mix(&mut self, json: Option<String>) -> Result<(), JsValue> {
        self.engine
            .set_audio_mix(json.as_deref())
            .map_err(|e| JsValue::from_str(&e))
    }

    #[wasm_bindgen(js_name = audioMix)]
    pub fn audio_mix(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(self.engine.audio_mix()).map_err(Into::into)
    }

    #[wasm_bindgen(js_name = triggerVoice)]
    pub fn trigger_voice(&mut self, voice: u8, release: bool) -> Result<(), JsValue> {
        self.engine
            .trigger_voice(voice, release)
            .map_err(|e| JsValue::from_str(&e))
    }

    /// The last frame's interleaved stereo audio (L,R per sample, 32000 Hz).
    /// Empty before the first `frame()` (M12/audio).
    pub fn audio(&self) -> Vec<i16> {
        self.engine.audio().to_vec()
    }

    /// A decoded snapshot of the live S-DSP registers for the UI inspector
    /// (M12/audio).
    #[wasm_bindgen(js_name = dspState)]
    pub fn dsp_state(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.engine.dsp_view()).map_err(Into::into)
    }

    /// The playing `score{}`'s [`crate::lua::ScoreView`] (`tick`, `length`,
    /// `song`, `slot`, `step`, `anchor`, `loopTick`, and `finished` for a
    /// `loop = false` song that played out, which keeps its anchor), or
    /// undefined when there is no score or it was stopped — the Sequencer playhead's readout.
    #[wasm_bindgen(js_name = scoreState)]
    pub fn score_state(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.engine.score_view()).map_err(Into::into)
    }

    /// Controller bitmask for the next frame (bit order: LuaEngine::PAD_NAMES).
    #[wasm_bindgen(js_name = setPad)]
    pub fn set_pad(&mut self, mask: u16) {
        self.engine.set_pad(mask);
    }

    /// The second controller's bitmask (Lua `pad2`), same bit order.
    #[wasm_bindgen(js_name = setPad2)]
    pub fn set_pad2(&mut self, mask: u16) {
        self.engine.set_pad2(mask);
    }

    /// Battery-backed save data: JSON of the Lua `sram` table. Set BEFORE
    /// setSources so init() sees it; invalid JSON resets to `{}`.
    #[wasm_bindgen(js_name = setSram)]
    pub fn set_sram(&mut self, json: &str) {
        self.engine.set_sram(json);
    }

    /// JSON of `sram` if a frame changed it since the last take, else undefined.
    #[wasm_bindgen(js_name = takeSram)]
    pub fn take_sram(&mut self) -> Option<String> {
        self.engine.take_sram()
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

    /// Per-scanline Mode-7 sampling segments ([u0,v0,u1,v1] f32 per row, NaN =
    /// row not in mode 7) from the most recent frame — the M7 editor's fan.
    #[wasm_bindgen(js_name = m7Scanlines)]
    pub fn m7_scanlines(&self) -> Vec<f32> {
        match &self.last_lt {
            Some(lt) => crate::mode7_scanline_segments(lt, WIDTH, HEIGHT),
            None => vec![],
        }
    }

    /// Every scanline's window registers from the most recent frame — 11 bytes
    /// per row (WH0-3, W12SEL, W34SEL, WOBJSEL, WBGLOG, WOBJLOG, TMW, TSW), the
    /// Windows editor's per-scanline feed. The window counterpart of
    /// `m7Scanlines`: it is what makes an `hdma()`-swept mask visible instead of
    /// the row-0 snapshot `registers()` reports.
    #[wasm_bindgen(js_name = winScanlines)]
    pub fn win_scanlines(&self) -> Vec<u8> {
        match &self.last_lt {
            Some(lt) => crate::window_scanline_bytes(lt),
            None => vec![],
        }
    }

    /// The full 1024x1024 Mode-7 plane as RGBA (register-free base image for
    /// the M7 editor panel).
    #[wasm_bindgen(js_name = m7MapView)]
    pub fn m7_map_view(&self) -> Vec<u8> {
        crate::render_mode7_map(self.engine.memory())
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
        priority_image: JsValue,
    ) -> Result<JsValue, JsValue> {
        let kind: crate::SourceKind = kind.parse().map_err(|e: String| JsValue::from_str(&e))?;
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
        let priority = if priority_image.is_undefined() || priority_image.is_null() {
            None
        } else {
            let get_priority = |k: &str| Reflect::get(&priority_image, &JsValue::from_str(k)).ok();
            let pw = get_priority("width")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as u32;
            let ph = get_priority("height")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as u32;
            if (pw, ph) != (width, height) {
                return Err(JsValue::from_str(
                    "priority mask dimensions must match color image",
                ));
            }
            Some(
                get_priority("data")
                    .map(|d| Uint8ClampedArray::new(&d).to_vec())
                    .unwrap_or_default(),
            )
        };
        let (payload, meta) = crate::convert_source_with_priority(
            kind,
            &opts,
            &rgba,
            width,
            height,
            priority.as_deref(),
        )
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

    /// Pure PNG import: PNG bytes -> versioned source payload + meta. An indexed PNG that is
    /// SNES art keeps its indices; anything else quantizes like `convertSource`.
    /// `priorityPng` is Mode 7 EXTBG's mask. Returns `{ payload: Uint8Array, meta: SourceMeta }`.
    #[wasm_bindgen(js_name = convertPngSource)]
    pub fn convert_png_source(
        &self,
        kind: &str,
        options: JsValue,
        png: &[u8],
        priority_png: Option<Vec<u8>>,
    ) -> Result<JsValue, JsValue> {
        let kind: crate::SourceKind = kind.parse().map_err(|e: String| JsValue::from_str(&e))?;
        let opts: crate::ConvertOptions = if options.is_undefined() || options.is_null() {
            Default::default()
        } else {
            serde_wasm_bindgen::from_value(options)?
        };
        let (payload, meta) = crate::convert_png_source(kind, &opts, png, priority_png.as_deref())
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

    /// Pure render: a bg/sheet/obj/m7 payload + its `meta` (width, height, and a sheet's or obj's
    /// `cells`) -> the indexed PNG bytes that `convertPngSource` reads back exactly. `palette` is
    /// the payload of the palette source the art names, if any.
    #[wasm_bindgen(js_name = renderPng)]
    pub fn render_png(
        &self,
        payload: &[u8],
        meta: JsValue,
        palette: Option<Vec<u8>>,
    ) -> Result<Vec<u8>, JsValue> {
        let meta: crate::SourceMeta = serde_wasm_bindgen::from_value(meta)?;
        let payload =
            crate::SourcePayload::decode(payload).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let palette = match palette.as_deref().map(crate::SourcePayload::decode) {
            None => None,
            Some(Ok(crate::SourcePayload::Palette(p))) => Some(p),
            Some(Ok(_)) => return Err(JsValue::from_str("the palette is not a palette source")),
            Some(Err(e)) => return Err(JsValue::from_str(&format!("the palette: {e}"))),
        };
        crate::render::render_png(&payload, &meta, palette.as_ref())
            .map_err(|e| JsValue::from_str(&e))
    }

    /// Pure palette-fit check: whether the palette payload can stand in for the art payload's own
    /// colours (`source::palette_fits`, the rule dma applies). Undefined = fits, else the reason.
    #[wasm_bindgen(js_name = paletteFits)]
    pub fn palette_fits(&self, art: &[u8], palette: &[u8]) -> Option<String> {
        let art = match crate::SourcePayload::decode(art) {
            Ok(a) => a,
            Err(e) => return Some(e.to_string()),
        };
        match crate::SourcePayload::decode(palette) {
            Ok(crate::SourcePayload::Palette(p)) => crate::source::palette_fits(&art, &p).err(),
            Ok(_) => Some("not a palette source".into()),
            Err(e) => Some(e.to_string()),
        }
    }

    /// Pure `.gpl` conversion: palette text -> versioned palette payload + meta.
    /// Returns `{ payload: Uint8Array, meta: SourceMeta }`.
    #[wasm_bindgen(js_name = convertPalette)]
    pub fn convert_palette(&self, text: &str) -> Result<JsValue, JsValue> {
        let (payload, meta) =
            crate::gpl::convert_palette(text).map_err(|e| JsValue::from_str(&e))?;
        let out = Object::new();
        Reflect::set(
            &out,
            &"payload".into(),
            &Uint8Array::from(payload.encode().as_slice()).into(),
        )?;
        Reflect::set(&out, &"meta".into(), &serde_wasm_bindgen::to_value(&meta)?)?;
        Ok(out.into())
    }

    /// Pure BRR encode: 32 kHz i16 mono PCM -> versioned sample payload +
    /// meta. No engine state mutation. Returns `{ payload: Uint8Array, meta: SourceMeta }`.
    #[wasm_bindgen(js_name = convertSample)]
    pub fn convert_sample(&self, pcm: &[i16], options: JsValue) -> Result<JsValue, JsValue> {
        let opts: crate::ConvertSampleOptions = if options.is_undefined() || options.is_null() {
            Default::default()
        } else {
            serde_wasm_bindgen::from_value(options)?
        };
        let (payload, meta) =
            crate::convert_sample(pcm, &opts).map_err(|e| JsValue::from_str(&e))?;
        let out = Object::new();
        Reflect::set(
            &out,
            &"payload".into(),
            &Uint8Array::from(payload.encode().as_slice()).into(),
        )?;
        Reflect::set(&out, &"meta".into(), &serde_wasm_bindgen::to_value(&meta)?)?;
        Ok(out.into())
    }

    /// A built-in bank sound (`dma("piano")` with nothing uploaded) as
    /// `{ payload: Uint8Array, meta: SourceMeta }`, the shape `convertSample`
    /// returns, or undefined for a name the bank doesn't have.
    #[wasm_bindgen(js_name = builtinSample)]
    pub fn builtin_sample(&self, name: &str) -> Result<JsValue, JsValue> {
        let Some((payload, meta)) = crate::bank::get_with_meta(name) else {
            return Ok(JsValue::UNDEFINED);
        };
        let out = Object::new();
        Reflect::set(
            &out,
            &"payload".into(),
            &Uint8Array::from(payload.encode().as_slice()).into(),
        )?;
        Reflect::set(&out, &"meta".into(), &serde_wasm_bindgen::to_value(&meta)?)?;
        Ok(out.into())
    }

    /// The palette of each char of sheet source `name` (the `pal` of each `meta.cells` entry
    /// `convertSource` returned), which a Tiled map drawn from the sheet needs.
    #[wasm_bindgen(js_name = setSheetPalettes)]
    pub fn set_sheet_palettes(&mut self, name: &str, pals: &[u8]) {
        self.engine.set_sheet_palettes(name, pals);
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

    /// Forget a registered source. Returns whether the name existed.
    #[wasm_bindgen(js_name = removeSource)]
    pub fn remove_source(&mut self, name: &str) -> bool {
        self.engine.remove_source(name)
    }
}

impl Default for PpuCore {
    fn default() -> Self {
        Self::new()
    }
}

/// A Lua toy's files as a Luau toy's: `files` is `[{name, source}]` in tab order; the result is
/// `{files: [{name, source}], changes, unconverted}`, each note `{file, line, message}` (see `convert`).
#[wasm_bindgen(js_name = convertToLuau)]
pub fn convert_to_luau(files: JsValue) -> Result<JsValue, JsValue> {
    let files: Vec<SourceFileIn> = serde_wasm_bindgen::from_value(files)?;
    let pairs: Vec<(&str, &str)> = files
        .iter()
        .map(|f| (f.name.as_str(), f.source.as_str()))
        .collect();
    serde_wasm_bindgen::to_value(&crate::convert::convert(&pairs)).map_err(Into::into)
}

/// Live pokes written into scene module `file` (see `apply`): `edits` is `[{kind: "assign", lvalue,
/// expr}]`; the result is `{source, written, notes}`, `written` the 1-based lines Apply wrote. A
/// refusal (a Lua file, a source that does not parse, an unknown poke kind) throws its message.
#[wasm_bindgen(js_name = applyPokes)]
pub fn apply_to_scene(file: &str, source: &str, edits: JsValue) -> Result<JsValue, JsValue> {
    let edits: Vec<crate::apply::Edit> = serde_wasm_bindgen::from_value(edits)?;
    let applied = crate::apply::apply(file, source, &edits).map_err(|e| JsValue::from_str(&e))?;
    serde_wasm_bindgen::to_value(&applied).map_err(Into::into)
}

/// A source made from VRAM words (see `generated_source`): `{payload: Uint8Array, meta}`; `cells`
/// (`{cell_w, cell_h, cells}`, an obj source's) is undefined for a sheet. A kind it does not make,
/// or words that are not whole tiles, throw.
#[wasm_bindgen(js_name = generatedSource)]
pub fn generated_source(
    kind: &str,
    bit_depth: u8,
    char_words: &[u16],
    cells: JsValue,
) -> Result<JsValue, JsValue> {
    let kind: crate::SourceKind = kind.parse().map_err(|e: String| JsValue::from_str(&e))?;
    let cells: Option<crate::GeneratedCells> = serde_wasm_bindgen::from_value(cells)?;
    let (payload, meta) = crate::generated_source(kind, bit_depth, char_words, cells.as_ref())
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

// --- songs: static calls, no core instance --------------------------------
//
// Each returns `{ ok: true, value }` or `{ ok: false, error }`. Decode and
// encode both compile the song, so a bad song fails with the same message
// `score{}` setup would give it.

fn song_result(res: Result<JsValue, crate::song::SongError>) -> Result<JsValue, JsValue> {
    let out = Object::new();
    match res {
        Ok(value) => {
            Reflect::set(&out, &"ok".into(), &true.into())?;
            Reflect::set(&out, &"value".into(), &value)?;
        }
        Err(e) => {
            Reflect::set(&out, &"ok".into(), &false.into())?;
            Reflect::set(&out, &"error".into(), &e.0.into())?;
        }
    }
    Ok(out.into())
}

fn checked_song(bytes: &[u8]) -> Result<crate::song::Song, crate::song::SongError> {
    let song = crate::song::decode(bytes)?;
    crate::song::compile(&song)?;
    Ok(song)
}

/// PSNG bytes -> the song object (`SongData` in core.ts).
#[wasm_bindgen(js_name = decodeSong)]
pub fn decode_song(bytes: &[u8]) -> Result<JsValue, JsValue> {
    song_result(checked_song(bytes).and_then(|s| {
        serde_wasm_bindgen::to_value(&s).map_err(|e| crate::song::SongError(e.to_string()))
    }))
}

/// The panel's write gate: the song object -> PSNG bytes (`Uint8Array`),
/// only after the full check `score{}` setup runs (validate + compile);
/// an invalid song returns that error, never bytes.
#[wasm_bindgen(js_name = encodeSong)]
pub fn encode_song(song: JsValue) -> Result<JsValue, JsValue> {
    song_result(
        serde_wasm_bindgen::from_value::<crate::song::Song>(song)
            .map_err(|e| crate::song::SongError(e.to_string()))
            .and_then(|s| {
                crate::song::compile(&s)?;
                Ok(Uint8Array::from(crate::song::encode(&s).as_slice()).into())
            }),
    )
}

/// write_song's argument (the authored song JSON, as text: `song::Authored`) -> the song
/// object, or the key or field that is wrong.
#[wasm_bindgen(js_name = songFromAuthored)]
pub fn song_from_authored(text: &str) -> Result<JsValue, JsValue> {
    song_result(crate::song::from_json(text).and_then(|s| {
        serde_wasm_bindgen::to_value(&s).map_err(|e| crate::song::SongError(e.to_string()))
    }))
}

/// .mid bytes -> `{ song, rounded }`: the song object and how many notes moved onto the
/// 1/48-beat grid, or why the file is not a song (`import::midi`).
#[wasm_bindgen(js_name = songFromMidi)]
pub fn song_from_midi(bytes: &[u8]) -> Result<JsValue, JsValue> {
    song_result(
        crate::import::midi::import(bytes)
            .map_err(crate::song::SongError)
            .and_then(|i| {
                let out = Object::new();
                let song = serde_wasm_bindgen::to_value(&i.song)
                    .map_err(|e| crate::song::SongError(e.to_string()))?;
                Reflect::set(&out, &"song".into(), &song).expect("set on a fresh object");
                Reflect::set(&out, &"rounded".into(), &i.rounded.into())
                    .expect("set on a fresh object");
                Ok(out.into())
            }),
    )
}

/// The song object -> read_song's answer: the authored song JSON, as compact text.
#[wasm_bindgen(js_name = songToAuthored)]
pub fn song_to_authored(song: JsValue) -> Result<JsValue, JsValue> {
    song_result(
        serde_wasm_bindgen::from_value::<crate::song::Song>(song)
            .map_err(|e| crate::song::SongError(e.to_string()))
            .map(|s| {
                serde_json::to_string(&crate::song::Authored::of(&s))
                    .expect("a song serializes")
                    .into()
            }),
    )
}

/// PSNG bytes -> the song_check report as text, `name` heading it (`song_check::Report::lines`).
/// `samples` is an array of `[name, source payload]` pairs: the toy's samples a row may name.
#[wasm_bindgen(js_name = songCheck)]
pub fn song_check(bytes: &[u8], name: &str, samples: js_sys::Array) -> Result<JsValue, JsValue> {
    let mut owned = Vec::new();
    for pair in samples.iter() {
        let pair = js_sys::Array::from(&pair);
        let sample = pair
            .get(0)
            .as_string()
            .ok_or("a sample's name is a string")?;
        owned.push((sample, Uint8Array::new(&pair.get(1)).to_vec()));
    }
    let samples: Vec<(&str, &[u8])> = owned
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect();
    song_result(
        crate::song::decode(bytes)
            .and_then(|s| crate::song_check::check(&s, &samples))
            .map(|r| r.lines(name).join("\n").into()),
    )
}

/// PSNG bytes -> the piano roll `ppu song roll` draws, as PNG bytes (`song_roll`); `from` and
/// `to` (both drawn) crop it to arrangement slots, all of them when either is undefined.
#[wasm_bindgen(js_name = songRoll)]
pub fn song_roll(bytes: &[u8], from: Option<u32>, to: Option<u32>) -> Result<JsValue, JsValue> {
    let slots = from.zip(to).map(|(a, b)| (a as usize, b as usize));
    song_result(
        crate::song::decode(bytes)
            .and_then(|s| crate::song_roll::roll(&s, slots))
            .map(|r| Uint8Array::from(crate::song_roll::encode_png(&r).as_slice()).into()),
    )
}

/// `ppu sfx`'s arguments as one string ("laser --pitch 900 --length 200") -> `{ wav, stats }`:
/// the WAV bytes `ppu sfx` writes and its JSON report as text, or every problem with the request.
#[wasm_bindgen(js_name = sfxWav)]
pub fn sfx_wav(args: &str) -> Result<JsValue, JsValue> {
    let args: Vec<String> = args.split_whitespace().map(String::from).collect();
    song_result(
        crate::sfx::parse(&args)
            .and_then(|r| crate::sfx::make(r.preset, &r.knobs))
            .map_err(crate::song::SongError)
            .map(|(wav, stats)| {
                let out = Object::new();
                Reflect::set(
                    &out,
                    &"wav".into(),
                    &Uint8Array::from(wav.as_slice()).into(),
                )
                .expect("set on a fresh object");
                Reflect::set(&out, &"stats".into(), &stats.to_json().to_string().into())
                    .expect("set on a fresh object");
                out.into()
            }),
    )
}

/// PSNG bytes -> `{ events, length, loopTick, slotSteps, stepTicks, used, wanted, over }`, with
/// events flattened to a Float64Array of `row, start, end, voice` quads (the
/// TS wrapper builds the objects; per-object serde is the slow path).
#[wasm_bindgen(js_name = analyzeSong)]
pub fn analyze_song(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let a = match crate::song::decode(bytes).and_then(|s| crate::song_analyze::analyze(&s)) {
        Ok(a) => a,
        Err(e) => return song_result(Err(e)),
    };
    let flat: Vec<f64> = a
        .events
        .iter()
        .flat_map(|e| [e.row as f64, e.start as f64, e.end as f64, e.voice as f64])
        .collect();
    let out = Object::new();
    Reflect::set(
        &out,
        &"events".into(),
        &js_sys::Float64Array::from(flat.as_slice()).into(),
    )?;
    Reflect::set(&out, &"length".into(), &(a.length as f64).into())?;
    Reflect::set(&out, &"loopTick".into(), &(a.loop_tick as f64).into())?;
    Reflect::set(
        &out,
        &"slotSteps".into(),
        &js_sys::Uint32Array::from(a.slot_steps.as_slice()).into(),
    )?;
    Reflect::set(
        &out,
        &"stepTicks".into(),
        &js_sys::Uint32Array::from(a.step_ticks.as_slice()).into(),
    )?;
    Reflect::set(
        &out,
        &"used".into(),
        &Uint8Array::from(a.used.as_slice()).into(),
    )?;
    Reflect::set(
        &out,
        &"wanted".into(),
        &js_sys::Uint32Array::from(a.wanted.as_slice()).into(),
    )?;
    Reflect::set(
        &out,
        &"over".into(),
        &js_sys::Uint32Array::from(a.over.as_slice()).into(),
    )?;
    song_result(Ok(out.into()))
}

/// `crate::sync::hashes` of a project body (`{title, files: [{name, source}], sources, ...}`): an
/// object of entry key (`file main.luau`, `source ship`, `meta title`) to hash, as connect links store.
#[wasm_bindgen(js_name = syncHashes)]
pub fn sync_hashes(body: JsValue) -> Result<JsValue, JsValue> {
    let body: serde_json::Value = serde_wasm_bindgen::from_value(body)?;
    to_object(&crate::sync::hashes(&body))
}

/// `crate::sync::compare` of three key-to-hash objects (base, mine, theirs; an absent key is an
/// absent entry): `{take: string[], clashes: string[]}`, the entries to take from theirs and the
/// ones both sides changed differently.
#[wasm_bindgen(js_name = syncCompare)]
pub fn sync_compare(base: JsValue, mine: JsValue, theirs: JsValue) -> Result<JsValue, JsValue> {
    type Hashes = std::collections::BTreeMap<String, String>;
    let read = |v: JsValue| serde_wasm_bindgen::from_value::<Hashes>(v);
    to_object(&crate::sync::compare(&read(base)?, &read(mine)?, &read(theirs)?))
}

/// Maps as plain JS objects (serde_wasm_bindgen's default is a `Map`).
fn to_object<T: serde::Serialize>(v: &T) -> Result<JsValue, JsValue> {
    v.serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(Into::into)
}
