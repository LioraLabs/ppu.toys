//! piccolo Lua VM + flat-global DSL binding. Runs `frame(t,f)` once to populate
//! frame-wide defaults + CGRAM/OAM, registers `hdma` hooks, then (Phase A)
//! applies `ppuglobals.lua`'s `apply_pokes` as a synthetic frame-wide `hdma`
//! hook spliced in after the program's own hooks, then resolves the
//! LineTable by invoking each covering hook per scanline (later call wins).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use piccolo::{
    Callback, CallbackReturn, Closure, Executor, FromMultiValue, Function, Lua, PrototypeError,
    StashedFunction, StaticError, Table, Value,
};

use crate::{
    rgb15, AudioMix, Dsp, LineTable, LineTableBuilder, LineTableRow, Memory, AUDIO_MIX_FILE, HEIGHT,
};

/// Per-frame placement diagnostics surfaced to the UI (assets panel/inspector).
#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "mode")]
pub enum ImportBudget {
    /// A `dma()` placement whose source is gone from the store (removed after
    /// init): the placement places nothing and this diagnostic says why.
    /// `expected` describes the placement, `found` the failure.
    #[serde(rename = "mismatch")]
    Mismatch {
        #[serde(skip_serializing_if = "Option::is_none")]
        layer: Option<usize>,
        slot: String,
        expected: String,
        found: String,
    },
}

/// One `dma()`-claimed span of VRAM (word addresses) or CGRAM (entries),
/// `start..end` half-open, tagged with the source that owns it.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct MemoryRange {
    pub name: String,
    pub start: usize,
    pub end: usize,
}

/// One recorded `dma()` placement as the DMA panel sees it.
#[derive(Clone, Debug, serde::Serialize)]
pub struct PlacementView {
    pub name: String,
    pub char: u16,
    pub map: u16,
    pub pal: u8,
}

/// What the current program's init window placed where — the DMA panel's
/// memory map. Recorded at recompile, so it only changes with one.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct MemoryMap {
    pub vram: Vec<MemoryRange>,
    pub cgram: Vec<MemoryRange>,
    pub placements: Vec<PlacementView>,
}

/// L/R pair for a signed 8-bit volume-style register (VOL, MVOL, EVOL).
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DspLr {
    pub l: i8,
    pub r: i8,
}

/// A voice's ADSR1/ADSR2 fields, decoded to their raw 4/3/3/5-bit ranges.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DspAdsr {
    pub a: u8,
    pub d: u8,
    pub s: u8,
    pub r: u8,
}

/// The echo unit's tunable fields (buffer position is derived from `delay`,
/// not surfaced here — see `write_dsp_regs`'s ESA formula).
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DspEchoView {
    pub delay: u8,
    pub feedback: i8,
    pub fir: [i8; 8],
}

/// One voice's full register state, decoded for the UI inspector.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DspVoiceView {
    pub sample: u8,
    pub pitch: u16,
    pub vol: DspLr,
    pub adsr: DspAdsr,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gain: Option<u8>,
    pub noise: bool,
    pub pmod: bool,
    pub echo: bool,
    pub envx: u8,
    pub outx: i8,
    pub ended: bool,
}

/// One `dma()` sample placement, as surfaced to the UI (Audio inspector) —
/// see [`LuaEngine::dsp_view`]. `start`/`end` are ARAM byte offsets
/// (`end` exclusive).
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DspSampleView {
    pub id: u8,
    pub name: String,
    pub start: u32,
    pub end: u32,
}

/// A full snapshot of the live S-DSP registers, decoded for the UI inspector
/// (M12/audio) — see [`LuaEngine::dsp_view`].
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DspView {
    pub voices: [DspVoiceView; 8],
    pub mvol: DspLr,
    pub evol: DspLr,
    pub echo: DspEchoView,
    pub noise_clock: u8,
    pub mute: bool,
    /// Sample sources placed into ARAM by `dma()` at compile time, in
    /// placement (call) order — filled by [`LuaEngine::dsp_view`] from
    /// `self.dma.samples`, not by `decode_dsp_view` (which has no access to
    /// the recorder).
    pub samples: Vec<DspSampleView>,
}

/// Where the playing `score{}` is, in its 4 ms timer ticks — see
/// [`LuaEngine::score_view`]. `tick` is the next tick to play, `0..length-1`;
/// `song` is the id of a `score{ song = "<id>" }` (absent for `data =`).
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct ScoreView {
    pub tick: i64,
    pub length: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub song: Option<String>,
}

/// The controls document's reserved file name (PPU-146's generated
/// `ppuglobals.lua`): its `apply_pokes` is applied automatically every frame
/// as a final visual override pass — see [`LuaEngine::load_controls`] and
/// `frame()`'s Phase A. Filtered out of the normal multi-file chunk list the
/// same way [`crate::AUDIO_MIX_FILE`] is.
pub const CONTROLS_FILE: &str = "ppuglobals.lua";

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
    /// Pre-interned glue keys for `lua` (see [`Keys`]); swapped with it.
    keys: Rc<StashedKeys>,
    /// Diff-on-write baseline for `write_state` (see [`Mirror`]). Valid only
    /// between one `read_state` and the next `write_state` with no other
    /// global writes in between — `frame()` resets it around everything else.
    mirror: Rc<RefCell<Mirror>>,
    frame_fn: Option<StashedFunction>,
    init_fn: Option<StashedFunction>,
    /// Defining chunk of `frame_fn`, for runtime error attribution.
    frame_file: Option<String>,
    /// `ppuglobals.lua`'s `apply_pokes`, stashed from the tracked
    /// `__ppu_controls_env` (see [`Self::load_controls`]). `None` when the
    /// current program carries no controls document — `frame()`'s Phase A
    /// is then a no-op, matching pre-M147 behavior exactly.
    controls_fn: Option<StashedFunction>,
    /// Painted tiles captured from `ppuglobals.lua`'s `apply_vram()` at load;
    /// overlaid onto VRAM every frame after all Lua has run.
    controls_vram: Vec<(u16, u16)>,
    memory: Memory,
    /// The source store: decoded `addSource` payloads keyed by name — the
    /// graphics-data home (kind + depth + palettes + tiles + tilemap all
    /// self-described by the payload). App-level; survives recompiles (NOT
    /// cleared by set_sources, which owns Lua files). Shared with the `dma`
    /// callback installed in the VM (init-time name/kind validation).
    source_store: Rc<RefCell<HashMap<String, crate::source::SourcePayload>>>,
    /// Placements recorded by `dma()` during the current program's init
    /// (top-level chunks + `init()`), replayed into VRAM/CGRAM each frame.
    /// Swapped wholesale on a successful `set_sources` (recompile).
    dma: Rc<DmaRecorder>,
    /// Per-layer import/diagnostic reports produced by the most recent `frame()`.
    reports: Vec<ImportBudget>,
    program_sources: Vec<(String, String)>,
    source_dirty: bool,
    /// Controller state for the next frame: a PAD_* bitmask mirrored into the
    /// Lua `pad` table before frame() runs. JS owns the key/gamepad mapping.
    pad: u16,
    /// The live S-DSP core (M12/audio). Survives recompiles and `Memory::new()`
    /// resets — only `render_frame_audio` writes to it.
    dsp: Dsp,
    /// The 64 KB ARAM the DSP renders from/into. Boxed so `LuaEngine` doesn't
    /// carry a 64 KB inline array. Survives recompiles; only the `aram[]`
    /// Lua table (drained once per frame) and the echo unit itself write here.
    aram: Box<[u8; 0x10000]>,
    /// The most recently rendered frame's interleaved stereo audio (L,R per
    /// sample). Empty before the first `frame()`.
    audio: Vec<i16>,
    /// Fractional-sample accumulator for the 32000/60.0988 span length (see
    /// `render_frame_audio`): exact over time, no drift.
    audio_acc: u32,
    /// `timer(n, div, fn)` hooks registered by the current program's init
    /// window (top-level chunks + `init()`). Dropped and re-registered
    /// wholesale on every `set_sources` (recompile resets timer phase — see
    /// `DmaRecorder::timers`).
    timers: Vec<TimerHook>,
    /// Battery-backed save data: the `sram` global mirrored as normalized JSON.
    /// The host sets it before the program loads (`set_sram`); `frame()`
    /// re-serializes the table and flags a change for the host to persist.
    sram_json: String,
    sram_dirty: bool,
    saved_mix: AudioMix,
    live_mix: Option<AudioMix>,
    unmixed_dsp: Option<DspView>,
    mix_kon: u8,
    mix_koff: u8,
}

/// One `timer(n, div, fn)` registration, resolved to half-sample (`h`) units
/// at 32 kHz output (see `render_frame_audio`'s segment-walker doc comment):
/// `period_h` is `div * 8` for timers 0/1 (8 kHz) or `div * 1` for timer 2
/// (64 kHz); `due_h` is the next expiry and carries across frames.
struct TimerHook {
    period_h: u64,
    due_h: u64,
    func: StashedFunction,
    file: Option<String>,
}

/// Size cap of the serialized `sram` blob — a real cartridge's battery RAM.
pub const SRAM_MAX_BYTES: usize = 32 * 1024;
const SRAM_MAX_DEPTH: usize = 32;

/// The resolved result of one init-stage `dma(name, opts?)` call. Replay
/// resolves `name` against the LIVE source store each frame, so an
/// `add_source` under the same name flows into the placement without a
/// recompile, and a `remove_source` degrades to a Mismatch report (same UX
/// as the old binding path) instead of a stale copy.
struct DmaPlacement {
    name: String,
    char_base: u16,
    map_base: u16,
    cgram_base: u8,
}

/// The resolved result of one init-stage `dma(name, { addr })` call on a
/// SAMPLE source. Written into ARAM + the sample directory ONCE at
/// compile time (`set_sources`, after `init()` succeeds) — never replayed
/// per frame, unlike [`DmaPlacement`]. `end` is exclusive; `loop_addr` equals
/// `addr` for a non-looping sample.
#[derive(Clone)]
struct SamplePlacement {
    name: String,
    id: u8,
    addr: u16,
    end: u32,
    loop_addr: u16,
}

/// Fixed ARAM home of the 256-entry sample directory (DIR pinned to page
/// 0x01 — see `LuaEngine::new`'s DSP power-on write of `dsp.write(0x5d, ..)`).
const SAMPLE_DIR: u32 = 0x0100;
/// End (exclusive) of the sample directory page: 256 entries * 4 bytes.
const SAMPLE_DIR_END: u32 = SAMPLE_DIR + 256 * 4;

/// The half-open ARAM range `[start, end)` the echo buffer reserves for a
/// given `dsp.echo.delay` (0..=15, already clamped by the caller). Echo RAM
/// sits at the TOP of ARAM: delay 0 is the documented "no echo writes"
/// sentinel (ESA 0xff, a 4-byte placeholder region); every other delay
/// reserves `delay*0x800` bytes ending exactly at 0x10000.
fn echo_region(delay: i64) -> (u32, u32) {
    if delay == 0 {
        (0xff00, 0xff04)
    } else {
        (0x10000 - delay as u32 * 0x800, 0x10000)
    }
}

/// `dma()`/`timer()` call recorder shared between the engine and the
/// callbacks installed in its VM. `active` is true only while `set_sources`
/// executes top-level chunks + `init()` — the init-only gate both `dma` and
/// `timer` share.
#[derive(Default)]
struct DmaRecorder {
    active: Cell<bool>,
    placements: RefCell<Vec<DmaPlacement>>,
    vram_ranges: RefCell<Vec<(String, usize, usize)>>,
    cgram_ranges: RefCell<Vec<(String, usize, usize)>>,
    obj_base: Cell<Option<u16>>,
    /// Sample placements recorded during the init window, in call order.
    /// Never replayed — `replay_dma`'s `Sample(_) => {}` arm is a deliberate
    /// no-op: samples are written once by `LuaEngine::set_sources` after
    /// `init()`, not per frame.
    samples: RefCell<Vec<SamplePlacement>>,
    /// `timer(n, div, fn)` registrations recorded during the init window:
    /// (timer index 0..=2, div 1..=255, stashed hook, defining chunk). Moved
    /// into `LuaEngine::timers` (with computed period_h/due_h) once
    /// `set_sources` confirms `init()` succeeded.
    timers: RefCell<Vec<(u8, u8, StashedFunction, Option<String>)>>,
}

impl Default for LuaEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// A freshly power-cycled S-DSP: ADSR mode (gain nil) on all 8 voices — see
/// the `Dsp::new()` state doc comment (a fresh Dsp otherwise leaves ADSR1 at
/// 0, which is GAIN mode with gain 0) — and the sample directory pinned at
/// ARAM 0x0100 (DIR = 0x01; the DSL never exposes DIR). Used by both
/// `LuaEngine::new()` and `LuaEngine::reset()`.
fn power_on_dsp() -> Dsp {
    let mut dsp = Dsp::new();
    for v in 0..8u8 {
        dsp.write((v << 4) | 0x05, 0x80);
    }
    dsp.write(0x5d, (SAMPLE_DIR >> 8) as u8);
    dsp
}

impl LuaEngine {
    pub fn new() -> Self {
        let source_store = Rc::new(RefCell::new(HashMap::new()));
        let dma = Rc::new(DmaRecorder::default()); // inactive: no code has run
        let mut lua = Lua::core();
        let keys = Rc::new(lua.enter(StashedKeys::new));
        lua.enter(|ctx| install_bindings(ctx, &keys.fetch(ctx)));
        {
            let (store, rec) = (source_store.clone(), dma.clone());
            lua.enter(move |ctx| install_dma(ctx, store, rec));
        }
        let dsp = power_on_dsp();
        lua.enter(|ctx| seed_dsp_tables(ctx, &dsp, None));
        LuaEngine {
            lua: Rc::new(RefCell::new(lua)),
            keys,
            mirror: Rc::default(),
            frame_fn: None,
            init_fn: None,
            frame_file: None,
            controls_fn: None,
            controls_vram: Vec::new(),
            memory: Memory::new(),
            source_store,
            dma,
            reports: Vec::new(),
            program_sources: Vec::new(),
            source_dirty: false,
            pad: 0,
            dsp,
            aram: Box::new([0u8; 0x10000]),
            audio: Vec::new(),
            audio_acc: 0,
            timers: Vec::new(),
            sram_json: "{}".to_string(),
            sram_dirty: false,
            saved_mix: AudioMix::default(),
            live_mix: None,
            unmixed_dsp: None,
            mix_kon: 0,
            mix_koff: 0,
        }
    }

    /// Mirrored PPU memory after the most recent `frame()`.
    pub fn memory(&self) -> &Memory {
        &self.memory
    }

    /// Decode + register a source payload under `name` (the `addSource` core).
    /// The store is the graphics-data home; `dma()` validates against it at
    /// init and the per-frame replay places from it (no importer on this path).
    pub fn add_source(
        &mut self,
        name: &str,
        payload: &[u8],
    ) -> Result<(), crate::source::PayloadError> {
        let p = crate::source::SourcePayload::decode(payload)?;
        self.source_store.borrow_mut().insert(name.to_string(), p);
        self.source_dirty = !self.program_sources.is_empty();
        Ok(())
    }

    /// Forget the source registered under `name` (the `removeSource` core).
    /// The next frame reruns setup, so a remaining `dma()` reference fails
    /// loudly like any other missing source.
    pub fn remove_source(&mut self, name: &str) -> bool {
        let removed = self.source_store.borrow_mut().remove(name).is_some();
        self.source_dirty |= removed && !self.program_sources.is_empty();
        removed
    }

    /// Per-layer import budgets from the most recent `frame()` (m4/inspector).
    pub fn import_reports(&self) -> &[ImportBudget] {
        &self.reports
    }

    /// Every VRAM/CGRAM span and placement the last recompile's `dma()`
    /// calls recorded (samples excluded — they live in ARAM).
    pub fn memory_map(&self) -> MemoryMap {
        let range = |(name, start, end): &(String, usize, usize)| MemoryRange {
            name: name.clone(),
            start: *start,
            end: *end,
        };
        MemoryMap {
            vram: self.dma.vram_ranges.borrow().iter().map(range).collect(),
            cgram: self.dma.cgram_ranges.borrow().iter().map(range).collect(),
            placements: self
                .dma
                .placements
                .borrow()
                .iter()
                .map(|p| PlacementView {
                    name: p.name.clone(),
                    char: p.char_base,
                    map: p.map_base,
                    pal: p.cgram_base,
                })
                .collect(),
        }
    }

    /// Mutable mirrored memory — used by the wasm shim (e.g. to clear OAM on-flags
    /// for the layer-visibility override).
    pub fn memory_mut(&mut self) -> &mut Memory {
        &mut self.memory
    }

    /// The most recently rendered frame's interleaved stereo audio (L,R per
    /// sample, [`crate::dsp::SAMPLE_RATE`]). Empty before the first
    /// `frame()`. An errored `frame()` (before the audio pass — see
    /// `render_frame_audio`) leaves the *previous* clean frame's span
    /// untouched. An errored timer hook aborts the segment walker partway,
    /// so this holds THIS frame's rendered prefix followed by a stale tail —
    /// leftover samples from the previous frame, since `Vec::resize` doesn't
    /// zero elements that already existed. An errored hdma hook runs AFTER
    /// the audio pass has already completed, so it leaves this frame's fully
    /// and cleanly rendered span. Either way this never reaches the web:
    /// the wasm shim (`web/src/ppu/wasm.ts`) throws on the `frame()` error
    /// before it ever reads `core.audio()`.
    pub fn audio(&self) -> &[i16] {
        &self.audio
    }

    /// The live S-DSP core — read-only; all writes go through the `dsp`/
    /// `voice[]` DSL tables, flushed by `render_frame_audio`.
    pub fn dsp(&self) -> &Dsp {
        &self.dsp
    }

    /// A decoded snapshot of the live DSP registers for the UI inspector,
    /// plus the sample sources placed into ARAM by `dma()` (compile-time,
    /// not part of `Dsp`'s own state — filled here from `self.dma.samples`).
    pub fn dsp_view(&self) -> DspView {
        let mut view = decode_dsp_view(&self.dsp);
        view.samples = self
            .dma
            .samples
            .borrow()
            .iter()
            .map(|s| DspSampleView {
                id: s.id,
                name: s.name.clone(),
                start: s.addr as u32,
                end: s.end,
            })
            .collect();
        view
    }

    /// The most recently started `score{}` (kit.lua publishes its handle as
    /// `__score`), or None when there is none or it is stopped/finished.
    pub fn score_view(&self) -> Option<ScoreView> {
        self.lua.borrow_mut().enter(|ctx| {
            let Value::Table(h) = ctx.get_global("__score") else {
                return None;
            };
            if !h.get(ctx, "playing").to_bool() {
                return None;
            }
            let length = h.get(ctx, "length").to_int().filter(|&l| l > 0)?;
            // The timer leaves `tick == length` between the last tick and the
            // wrap; the next tick to play is then 0.
            Some(ScoreView {
                tick: h.get(ctx, "tick").to_int()? % length,
                length,
                song: match h.get(ctx, "song") {
                    Value::String(s) => Some(s.to_str_lossy().into_owned()),
                    _ => None,
                },
            })
        })
    }

    /// Single-file sugar for [`Self::set_sources`]; the chunk keeps its
    /// historical name `"source"` so existing diagnostics are unchanged.
    pub fn set_source(&mut self, src: &str) -> Result<(), LuaError> {
        self.set_sources(&[("source", src)])
    }

    /// The sequencer sugar prelude (`note`/`instrument`/`sfx`/`song`), run as
    /// the `"kit"` chunk before every user chunk in [`Self::set_sources`] —
    /// see `kit.lua`'s own doc comment.
    const KIT_LUA: &'static str = include_str!("kit.lua");

    /// The change-tracking environment `ppuglobals.lua` is compiled against —
    /// see `controls_env.lua`'s own doc comment and [`Self::load_controls`].
    const CONTROLS_ENV_LUA: &'static str = include_str!("controls_env.lua");

    /// Compile and load a multi-file sketch (PICO-8 scope): builds a fresh VM,
    /// installs bindings, then executes each `(name, source)` chunk **in list
    /// order** into ONE shared global environment, each compiled with its file
    /// name as chunk name. `frame`/`init` are resolved only after every chunk
    /// has run (`main.lua` is convention, not special-cased); `init()` runs
    /// once if present. Errors carry `{file, line?, message}`.
    pub fn set_sources(&mut self, files: &[(&str, &str)]) -> Result<(), LuaError> {
        // Older saved toys used controls.lua for the generated document.
        // A user's controls.lua stays ordinary code; an existing ppuglobals.lua wins.
        let has_globals = files.iter().any(|(name, _)| *name == CONTROLS_FILE);
        let normalized: Vec<_> = files
            .iter()
            .map(|&(name, source)| {
                if !has_globals
                    && name == "controls.lua"
                    && source.starts_with("-- controls.lua · generated by the Studio panels.")
                {
                    (CONTROLS_FILE, source)
                } else {
                    (name, source)
                }
            })
            .collect();
        let files = normalized.as_slice();
        let saved_mix = AudioMix::parse(
            files
                .iter()
                .find(|(name, _)| *name == AUDIO_MIX_FILE)
                .map(|(_, s)| *s)
                .unwrap_or("{}"),
        )
        .map_err(|message| LuaError {
            message,
            line: None,
            file: Some(AUDIO_MIX_FILE.into()),
        })?;
        // Saving only the mix and/or ppuglobals.lua must preserve the VM,
        // timer phase, and sounding notes. `live` names the files this fast
        // path may reload in place without a full recompile — CONTROLS_FILE
        // joins AUDIO_MIX_FILE here (PPU-147): a controls-only edit is
        // exactly the same "reload without recompiling" shape a mix-only
        // edit already was.
        //
        // Song files join them: a file whose text changed but which was, and
        // still is, a song file for the same `seq_<id>` (see `song_id`) is
        // re-run in the live VM and its bound `score{ song = id }`s reload in
        // place. A song file added, removed or renamed to another id is not
        // live, so it lands in the non-live comparison below and recompiles.
        let run_chunk = |lua: &mut Lua, name: &str, src: &str| -> Result<(), LuaError> {
            let load = lua.try_enter(|ctx| {
                let closure = Closure::load(ctx, Some(name), src.as_bytes())?;
                Ok(ctx.stash(Executor::start(ctx, closure.into(), ())))
            });
            load.and_then(|ex| lua.execute::<()>(&ex))
                .map_err(|e| static_error_to_lua(e).in_file(name))
        };
        let songs: Vec<(&str, String)> = if self.source_dirty || self.program_sources.is_empty() {
            Vec::new()
        } else {
            let mut l = self.lua.borrow_mut();
            files
                .iter()
                .filter(|(n, _)| *n != AUDIO_MIX_FILE && *n != CONTROLS_FILE)
                .filter_map(|&(n, new)| {
                    let (_, old) = self.program_sources.iter().find(|(m, _)| m == n)?;
                    if old == new {
                        return None;
                    }
                    let id = song_id(&mut l, n, new)?;
                    (song_id(&mut l, n, old)? == id).then_some((n, id))
                })
                .collect()
        };
        let live = |n: &str| {
            n == AUDIO_MIX_FILE || n == CONTROLS_FILE || songs.iter().any(|(m, _)| *m == n)
        };
        let controls_present = |fs: &[(String, String)]| fs.iter().any(|(n, _)| n == CONTROLS_FILE);
        // `dma()` only works inside the init window, which the fast path
        // never reopens — ppuglobals.lua's top-level `dma(...)` lines (the
        // DMA panel's placements) take effect only via a full recompile,
        // where ppuglobals.lua runs FIRST inside that window. So a
        // controls-only push that changes those lines must recompile.
        let old_setup = self
            .program_sources
            .iter()
            .find(|(n, _)| n == CONTROLS_FILE)
            .map(|(_, s)| setup_text(s));
        let new_setup = files
            .iter()
            .find(|(n, _)| *n == CONTROLS_FILE)
            .map(|(_, s)| setup_text(s));
        if !self.source_dirty
            && !self.program_sources.is_empty()
            && files.iter().copied().ne(self
                .program_sources
                .iter()
                .map(|(n, s)| (n.as_str(), s.as_str())))
            && files.iter().filter(|(n, _)| !live(n)).copied().eq(self
                .program_sources
                .iter()
                .filter(|(n, _)| !live(n))
                .map(|(n, s)| (n.as_str(), s.as_str())))
            // A controls PRESENCE change (ppuglobals.lua added or removed
            // outright, not just its text edited) is a full recompile by
            // design — the fast path below only ever reloads ppuglobals.lua's
            // text in place in the live VM, never installs it fresh.
            && files.iter().any(|(n, _)| *n == CONTROLS_FILE)
                == controls_present(&self.program_sources)
            && old_setup == new_setup
        {
            'fast: {
                saved_mix
                    .validate_samples(&self.dsp_view().samples)
                    .map_err(|message| LuaError {
                        message,
                        line: None,
                        file: Some(AUDIO_MIX_FILE.into()),
                    })?;
                // Songs first, in two phases so a push reloads all its songs or
                // none. Each is re-run and prepared; one that can't reload in
                // place (no `song =` score plays it, or it names a sound its
                // score didn't place at setup: `dma()` needs the init window)
                // falls through to the full recompile below, before anything
                // was swapped in. The old VM keeps its old events either way;
                // only the re-run seq_<id> globals are new.
                let mut commits = Vec::new();
                for (name, id) in &songs {
                    let src = files
                        .iter()
                        .find(|(n, _)| n == name)
                        .map_or("", |(_, s)| *s);
                    let mut l = self.lua.borrow_mut();
                    run_chunk(&mut l, name, src)?;
                    let ex = l.enter(|ctx| match ctx.get_global("__score_prepare") {
                        Value::Function(f) => {
                            let id = ctx.intern(id.as_bytes());
                            ctx.stash(Executor::start(ctx, f, id))
                        }
                        _ => panic!("kit.lua must define __score_prepare"),
                    });
                    l.finish(&ex);
                    let prepared = l.try_enter(|ctx| {
                        Ok(match ctx.fetch(&ex).take_result::<Value>(ctx)?? {
                            Value::Function(f) => Some(ctx.stash(f)),
                            _ => None,
                        })
                    });
                    match prepared.map_err(|e| static_error_to_lua(e).in_file(name))? {
                        Some(commit) => commits.push(commit),
                        None => break 'fast,
                    }
                }
                for commit in commits {
                    let mut l = self.lua.borrow_mut();
                    let ex = l.enter(|ctx| {
                        let f = ctx.fetch(&commit);
                        ctx.stash(Executor::start(ctx, f, ()))
                    });
                    l.execute::<()>(&ex)
                        .expect("a prepared score reload only swaps tables in");
                }
                // A controls-only reload never recompiles: load the new text
                // into the SAME live VM (same `__ppu_controls_env`/log, same
                // frame/init/timers/DSP) and only touch `controls_fn` if the
                // text actually changed — an unrelated push (e.g. mix-only)
                // must leave the currently applying controls fn alone.
                if let Some((_, new_src)) = files.iter().find(|(n, _)| *n == CONTROLS_FILE) {
                    let old_src = self
                        .program_sources
                        .iter()
                        .find(|(n, _)| n == CONTROLS_FILE)
                        .map(|(_, s)| s.as_str());
                    if old_src != Some(*new_src) {
                        // The `dma(` lines are unchanged (checked above) and
                        // already in effect from the last recompile; reloading
                        // them here would hit the init-window gate, so they are
                        // blanked (line numbers kept) before the in-place load.
                        let mut l = self.lua.borrow_mut();
                        self.controls_fn =
                            Self::load_controls(&mut l, &without_dma_lines(new_src))?;
                        self.controls_vram = Self::read_controls_vram(&mut l);
                    }
                }
                self.saved_mix = saved_mix;
                self.program_sources = files
                    .iter()
                    .map(|(n, s)| ((*n).into(), (*s).into()))
                    .collect();
                return Ok(());
            }
        }
        let mut lua = Lua::core();
        let keys = Rc::new(lua.enter(StashedKeys::new));
        lua.enter(|ctx| install_bindings(ctx, &keys.fetch(ctx)));
        // Reseed voice[]/dsp from the LIVE registers (not reset) — a recompile
        // must not silence a sounding voice. See `seed_dsp_tables`.
        lua.enter(|ctx| seed_dsp_tables(ctx, &self.dsp, self.unmixed_dsp.as_ref()));
        let sram = self.sram_json.clone();
        lua.enter(|ctx| set_sram_table(ctx, &sram));
        // dma() records into a FRESH recorder, active for the init window
        // (top-level chunks + init()). A load error returns before the
        // recorder is swapped in; an error after the swap (init(), echo
        // re-check) restores the previous program's sample placements into
        // it — see `prev_samples` below — since nothing was written to ARAM.
        let rec = Rc::new(DmaRecorder::default());
        rec.active.set(true);
        {
            let (store, rec) = (self.source_store.clone(), rec.clone());
            lua.enter(move |ctx| install_dma(ctx, store, rec));
        }
        // Run once per fresh VM, right before the chunk loop: installs the
        // change-tracking `__ppu_controls_env` (a proxy of globals) and the
        // `__ppu_controls_begin/dirty/restore` hooks ppuglobals.lua's
        // `apply_pokes` runs under — see `controls_env.lua` and
        // `Self::load_controls`. Static and self-contained: a load/exec
        // failure here would be our own bug, not a user error.
        {
            let load = lua.try_enter(|ctx| {
                let closure =
                    Closure::load(ctx, Some("controls_env"), Self::CONTROLS_ENV_LUA.as_bytes())?;
                Ok(ctx.stash(Executor::start(ctx, closure.into(), ())))
            });
            load.and_then(|ex| lua.execute::<()>(&ex))
                .expect("controls_env.lua is static and must load/run cleanly");
        }

        run_chunk(&mut lua, "kit", Self::KIT_LUA)?;
        run_chunk(&mut lua, "ramps", include_str!("ramps.lua"))?;
        run_chunk(&mut lua, "vram_helpers", include_str!("vram_helpers.lua"))?;

        // ppuglobals.lua runs FIRST among the sketch's files, wherever it sits
        // in the list, so `markers`/`scanlines`/`apply_pokes` exist before
        // any user chunk's top level runs (a pasted `markers.intro` must
        // resolve). It compiles against the tracked env installed above, not
        // the plain globals table the other chunks share — see
        // `Self::load_controls`.
        let controls_fn = match files.iter().find(|(name, _)| *name == CONTROLS_FILE) {
            Some((_, src)) => Self::load_controls(&mut lua, src)?,
            None => None,
        };
        let controls_vram = match controls_fn {
            Some(_) => Self::read_controls_vram(&mut lua),
            None => Vec::new(),
        };

        for (name, src) in files
            .iter()
            .filter(|(name, _)| *name != AUDIO_MIX_FILE && *name != CONTROLS_FILE)
        {
            run_chunk(&mut lua, name, src)?;
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
        self.keys = keys;
        *self.mirror.borrow_mut() = Mirror::default();
        self.frame_fn = frame_fn;
        self.frame_file = frame_file;
        self.init_fn = init_fn;
        self.controls_fn = controls_fn;
        self.controls_vram = controls_vram;
        // Snapshot the OLD recorder's placements before swapping it out: if
        // this compile fails below, nothing new was actually written to
        // ARAM, so these are what ARAM still holds — restoring them (instead
        // of clearing) keeps `dsp_view()` truthful on the failure path.
        let prev_samples = self.dma.samples.borrow().clone();
        self.dma = rec.clone();
        self.memory = Memory::new();
        // The old stashed hooks belong to the dead VM just swapped out —
        // drop them now regardless of whether init() below succeeds.
        self.timers.clear();

        // Two setup-window calls, in order: the program's own `init()`, then
        // the controls document's `apply_setup()` (the panels' setup-only
        // calls — `midi{}`, and anything else that needs every user/data
        // chunk's globals to exist, which is why it can't run at
        // ppuglobals.lua's own top level). Both share one failure shape: the
        // init window closes, and nothing was written to ARAM yet (that
        // happens after the `?` below), so the OLD program's placements
        // (recorded above, before this compile's dma() calls) are restored —
        // a failed setup keeps reporting what ARAM still actually holds.
        let setup_fn = self
            .lua
            .borrow_mut()
            .enter(|ctx| match ctx.get_global("apply_setup") {
                Value::Function(f) => Some(ctx.stash(f)),
                _ => None,
            });
        let calls = [
            (self.init_fn.clone(), init_file.clone()),
            (setup_fn, Some(CONTROLS_FILE.to_string())),
        ];
        let mut res = Ok(());
        for (func, file) in calls {
            let Some(func) = func else { continue };
            let mut l = self.lua.borrow_mut();
            let ex = l.enter(|ctx| {
                let f = ctx.fetch(&func);
                ctx.stash(Executor::start(ctx, f, ()))
            });
            if let Err(e) = l.execute::<()>(&ex) {
                let mut err = static_error_to_lua(e);
                err.file = file;
                res = Err(err);
                break;
            }
        }
        rec.active.set(false); // init window closes even on error
        res.map_err(|e| {
            *rec.samples.borrow_mut() = prev_samples.clone();
            e
        })?;
        // Re-check the echo region here against the FINAL `dsp.echo.delay`
        // (init() may have changed it after a sample dma() call already
        // placed against an earlier value — the dma() arm only ever sees
        // the value at call time). Same lookup/clamp/message shape as that
        // arm's own check.
        let delay = self
            .lua
            .borrow_mut()
            .enter(|ctx| match ctx.get_global("dsp") {
                Value::Table(d) => match d.get(ctx, "echo") {
                    Value::Table(echo) => echo.get(ctx, "delay").to_int().unwrap_or(0),
                    _ => 0,
                },
                _ => 0,
            })
            .clamp(0, 15);
        let (echo_lo, echo_hi) = echo_region(delay);
        let overlap = rec
            .samples
            .borrow()
            .iter()
            .find(|sp| (sp.addr as u32) < echo_hi && echo_lo < sp.end)
            .map(|sp| sp.name.clone());
        if let Some(name) = overlap {
            // Same reasoning as the init() error path above: nothing was
            // written to ARAM by this compile, so the previous program's
            // placements are what ARAM still holds.
            *rec.samples.borrow_mut() = prev_samples.clone();
            return Err(LuaError {
                message: format!(
                    "dma: '{name}' overlaps the echo region (0x{echo_lo:04x}-0x{hi:04x}, \
                     dsp.echo.delay = {delay})",
                    hi = echo_hi - 1
                ),
                line: None,
                file: init_file.clone(),
            });
        }
        let mix_samples: Vec<_> = rec
            .samples
            .borrow()
            .iter()
            .map(|s| DspSampleView {
                id: s.id,
                name: s.name.clone(),
                start: s.addr as u32,
                end: s.end,
            })
            .collect();
        for mix in [&saved_mix, self.live_mix.as_ref().unwrap_or(&saved_mix)] {
            if let Err(message) = mix.validate_samples(&mix_samples) {
                *rec.samples.borrow_mut() = prev_samples.clone();
                return Err(LuaError {
                    message,
                    line: None,
                    file: Some(AUDIO_MIX_FILE.into()),
                });
            }
        }
        self.saved_mix = saved_mix;
        // Sample placements write ARAM + the sample directory ONCE here,
        // after init() has succeeded (or there is none) and passed the
        // final echo-region check above — never in frame(), and never
        // cleared first: DSP/ARAM survive recompiles by design, so an
        // earlier program's placed samples stay put underneath these.
        // The name was already validated against the store at `dma()` call
        // time, moments ago, so it's expected to still resolve.
        for sp in rec.samples.borrow().iter() {
            let payload = self
                .source_store
                .borrow()
                .get(&sp.name)
                .cloned()
                .or_else(|| crate::bank::get(&sp.name));
            if let Some(crate::source::SourcePayload::Sample(src)) = payload {
                let addr = sp.addr as usize;
                let end = sp.end as usize;
                self.aram[addr..end].copy_from_slice(&src.brr);
                let dir = SAMPLE_DIR as usize + 4 * sp.id as usize;
                self.aram[dir] = (sp.addr & 0xff) as u8;
                self.aram[dir + 1] = (sp.addr >> 8) as u8;
                self.aram[dir + 2] = (sp.loop_addr & 0xff) as u8;
                self.aram[dir + 3] = (sp.loop_addr >> 8) as u8;
            }
        }
        // Recompile == drop + re-register: a fresh registration always
        // starts one period from firing (`due_h = period_h`), so phase never
        // survives a recompile.
        self.timers = rec
            .timers
            .borrow_mut()
            .drain(..)
            .map(|(n, div, func, file)| {
                let period_h = div as u64 * if n == 2 { 1 } else { 8 };
                TimerHook {
                    period_h,
                    due_h: period_h,
                    func,
                    file,
                }
            })
            .collect();
        self.program_sources = files
            .iter()
            .map(|(name, source)| ((*name).to_string(), (*source).to_string()))
            .collect();
        self.source_dirty = false;
        Ok(())
    }

    /// Compile `ppuglobals.lua`'s source against the tracked
    /// `__ppu_controls_env` (installed by `controls_env.lua` — see
    /// `set_sources`'s fresh-VM setup and its fast path) and stash
    /// `apply_pokes` if the chunk defines one (`None` is treated exactly
    /// like no ppuglobals.lua at all). Errors are attributed to
    /// [`CONTROLS_FILE`].
    ///
    /// The whole load runs inside its own begin/restore bracket: a
    /// RUNTIME failure partway through the chunk has already executed some
    /// of its top-level writes straight onto the real globals (only logged
    /// for undo) — `__ppu_controls_restore` unwinds exactly those before the
    /// error returns, so a partially-applied controls text never leaves
    /// stray globals live in the VM. Before executing the chunk, any
    /// previously-stashed `apply_pokes` is nil'd out THROUGH the tracked env
    /// too, so a text that no longer defines one doesn't leave the old
    /// chunk's function re-stashed below, and that clear is itself undone by
    /// the same restore on a load failure. On success the bracket is
    /// re-begun (fresh, empty log) so nothing done here is later undone by
    /// `frame()`'s own bracket.
    fn load_controls(lua: &mut Lua, src: &str) -> Result<Option<StashedFunction>, LuaError> {
        call_controls_hook::<()>(lua, "__ppu_controls_begin");

        // Nil the previous apply_pokes/apply_setup THROUGH the tracked env
        // first, so a text that no longer defines one doesn't re-stash the old
        // chunk's function below — logged for undo like any other write, so
        // a load failure below puts the old one right back.
        let clear = lua.try_enter(|ctx| {
            let env = controls_env(ctx);
            let closure = Closure::load_with_env(
                ctx,
                None,
                "apply_pokes = nil; apply_vram = nil; apply_setup = nil".as_bytes(),
                env,
            )?;
            Ok(ctx.stash(Executor::start(ctx, closure.into(), ())))
        });
        clear
            .and_then(|ex| lua.execute::<()>(&ex))
            .unwrap_or_else(|e| panic!("clearing apply_pokes must not fail: {e}"));

        let load = lua.try_enter(|ctx| {
            let env = controls_env(ctx);
            let closure = Closure::load_with_env(ctx, Some(CONTROLS_FILE), src.as_bytes(), env)?;
            Ok(ctx.stash(Executor::start(ctx, closure.into(), ())))
        });
        if let Err(e) = load.and_then(|ex| lua.execute::<()>(&ex)) {
            call_controls_hook::<()>(lua, "__ppu_controls_restore");
            return Err(static_error_to_lua(e).in_file(CONTROLS_FILE));
        }
        call_controls_hook::<()>(lua, "__ppu_controls_begin");
        // Painted tiles: run `apply_vram()` once, capturing its `vr()` words
        // (see `__ppu_controls_capture_vram`); `read_controls_vram` lifts them
        // into Rust. Any other write it made is undone like a failed load's.
        let cap = lua.try_enter(|ctx| match ctx.get_global("__ppu_controls_capture_vram") {
            Value::Function(f) => Ok(ctx.stash(Executor::start(ctx, f, ()))),
            _ => panic!("controls_env.lua must define __ppu_controls_capture_vram"),
        });
        let captured = cap.and_then(|ex| lua.execute::<()>(&ex));
        call_controls_hook::<()>(lua, "__ppu_controls_restore");
        if let Err(e) = captured {
            return Err(static_error_to_lua(e).in_file(CONTROLS_FILE));
        }
        call_controls_hook::<()>(lua, "__ppu_controls_begin");
        Ok(lua.enter(|ctx| match ctx.get_global("apply_pokes") {
            Value::Function(f) => Some(ctx.stash(f)),
            _ => None,
        }))
    }

    /// The words the last `load_controls` captured from `apply_vram()`.
    fn read_controls_vram(lua: &mut Lua) -> Vec<(u16, u16)> {
        lua.enter(|ctx| {
            let mut out = Vec::new();
            if let Value::Table(t) = ctx.get_global("__ppu_controls_vram") {
                for (k, v) in t {
                    if let (Some(a), Some(w)) = (k.to_int(), v.to_int()) {
                        if (0..0x8000).contains(&a) {
                            out.push((a as u16, w as u16));
                        }
                    }
                }
            }
            out
        })
    }

    /// Recompile the cached program sources (`frame()`'s dirty-source path and `reset()`).
    fn recompile(&mut self) -> Result<(), LuaError> {
        let files = self.program_sources.clone();
        let refs: Vec<_> = files
            .iter()
            .map(|(n, s)| (n.as_str(), s.as_str()))
            .collect();
        self.set_sources(&refs)
    }

    /// Run (t=0): a full power cycle of the sound chip. Fresh `Dsp`, zeroed
    /// ARAM (echo RAM included, since it's just the top of ARAM), then the
    /// same recompile path `frame()` uses for a dirty source list — so
    /// `dma()` placements are re-written into the freshly-zeroed ARAM and
    /// `timer()` hooks re-register at phase zero, exactly like a fresh
    /// engine loaded with the same program. Clearing `dma.samples` first
    /// keeps `dsp_view()` truthful if that recompile itself fails (same
    /// contract as `set_sources`'s own failure path). Contrast `set_sources`
    /// (recompile): that keeps the DSP/ARAM/timer phase running — it never
    /// resets anything.
    pub fn reset(&mut self) -> Result<(), LuaError> {
        self.unmixed_dsp = None;
        self.mix_kon = 0;
        self.mix_koff = 0;
        self.dsp = power_on_dsp();
        self.aram.fill(0);
        self.audio.clear();
        self.audio_acc = 0;
        self.dma.samples.borrow_mut().clear();
        self.recompile()
    }

    /// Run one frame: call `frame(t,f)` once (bare assigns -> frame-wide defaults,
    /// `hdma` -> registered hooks), read CGRAM/OAM, then resolve the 224-row
    /// LineTable by applying each covering hook per scanline (later call wins).
    /// Set the controller bitmask read by the next frame() (see PAD_NAMES for
    /// the bit order). Sticky: a held button stays held until cleared.
    pub fn set_pad(&mut self, mask: u16) {
        self.pad = mask;
    }

    /// Bind the `sram` global to a save blob (JSON object or array). Lands in
    /// the live VM now and seeds every later recompile, so `init()` sees it.
    /// Anything else (invalid JSON, a scalar) resets to an empty table.
    pub fn set_sram(&mut self, json: &str) {
        let v: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
        self.sram_json = if v.is_object() || v.is_array() {
            v.to_string()
        } else {
            "{}".to_string()
        };
        self.sram_dirty = false;
        let json = self.sram_json.clone();
        self.lua
            .borrow_mut()
            .enter(|ctx| set_sram_table(ctx, &json));
    }

    /// JSON of `sram` if a frame changed it since the last take, else `None`.
    pub fn take_sram(&mut self) -> Option<String> {
        if !self.sram_dirty {
            return None;
        }
        self.sram_dirty = false;
        Some(self.sram_json.clone())
    }

    /// Undo the controls log if a controls chunk is loaded: every error
    /// exit from `frame()` after `__ppu_controls_begin` must call this
    /// before returning, so a partial frame (e.g. an explicit
    /// `apply_pokes()` call the program made before erroring) never bakes
    /// into the next frame's baseline.
    fn restore_controls(&self) {
        if self.controls_fn.is_some() {
            let mut l = self.lua.borrow_mut();
            call_controls_hook::<()>(&mut l, "__ppu_controls_restore");
        }
    }

    pub fn frame(&mut self, t: f64, f: u32) -> Result<LineTable, LuaError> {
        if self.source_dirty {
            self.recompile()?;
        }
        // Reset the per-frame hook registry, then run frame(t,f) once.
        {
            let mut l = self.lua.borrow_mut();
            let pad = self.pad;
            l.enter(|ctx| {
                ctx.set_global("__ppu_hooks", Table::new(&ctx)).unwrap();
                set_pad_table(ctx, pad);
            });
            // Begin the controls undo bracket BEFORE the program's own
            // frame() runs, not after — an explicit `apply_pokes()` call
            // from inside frame() (or a timer hook) runs under the tracked
            // env too, so its writes must be logged from the start or they
            // escape undoing entirely. One begin() now covers the whole
            // frame — Phase A no longer calls it, and the single restore()
            // at the end of this function undoes everything logged since.
            if self.controls_fn.is_some() {
                call_controls_hook::<()>(&mut l, "__ppu_controls_begin");
            }
            if let Some(frame) = self.frame_fn.clone() {
                let ex = l.enter(|ctx| {
                    let func = ctx.fetch(&frame);
                    ctx.stash(Executor::start(ctx, func, (t, f as i64)))
                });
                if let Err(e) = l.execute::<()>(&ex) {
                    // Restore here too (`l` is already borrowed, so not via
                    // `restore_controls`) — an explicit `apply_pokes()`
                    // call made before this error would otherwise stay
                    // logged into the next frame's stale bracket.
                    if self.controls_fn.is_some() {
                        call_controls_hook::<()>(&mut l, "__ppu_controls_restore");
                    }
                    let mut err = static_error_to_lua(e);
                    err.file = self.frame_file.clone();
                    return Err(err);
                }
            }
        }

        // Read frame-wide defaults + frame-global memory. VRAM is rebuilt fresh
        // each frame so the flush order is deterministic: zero -> dma replay
        // (init placements, call order) -> structured pokes (read_memory) ->
        // raw vram[] (final authority).
        let defaults = {
            let mut l = self.lua.borrow_mut();
            let keys = &self.keys;
            let mut mirror = self.mirror.borrow_mut();
            l.enter(|ctx| {
                self.memory.vram = [0u16; 0x8000];
                self.memory.cgram = [0u16; 256];
                self.reports.clear();
                replay_dma(
                    &self.dma.placements.borrow(),
                    &self.source_store.borrow(),
                    &mut self.reports,
                    &mut self.memory,
                );
                read_memory(ctx, &mut self.memory);
                read_state(ctx, &keys.fetch(ctx), &mut mirror)
            })
        };

        // M12/audio: walk this frame's audio span sample-accurately against
        // the registered `timer(n, div, fn)` hooks — BEFORE the hdma hooks
        // below are even collected, so timers and hdma interleave in a
        // fixed order within one frame (timers, then hdma) even though both
        // may touch the same globals (see `render_frame_audio`). Consequence
        // (Spec-locked, not to be reordered): an hdma hook's `kon`/`koff`/
        // `voice[]`/`dsp` writes are flushed to the DSP only at the NEXT
        // frame's offset-0 flush, one frame later than a `frame()`-body or
        // timer-hook write — see the `kon`/`koff` callback comment below.
        self.render_frame_audio()
            .inspect_err(|_| self.restore_controls())?;

        // Controls Phase A (PPU-147): run `apply_pokes()` frame-wide, once,
        // right after the program's own frame()/audio and BEFORE hooks are
        // collected below, so any `hdma()` it registers joins the program's
        // own. `n_program` counts only FUNCTION-bearing `__ppu_hooks`
        // entries — the same predicate the collector below applies, so a
        // program hook missing its function argument doesn't desync the two
        // — and is the splice index for the synthetic whole-frame hook
        // below: after the program's own hooks, before controls's own. The
        // undo bracket (begun at the top of this function) covers every write made here, undone by
        // the single restore() at the end of this function, so an override
        // never bakes into the program's own baseline. That undo also
        // covers `voice[]`/`dsp` table writes, so those never reach the
        // NEXT frame's `flush_dsp_writes` — dropped by design (visual
        // override pass only); `kon()`/`koff()` calls are NOT tracked (they
        // set `__dsp_kon`/`__dsp_koff` from Rust) and do fire next frame.
        // The second `read_memory` below overlays controls's OAM/VRAM/CGRAM
        // pokes on the program's, frame-global only: a band-scoped `vram[]`
        // /`obj[]` poke inside an `hdma` closure never reaches `memory()`
        // (band cgram does, via `take_cgram_pokes`).
        let n_program: usize = if let Some(cf) = self.controls_fn.clone() {
            let n_program = {
                let mut l = self.lua.borrow_mut();
                l.enter(|ctx| match ctx.get_global("__ppu_hooks") {
                    Value::Table(t) => (1..=t.length())
                        .filter(|&idx| {
                            matches!(
                                t.get(ctx, idx),
                                Value::Table(e) if matches!(e.get(ctx, 3), Value::Function(_))
                            )
                        })
                        .count(),
                    _ => 0,
                })
            };
            let res = {
                let mut l = self.lua.borrow_mut();
                let ex = l.enter(|ctx| {
                    let f = ctx.fetch(&cf);
                    ctx.stash(Executor::start(ctx, f, ()))
                });
                l.execute::<()>(&ex)
            };
            if let Err(e) = res {
                let mut l = self.lua.borrow_mut();
                call_controls_hook::<()>(&mut l, "__ppu_controls_restore");
                let mut err = static_error_to_lua(e);
                err.file = Some(CONTROLS_FILE.to_string());
                return Err(err);
            }
            let mut l = self.lua.borrow_mut();
            l.enter(|ctx| read_memory(ctx, &mut self.memory));
            n_program
        } else {
            0
        };
        // Painted tiles, last of all: over the program's and apply_pokes()'s
        // own VRAM writes. `read_memory` rebuilt VRAM this frame, so a word
        // that left the list is simply not re-applied.
        for &(addr, word) in &self.controls_vram {
            self.memory.vram[addr as usize] = word;
        }

        // Collect registered hooks (stash each fn with its [y0,y1]), then
        // clear the registry: nothing reads it again this frame, and
        // `hdma()` already no-ops when this global isn't a table; the next
        // frame() recreates it.
        let mut hooks: Vec<(usize, usize, StashedFunction, Option<String>)> = {
            let mut l = self.lua.borrow_mut();
            l.enter(|ctx| {
                let mut out = Vec::new();
                if let Value::Table(hk) = ctx.get_global("__ppu_hooks") {
                    let n = hk.length();
                    for idx in 1..=n {
                        if let Value::Table(entry) = hk.get(ctx, idx) {
                            let y0 = entry.get(ctx, 1).to_int().unwrap_or(0).max(0) as usize;
                            let y1 = entry.get(ctx, 2).to_int().unwrap_or(0).max(0) as usize;
                            if let Value::Function(func) = entry.get(ctx, 3) {
                                let file = function_chunk_name(&func);
                                out.push((y0, y1, ctx.stash(func), file));
                            }
                        }
                    }
                }
                ctx.set_global("__ppu_hooks", Value::Nil).unwrap();
                out
            })
        };
        // Splice the whole-frame controls override in between the program's
        // hooks and controls's own (hooks[n_program..], registered by
        // apply_pokes above, if any): it reuses the exact per-hook closure
        // below (write_state(row)/run/read_state), so a property controls
        // doesn't touch keeps whatever the program's hooks resolved for that
        // row, and a controls hdma hook composes on top last-write-wins, same
        // as any other hook. The hook is `__ppu_controls_replay`, which
        // re-applies the register writes Phase A logged (frozen here), not
        // apply_pokes itself: re-executing apply_pokes 224x/frame made every
        // frame-wide poke cost 224 tracked writes, and a tilemap paint's
        // thousands of `vram[]` pokes (which no row reads) dominated the
        // frame. Skipped when apply_pokes wrote no register this frame.
        if self.controls_fn.is_some() {
            let replay: Option<StashedFunction> = {
                let mut l = self.lua.borrow_mut();
                let dirty: bool = call_controls_hook(&mut l, "__ppu_controls_freeze");
                dirty.then(|| {
                    l.enter(|ctx| match ctx.get_global("__ppu_controls_replay") {
                        Value::Function(f) => ctx.stash(f),
                        _ => panic!("controls_env.lua must define __ppu_controls_replay"),
                    })
                })
            };
            if let Some(replay) = replay {
                // `.min(hooks.len())`: n_program matches hooks.len() in the
                // common case (see the Phase A comment above), but a
                // hand-edited apply_pokes assigning `__ppu_hooks = {}` can
                // shrink it out from under that count — clamp so the splice
                // never goes out of range.
                hooks.insert(
                    n_program.min(hooks.len()),
                    (0, HEIGHT - 1, replay, Some(CONTROLS_FILE.to_string())),
                );
            }
        }

        // The frame-wide `cgram` table, so a hook's palette write can be told
        // apart, recorded on its line, and put back (HDMA to CGRAM). Taken
        // AFTER Phase A, so a frame-wide controls cgram poke is already
        // baked into the baseline here — per-row hooks (incl. the synthetic
        // one above) won't re-report it as a per-row poke on top.
        let cg_snap: Rc<[Option<i64>; 256]> = {
            let mut l = self.lua.borrow_mut();
            Rc::new(l.enter(|ctx| snapshot_cgram(ctx, &self.keys.fetch(ctx))))
        };
        // Everything since the `defaults` read wrote globals behind the
        // mirror's back (the program's frame(), timer hooks, apply_pokes, the
        // controls bracket): forget it, so the first per-row write_state is
        // a full write. Inside the loop only hooks write, and each is
        // followed by the read_state that refreshes the mirror.
        *self.mirror.borrow_mut() = Mirror::default();
        // Resolve the line table: each hook becomes a closure that re-baselines
        // globals to the working row, runs fn(y), and reads the row back.
        let err_sink: Rc<RefCell<Option<LuaError>>> = Rc::new(RefCell::new(None));
        let mut builder = LineTableBuilder::new(defaults.clone());
        for (y0, y1, sf, file) in hooks {
            let lua = self.lua.clone();
            let keys = self.keys.clone();
            let mirror = self.mirror.clone();
            let sink = err_sink.clone();
            let snap = cg_snap.clone();
            builder.hdma(y0, y1, move |y, row| {
                if sink.borrow().is_some() {
                    return;
                }
                let mut l = lua.borrow_mut();
                l.enter(|ctx| write_state(ctx, &keys.fetch(ctx), &mut mirror.borrow_mut(), row));
                let ex = l.enter(|ctx| {
                    let func = ctx.fetch(&sf);
                    ctx.stash(Executor::start(ctx, func, (y as i64,)))
                });
                match l.execute::<()>(&ex) {
                    Ok(()) => {
                        let mut pokes = std::mem::take(&mut row.cgram);
                        *row = l.enter(|ctx| {
                            read_state(ctx, &keys.fetch(ctx), &mut mirror.borrow_mut())
                        });
                        pokes.extend(l.enter(|ctx| take_cgram_pokes(ctx, &keys.fetch(ctx), &snap)));
                        row.cgram = pokes;
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

        // Undo every write controls made this frame — including an explicit
        // `apply_pokes()` call from inside the program's own frame(),
        // Phase A's implicit call, and any per-row replay — now that the
        // LineTable/Memory have captured them: the program's own globals
        // return to exactly where its own frame() left them, so a released
        // override never bakes in and an accumulator never compounds. See
        // controls_env.lua. Runs BEFORE the sticky-globals restore below:
        // a controls BAND hook's first logged write for a key holds
        // that row's `write_state(row)` baseline as its "old" value, not the
        // frame-wide default — restoring here first, then re-baselining to
        // `defaults` below, makes the frame-wide write_state the one that
        // sticks instead of a stray row value.
        self.restore_controls();

        // Restore sticky globals to the frame-wide defaults (hooks mutated
        // them; the controls restore above and any erroring hook wrote
        // globals behind the mirror, so this is a full write).
        {
            let mut l = self.lua.borrow_mut();
            let keys = &self.keys;
            let mut mirror = self.mirror.borrow_mut();
            *mirror = Mirror::default();
            l.enter(|ctx| write_state(ctx, &keys.fetch(ctx), &mut mirror, &defaults));
        }

        if let Some(e) = err_sink.borrow_mut().take() {
            return Err(e);
        }

        // Mirror `sram` back to JSON; a change is flagged for the host to persist.
        let json = self
            .lua
            .borrow_mut()
            .enter(|ctx| sram_to_json(ctx.get_global("sram")))
            .map_err(|message| LuaError {
                message,
                line: None,
                file: self.frame_file.clone(),
            })?;
        if json != self.sram_json {
            self.sram_json = json;
            self.sram_dirty = true;
        }
        Ok(lt)
    }

    /// Replace live adjustments without recompiling the song. None restores the saved mix.
    pub fn set_audio_mix(&mut self, json: Option<&str>) -> Result<(), String> {
        let mix = json.map(AudioMix::parse).transpose()?;
        mix.as_ref()
            .unwrap_or(&self.saved_mix)
            .validate_samples(&self.dsp_view().samples)?;
        self.live_mix = mix;
        Ok(())
    }

    pub fn audio_mix(&self) -> &AudioMix {
        self.live_mix.as_ref().unwrap_or(&self.saved_mix)
    }

    pub fn trigger_voice(&mut self, voice: u8, release: bool) -> Result<(), String> {
        if voice >= 8 {
            return Err("Voice must be 0–7".into());
        }
        if release {
            self.mix_koff |= 1 << voice;
        } else {
            self.mix_kon |= 1 << voice;
        }
        Ok(())
    }

    /// Drain `aram[]`, write `voice[]`/`dsp` to the DSP registers wholesale,
    /// and flush KOF then KON (KON LAST, so a koff+kon in the same span
    /// restarts the voice — matches the Dsp's KON-deferred-to-next-render-
    /// tick contract). One atomic unit applied by `render_frame_audio` at
    /// segment offset 0 and again after every timer hook, so a hook's
    /// `voice[]`/`dsp`/`kon`/`koff` writes land exactly at its sample offset.
    fn flush_dsp_writes(&mut self) {
        let mut l = self.lua.borrow_mut();
        l.enter(|ctx| {
            // 1. Drain aram[] pokes. A poke lands ONCE — the table is
            // never rebuilt from `self.aram`, so the echo unit's own
            // writes (and anything else living in ARAM) persist across
            // frames exactly like real hardware.
            if let Value::Table(a) = ctx.get_global("aram") {
                let mut keys = Vec::new();
                for (k, v) in a {
                    if let Some(addr) = k.to_int() {
                        if let Some(byte) = v.to_int() {
                            if (0..=0xffff).contains(&addr) {
                                self.aram[addr as usize] = (byte & 0xff) as u8;
                            }
                        }
                        keys.push(k);
                    }
                }
                for k in keys {
                    a.set(ctx, k, Value::Nil).unwrap();
                }
            }

            // 2. voice[]/dsp -> DSP registers, wholesale and idempotent.
            // Never touches ENDX (0x7c) or the read-only ENVX/OUTX (n8/n9).
            write_dsp_regs(ctx, &mut self.dsp);
            self.unmixed_dsp = Some(decode_dsp_view(&self.dsp));
            self.live_mix
                .as_ref()
                .unwrap_or(&self.saved_mix)
                .apply(&mut self.dsp);

            // 3. Flush KOF then KON.
            let kof = ctx.get_global("__dsp_koff").to_int().unwrap_or(0) as u8;
            self.dsp.write(0x5c, kof | self.mix_koff);
            self.mix_koff = 0;
            ctx.set_global("__dsp_koff", 0).unwrap();
            let kon = ctx.get_global("__dsp_kon").to_int().unwrap_or(0) as u8;
            self.dsp.write(0x4c, kon | self.mix_kon);
            self.mix_kon = 0;
            ctx.set_global("__dsp_kon", 0).unwrap();
        });
    }

    /// Render this frame's audio span sample-accurately against the
    /// registered `timer(n, div, fn)` hooks: flush `voice[]`/`dsp`/`aram[]`/
    /// KON/KOFF at offset 0, then walk expiry to expiry — render the DSP up
    /// to the next due hook (across all three timers, earliest `due_h`
    /// first, ties in registration order), call it as `fn(off)` where `off`
    /// is the sample offset within this span, flush again so its writes take
    /// effect from that offset on, and repeat until no hook is due within
    /// this span, then render the tail. Timer phase (`due_h`, tracked in
    /// half-samples at 32 kHz — see `TimerHook`) carries across frames
    /// unconditionally, including on the error path. Runs once per
    /// `frame()`, right after `defaults`/`read_state` and before the hdma
    /// hooks are collected from `__ppu_hooks` — timers always run before
    /// hdma within the same frame. Republishes `voice[n].envx/.outx/.ended`
    /// for the next `frame()`/hook to read only once the whole span rendered
    /// cleanly; a timer hook error aborts the walk early and leaves this
    /// frame's rendered prefix plus a stale (previous-frame) tail in
    /// `self.audio` — see [`Self::audio`].
    fn render_frame_audio(&mut self) -> Result<(), LuaError> {
        self.flush_dsp_writes();

        // Exact 32000/60.0988 accumulator: 320_000_000 / 600_988 ==
        // 32000 / 60.0988, so `n` alternates 532/533 and drifts by less
        // than one sample over any run length.
        self.audio_acc += 320_000_000;
        let n = (self.audio_acc / 600_988) as usize;
        self.audio_acc %= 600_988;
        self.audio.resize(n * 2, 0);

        let span_h = 2 * n as u64;
        let mut cursor = 0usize;
        let mut err: Option<LuaError> = None;

        loop {
            let pick = self
                .timers
                .iter()
                .enumerate()
                .filter(|(_, h)| h.due_h < span_h)
                .min_by_key(|(i, h)| (h.due_h, *i))
                .map(|(i, _)| i);
            let Some(i) = pick else { break };

            let off = (self.timers[i].due_h / 2) as usize;
            if off > cursor {
                self.dsp
                    .render(&mut self.aram, &mut self.audio[2 * cursor..2 * off]);
            }

            let func = self.timers[i].func.clone();
            let res = {
                let mut l = self.lua.borrow_mut();
                let ex = l.enter(|ctx| {
                    let f = ctx.fetch(&func);
                    ctx.stash(Executor::start(ctx, f, (off as i64,)))
                });
                l.execute::<()>(&ex)
            };
            if let Err(e) = res {
                let mut e = static_error_to_lua(e);
                e.file = self.timers[i].file.clone();
                err = Some(e);
            }

            self.flush_dsp_writes();
            self.timers[i].due_h += self.timers[i].period_h;
            cursor = off;
            if err.is_some() {
                break;
            }
        }

        if err.is_none() {
            self.dsp
                .render(&mut self.aram, &mut self.audio[2 * cursor..]);
        }

        // An error stops the walker from calling any MORE hooks (matching
        // the hdma sink's short-circuit), but time keeps passing: fast-
        // forward every timer's phase past this span without invoking them,
        // so the unconditional carry below never underflows and next
        // frame's due_h is exactly where it would be had the hooks run.
        if err.is_some() {
            for h in self.timers.iter_mut() {
                while h.due_h < span_h {
                    h.due_h += h.period_h;
                }
            }
        }

        // Phase carries across frames — unconditionally, even when a hook
        // above errored.
        for h in self.timers.iter_mut() {
            h.due_h -= span_h;
        }

        if let Some(e) = err {
            return Err(e);
        }

        // Republish envx/outx/ended so the NEXT frame() observes this
        // frame's envelope/output/end-of-sample state.
        let view = decode_dsp_view(&self.dsp);
        let mut l = self.lua.borrow_mut();
        l.enter(|ctx| publish_voice_readbacks(ctx, &view));
        Ok(())
    }
}

/// The `__ppu_controls_env` table controls_env.lua installs, looked up
/// fresh per `enter`/`try_enter` call (its GC lifetime is tied to that
/// call's `ctx`, so the `Table` itself can't be hoisted across calls — only
/// this lookup can).
fn controls_env<'gc>(ctx: piccolo::Context<'gc>) -> Table<'gc> {
    match ctx.get_global("__ppu_controls_env") {
        Value::Table(t) => t,
        _ => panic!("controls_env.lua must define __ppu_controls_env"),
    }
}

/// Call a zero-arg global Lua function defined by `controls_env.lua`
/// (`__ppu_controls_begin`/`_freeze`/`_restore`) for its side effect and/or
/// return value (`R`, e.g. `()` for begin/restore or `bool` for freeze).
/// That chunk is static and shipped with the engine, so a missing global or
/// a raise here is our own bug, not a user error — panics loudly (with the
/// Lua error) instead of threading a `Result` a caller could never
/// meaningfully recover from.
fn call_controls_hook<R: for<'gc> FromMultiValue<'gc>>(lua: &mut Lua, name: &'static str) -> R {
    let ex = lua.enter(|ctx| {
        let f = match ctx.get_global(name) {
            Value::Function(f) => f,
            _ => panic!("controls_env.lua must define {name}"),
        };
        ctx.stash(Executor::start(ctx, f, ()))
    });
    lua.execute::<R>(&ex)
        .unwrap_or_else(|e| panic!("{name} (controls_env.lua) must not raise: {e}"))
}

/// The `<id>` of a song file, or None. A song file defines `seq_<id>` and
/// nothing else at top level. The check runs the chunk in an EMPTY
/// environment (no globals to read, call or write through), so it can have
/// no side effects. The chunk counts as a song file when it completes and
/// leaves exactly one global: `seq_<id>`, a function. Top-level `local`s are
/// accepted, since they can reach nothing either. Anything else, including a
/// parse error or a top-level call, is not a song file, so the push takes the
/// full recompile.
fn song_id(lua: &mut Lua, name: &str, src: &str) -> Option<String> {
    let (env, ex) = lua
        .try_enter(|ctx| {
            let env = Table::new(&ctx);
            let closure = Closure::load_with_env(ctx, Some(name), src.as_bytes(), env)?;
            Ok((
                ctx.stash(env),
                ctx.stash(Executor::start(ctx, closure.into(), ())),
            ))
        })
        .ok()?;
    lua.execute::<()>(&ex).ok()?;
    lua.enter(|ctx| {
        let env = ctx.fetch(&env);
        let mut entries = env.iter();
        let (Value::String(k), Value::Function(_)) = entries.next()? else {
            return None;
        };
        let id = k.to_str().ok()?.strip_prefix("seq_")?;
        (entries.next().is_none() && !id.is_empty()).then(|| id.to_owned())
    })
}

/// The setup-only part of a controls document, in order — what the fast
/// path cannot reload in place: the top-level `dma(` calls, plus the whole
/// `function apply_setup()` … column-0 `end` block (only ever CALLED on a
/// recompile, after `init()`). A change to any of it forces a recompile.
fn setup_text(src: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut in_setup = false;
    for line in src.lines() {
        let t = line.trim_start();
        if in_setup {
            out.push(line);
            if line == "end" {
                in_setup = false;
            }
        } else if t.starts_with("dma(") {
            out.push(t);
        } else if t.starts_with("function apply_setup(") {
            in_setup = true;
            out.push(line);
        }
    }
    out
}

/// `src` with every `dma(` line blanked, line count preserved.
fn without_dma_lines(src: &str) -> String {
    src.lines()
        .map(|l| {
            if l.trim_start().starts_with("dma(") {
                ""
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Publish `json` (a serde-normalized object/array) as the `sram` global.
fn set_sram_table(ctx: piccolo::Context<'_>, json: &str) {
    let v: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
    ctx.set_global("sram", json_to_lua(ctx, &v)).unwrap();
}

fn json_to_lua<'gc>(ctx: piccolo::Context<'gc>, v: &serde_json::Value) -> Value<'gc> {
    use serde_json::Value as J;
    match v {
        J::Null => Value::Nil,
        J::Bool(b) => Value::Boolean(*b),
        J::Number(n) => match n.as_i64() {
            Some(i) => Value::Integer(i),
            None => Value::Number(n.as_f64().unwrap_or(0.0)),
        },
        J::String(s) => Value::String(ctx.intern(s.as_bytes())),
        J::Array(items) => {
            let t = Table::new(&ctx);
            for (i, item) in items.iter().enumerate() {
                t.set(ctx, i as i64 + 1, json_to_lua(ctx, item)).unwrap();
            }
            Value::Table(t)
        }
        J::Object(map) => {
            let t = Table::new(&ctx);
            for (k, item) in map {
                t.set(ctx, ctx.intern(k.as_bytes()), json_to_lua(ctx, item))
                    .unwrap();
            }
            Value::Table(t)
        }
    }
}

/// Serialize the `sram` global for the host. Errors name the offending path:
/// the table must be JSON-shaped (string keys or a dense 1..n array; numbers,
/// strings, booleans, tables only) and fit in `SRAM_MAX_BYTES`.
fn sram_to_json(root: Value<'_>) -> Result<String, String> {
    if !matches!(root, Value::Table(_)) {
        return Err(format!("sram must be a table, got {}", root.type_name()));
    }
    let json = lua_to_json(root, "sram", 0)?.to_string();
    if json.len() > SRAM_MAX_BYTES {
        return Err(format!(
            "sram is {} bytes; the cartridge holds {}",
            json.len(),
            SRAM_MAX_BYTES
        ));
    }
    Ok(json)
}

fn lua_to_json(v: Value<'_>, path: &str, depth: usize) -> Result<serde_json::Value, String> {
    use serde_json::Value as J;
    Ok(match v {
        Value::Nil => J::Null,
        Value::Boolean(b) => J::Bool(b),
        Value::Integer(i) => J::from(i),
        Value::Number(n) => serde_json::Number::from_f64(n)
            .map(J::Number)
            .ok_or_else(|| format!("{path} is not a finite number"))?,
        Value::String(s) => J::String(
            s.to_str()
                .map_err(|_| format!("{path} is not valid UTF-8"))?
                .to_string(),
        ),
        Value::Table(t) => {
            if depth >= SRAM_MAX_DEPTH {
                return Err(format!("{path} nests too deep (or is cyclic)"));
            }
            let n = t.length();
            let mut arr = Vec::new();
            let mut obj = serde_json::Map::new();
            for (k, item) in t.iter() {
                match k {
                    Value::Integer(i) if (1..=n).contains(&i) => arr.push((i, item)),
                    Value::String(s) => {
                        let key = s
                            .to_str()
                            .map_err(|_| format!("{path} has a non-UTF-8 key"))?
                            .to_string();
                        let item = lua_to_json(item, &format!("{path}.{key}"), depth + 1)?;
                        obj.insert(key, item);
                    }
                    other => {
                        return Err(format!(
                            "{path} has a {} key; sram keys must be strings or 1..n",
                            other.type_name()
                        ))
                    }
                }
            }
            if !arr.is_empty() && !obj.is_empty() {
                return Err(format!("{path} mixes array and string keys"));
            }
            if arr.is_empty() {
                J::Object(obj)
            } else {
                arr.sort_by_key(|(i, _)| *i);
                let mut out = Vec::with_capacity(arr.len());
                for (i, item) in arr {
                    out.push(lua_to_json(item, &format!("{path}[{i}]"), depth + 1)?);
                }
                J::Array(out)
            }
        }
        other => {
            return Err(format!(
                "{path} is a {}; sram holds only numbers, strings, booleans and tables",
                other.type_name()
            ))
        }
    })
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

/// Bit order of the controller mask, low bit first — the same order the web
/// side's `PAD` constants use. `pad.<name>` is a boolean per bit.
pub const PAD_NAMES: [&str; 12] = [
    "up", "down", "left", "right", "a", "b", "x", "y", "l", "r", "start", "select",
];

/// Publish the controller mask as the `pad` global: one boolean per button.
fn set_pad_table(ctx: piccolo::Context<'_>, mask: u16) {
    let pad = Table::new(&ctx);
    for (bit, name) in PAD_NAMES.iter().enumerate() {
        pad.set(ctx, *name, mask & (1 << bit) != 0).unwrap();
    }
    ctx.set_global("pad", pad).unwrap();
}

fn install_bindings<'gc>(ctx: piccolo::Context<'gc>, k: &Keys<'gc>) {
    // Throwaway: an empty mirror makes every sync_* write unconditional; the engine's own mirror starts empty too.
    let m = &mut Mirror::default();
    // controller: all released until the host sets a mask; init() may read it.
    set_pad_table(ctx, 0);
    // battery-backed save data; set_sources overwrites it from the host blob.
    ctx.set_global("sram", Table::new(&ctx)).unwrap();
    // scalar registers
    ctx.set_global("mode", 1).unwrap();
    ctx.set_global("bg3_priority", false).unwrap();
    ctx.set_global("brightness", 15).unwrap();
    // MOSAIC ($2106): global block size 0..15; per-layer enable via bg[n].mosaic.
    ctx.set_global("mosaic", 0).unwrap();
    ctx.set_global("direct_color", false).unwrap();
    ctx.set_global("force_blank", false).unwrap();
    // TM/TS main/sub screen designation ($212C/$212D). Both screens start
    // EMPTY (authentic power-on): a layer draws only once you designate it,
    // so a layer you never set up can never leak whatever VRAM holds.
    ctx.set_global("TM", 0x00).unwrap();
    ctx.set_global("TS", 0x00).unwrap();

    // Window-mask registers ($2123-$212F). Power-on: all zero -> no window
    // enabled, no layer clipped (existing goldens unaffected).
    for name in [
        "WH0", "WH1", "WH2", "WH3", "W12SEL", "W34SEL", "WOBJSEL", "WBGLOG", "WOBJLOG", "TMW",
        "TSW", "CGWSEL", "CGADSUB", "COLDATA",
    ] {
        ctx.set_global(name, 0).unwrap();
    }

    // bg[1..4] = { scroll = {x,y}, visible=true }
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
        let tile = stack.get(0).to_int().unwrap_or(0);
        let x = stack.get(1).to_int().unwrap_or(0);
        let y = stack.get(2).to_int().unwrap_or(0);
        let idx = stack.get(3).to_int().unwrap_or(0);
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

    // obj[0..127]
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

    // mode7_transform / mode7_floor: pure "view" helpers — callers assign the
    // returned fields (so generated ppuglobals.lua writes stay undo-tracked).
    // Native, not Lua: the floor view is called once per scanline (224x a
    // frame), where an interpreted body plus six argument checks cost ~2 ms
    // native and several times that under WASM.
    fn finite<'gc>(
        ctx: piccolo::Context<'gc>,
        v: Value<'gc>,
        name: &str,
    ) -> Result<f64, piccolo::Error<'gc>> {
        match v {
            Value::Integer(_) | Value::Number(_) => match v.to_number() {
                Some(n) if n.is_finite() => Ok(n),
                _ => Err(lua_err(ctx, &format!("{name} must be a finite number"))),
            },
            _ => Err(lua_err(ctx, &format!("{name} must be a finite number"))),
        }
    }
    /// Degrees + texture position -> (cos, sin, wrapped cx, wrapped cy).
    fn pose(angle: f64, x: f64, z: f64) -> (f64, f64, i64, i64) {
        let r = angle * std::f64::consts::PI / 180.0;
        let wrap = |v: f64| ((v + 0.5).floor() as i64).rem_euclid(1024);
        (r.cos(), r.sin(), wrap(x), wrap(z))
    }
    fn view<'gc>(
        ctx: piccolo::Context<'gc>,
        m: [f64; 4],
        cx: i64,
        cy: i64,
        scroll_y: i64,
        visible: bool,
    ) -> Table<'gc> {
        let t = Table::new(&ctx);
        for (k, v) in ["a", "b", "c", "d"].into_iter().zip(m) {
            t.set(ctx, k, v).unwrap();
        }
        t.set(ctx, "cx", cx).unwrap();
        t.set(ctx, "cy", cy).unwrap();
        t.set(ctx, "scroll_x", cx - 128).unwrap();
        t.set(ctx, "scroll_y", scroll_y).unwrap();
        t.set(ctx, "visible", visible).unwrap();
        t
    }
    // Degrees, visual magnification (2 = twice as large), texture center.
    let mode7_transform = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let zoom = finite(ctx, stack.get(1), "zoom")?;
        if zoom < 1.0 / 127.0 {
            return Err(lua_err(ctx, "zoom must be at least 1/127"));
        }
        let angle = finite(ctx, stack.get(0), "angle")?;
        let x = finite(ctx, stack.get(2), "x")?;
        let z = finite(ctx, stack.get(3), "z")?;
        let (c, s, cx, cy) = pose(angle, x, z);
        let t = view(ctx, [c / zoom, -s / zoom, s / zoom, c / zoom], cx, cy, cy - 112, true);
        stack.replace(ctx, t);
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("mode7_transform", mode7_transform).unwrap();
    // A level camera with focal length 128 pixels. Positions wrap over the
    // 1024x1024 plane; rows at/above the horizon expose the existing backdrop.
    let mode7_floor = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let y = finite(ctx, stack.get(0), "scanline")?;
        let horizon = finite(ctx, stack.get(1), "horizon")?;
        let height = finite(ctx, stack.get(2), "height")?;
        if height <= 0.0 {
            return Err(lua_err(ctx, "height must be positive"));
        }
        let heading = finite(ctx, stack.get(3), "angle")?;
        let x = finite(ctx, stack.get(4), "x")?;
        let z = finite(ctx, stack.get(5), "z")?;
        let (c, s, _, _) = pose(heading, x, z);
        // The signed Q8.8 matrix tops out below 128.
        let scale = (height / (y - horizon).max(1.0)).min(127.0);
        let depth = 128.0 * scale;
        let wrap = |v: f64| ((v + 0.5).floor() as i64).rem_euclid(1024);
        let (cx, cy) = (wrap(x - s * depth), wrap(z + c * depth));
        let t = view(ctx, [c * scale, 0.0, s * scale, 0.0], cx, cy, 0, y > horizon);
        stack.replace(ctx, t);
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("mode7_floor", mode7_floor).unwrap();

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
        let byte = stack.get(0).to_int().unwrap_or(0) as u16;
        stack.clear();
        let cur = ctx.get_global("COLDATA").to_int().unwrap_or(0) as u16 & 0x7fff;
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
        let y0 = stack.get(0).to_int().unwrap_or(0);
        let y1 = stack.get(1).to_int().unwrap_or(0);
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
    sync_color(ctx, k, m, 0, 0, 0); // power-on defaults = decode of zeroed registers

    // Friendly screen-designation namespace over TM/TS ($212C/$212D). Same
    // baseline change-detection pattern as `color` above; `__screen_base`
    // records the bytes at each (re-)baseline.
    let screen = Table::new(&ctx);
    screen.set(ctx, "main", Table::new(&ctx)).unwrap();
    screen.set(ctx, "sub", Table::new(&ctx)).unwrap();
    ctx.set_global("screen", screen).unwrap();
    ctx.set_global("__screen_base", Table::new(&ctx)).unwrap();
    sync_screen(ctx, k, m, 0x00, 0x00); // decode of the power-on TM/TS (both empty)

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
        win.set(ctx, k.win_names()[l.name], Table::new(&ctx))
            .unwrap();
    }
    ctx.set_global("win", win).unwrap();
    ctx.set_global("__win_base", Table::new(&ctx)).unwrap();
    sync_win(ctx, k, m, &[0u8; 11]); // power-on: every window register is zero

    // M12/audio: `aram[addr] = byte` poke surface, drained (once) at the
    // start of `render_frame_audio` — never rebuilt, so echo-unit writes
    // persist. `voice[0..7]`/`dsp` are NOT seeded here — they come from the
    // live DSP registers via `seed_dsp_tables`, called separately right
    // after `install_bindings` so a recompile can't silence a sounding voice.
    ctx.set_global("aram", Table::new(&ctx)).unwrap();
    // Hidden KON/KOFF accumulators: `kon`/`koff` OR bits in; flushed and
    // zeroed by `render_frame_audio` each frame.
    ctx.set_global("__dsp_kon", 0).unwrap();
    ctx.set_global("__dsp_koff", 0).unwrap();

    // kon(...)/koff(...): edge-triggered voice-number varargs, callable from
    // frame(), hooks, and init(). Two calls to the same voice in one frame
    // collapse to the same bit (a no-op repeat), which is exactly the "kon
    // twice == kon once" contract. Byte-identical apart from the name/global,
    // so both are built from the same loop. `flush_dsp_writes` is what
    // actually applies these (and `voice[]`/`dsp`) to the DSP registers, at
    // offset 0 and after every timer hook within `render_frame_audio`. Since
    // that pass runs BEFORE hdma hooks are invoked (see `frame()`), an hdma
    // hook's kon/koff/voice[]/dsp writes sit in these tables un-flushed for
    // the rest of the current frame and land at the NEXT frame's offset-0
    // flush — a one-frame lag (Spec-locked ordering; before this ticket the
    // audio pass ran after hdma, so these writes took effect the same
    // frame).
    for (name, global) in [("kon", "__dsp_kon"), ("koff", "__dsp_koff")] {
        let cb = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let mut mask = 0i64;
            for i in 0..stack.len() {
                match stack.get(i).to_int() {
                    Some(n) if (0..=7).contains(&n) => mask |= 1 << n,
                    _ => return Err(lua_err(ctx, &format!("{name}: voice numbers must be 0..7"))),
                }
            }
            stack.clear();
            let cur = ctx.get_global(global).to_int().unwrap_or(0);
            ctx.set_global(global, cur | mask).unwrap();
            Ok(CallbackReturn::Return)
        });
        ctx.set_global(name, cb).unwrap();
    }
}

/// Decode the live DSP registers into a [`DspView`] — the ONE place that
/// knows the S-DSP register layout. [`LuaEngine::dsp_view`] returns this
/// directly; `seed_dsp_tables` walks it into the Lua `voice`/`dsp` tables.
fn decode_dsp_view(dsp: &Dsp) -> DspView {
    let endx = dsp.read(0x7c);
    let voices = std::array::from_fn(|n| {
        let base = (n as u8) << 4;
        let adsr1 = dsp.read(base | 0x05);
        let adsr2 = dsp.read(base | 0x06);
        DspVoiceView {
            sample: dsp.read(base | 0x04),
            pitch: dsp.read(base | 0x02) as u16 | (((dsp.read(base | 0x03) & 0x3f) as u16) << 8),
            vol: DspLr {
                l: dsp.read(base) as i8,
                r: dsp.read(base | 0x01) as i8,
            },
            adsr: DspAdsr {
                a: adsr1 & 0x0f,
                d: (adsr1 >> 4) & 0x07,
                s: adsr2 >> 5,
                r: adsr2 & 0x1f,
            },
            gain: (adsr1 & 0x80 == 0).then(|| dsp.read(base | 0x07)),
            noise: dsp.read(0x3d) & (1 << n) != 0,
            pmod: dsp.read(0x2d) & (1 << n) != 0,
            echo: dsp.read(0x4d) & (1 << n) != 0,
            envx: dsp.read(base | 0x08),
            outx: dsp.read(base | 0x09) as i8,
            ended: endx & (1 << n) != 0,
        }
    });
    let flg = dsp.read(0x6c);
    DspView {
        voices,
        mvol: DspLr {
            l: dsp.read(0x0c) as i8,
            r: dsp.read(0x1c) as i8,
        },
        evol: DspLr {
            l: dsp.read(0x2c) as i8,
            r: dsp.read(0x3c) as i8,
        },
        echo: DspEchoView {
            delay: dsp.read(0x7d) & 0x0f,
            feedback: dsp.read(0x0d) as i8,
            fir: std::array::from_fn(|i| dsp.read(((i as u8) << 4) | 0x0f) as i8),
        },
        noise_clock: flg & 0x1f,
        mute: flg & 0x40 != 0,
        // Filled by `LuaEngine::dsp_view` from `self.dma.samples` — this
        // function only sees the `Dsp` registers, not the recorder.
        samples: Vec::new(),
    }
}

/// Seed `voice[0..7]`/`dsp` from the LIVE DSP register state — called right
/// after `install_bindings` in both `LuaEngine::new` and `set_sources`, so
/// the DSL tables always start as a faithful decode of whatever the chip
/// currently holds (a fresh `Dsp` after the ADSR1=0x80 power-on writes, or
/// the still-sounding registers across a recompile). Also republishes
/// envx/outx/ended (see `publish_voice_readbacks`).
fn seed_dsp_tables(ctx: piccolo::Context<'_>, dsp: &Dsp, unmixed: Option<&DspView>) {
    let view = unmixed.cloned().unwrap_or_else(|| decode_dsp_view(dsp));
    let voices = Table::new(&ctx);
    for (n, vv) in view.voices.iter().enumerate() {
        let v = Table::new(&ctx);
        v.set(ctx, "sample", vv.sample as i64).unwrap();
        v.set(ctx, "pitch", vv.pitch as i64).unwrap();

        let vol = Table::new(&ctx);
        vol.set(ctx, "l", vv.vol.l as i64).unwrap();
        vol.set(ctx, "r", vv.vol.r as i64).unwrap();
        v.set(ctx, "vol", vol).unwrap();

        let adsr = Table::new(&ctx);
        adsr.set(ctx, "a", vv.adsr.a as i64).unwrap();
        adsr.set(ctx, "d", vv.adsr.d as i64).unwrap();
        adsr.set(ctx, "s", vv.adsr.s as i64).unwrap();
        adsr.set(ctx, "r", vv.adsr.r as i64).unwrap();
        v.set(ctx, "adsr", adsr).unwrap();

        v.set(
            ctx,
            "gain",
            match vv.gain {
                Some(g) => Value::Integer(g as i64),
                None => Value::Nil,
            },
        )
        .unwrap();

        v.set(ctx, "noise", vv.noise).unwrap();
        v.set(ctx, "pmod", vv.pmod).unwrap();
        v.set(ctx, "echo", vv.echo).unwrap();

        voices.set(ctx, n as i64, v).unwrap();
    }
    ctx.set_global("voice", voices).unwrap();

    let d = Table::new(&ctx);
    let mvol = Table::new(&ctx);
    mvol.set(ctx, "l", view.mvol.l as i64).unwrap();
    mvol.set(ctx, "r", view.mvol.r as i64).unwrap();
    d.set(ctx, "mvol", mvol).unwrap();
    let evol = Table::new(&ctx);
    evol.set(ctx, "l", view.evol.l as i64).unwrap();
    evol.set(ctx, "r", view.evol.r as i64).unwrap();
    d.set(ctx, "evol", evol).unwrap();

    let echo = Table::new(&ctx);
    echo.set(ctx, "feedback", view.echo.feedback as i64)
        .unwrap();
    echo.set(ctx, "delay", view.echo.delay as i64).unwrap();
    let fir = Table::new(&ctx);
    for (i, c) in view.echo.fir.iter().enumerate() {
        fir.set(ctx, i as i64 + 1, *c as i64).unwrap();
    }
    echo.set(ctx, "fir", fir).unwrap();
    d.set(ctx, "echo", echo).unwrap();

    d.set(ctx, "noise_clock", view.noise_clock as i64).unwrap();
    d.set(ctx, "mute", view.mute).unwrap();
    ctx.set_global("dsp", d).unwrap();

    publish_voice_readbacks(ctx, &decode_dsp_view(dsp));
}

/// Publish `voice[n].envx/.outx/.ended` from a decoded [`DspView`]. Called at
/// seeding and at the end of every `render_frame_audio`, so the NEXT
/// `frame()`/hook sees this frame's envelope/output/end-of-sample state.
/// `ended` mirrors raw ENDX, which also pulses on a LOOPING sample's END
/// block (real hardware behavior, not a non-looping-only "finished" flag).
fn publish_voice_readbacks(ctx: piccolo::Context<'_>, view: &DspView) {
    let Value::Table(voices) = ctx.get_global("voice") else {
        return;
    };
    for (n, vv) in view.voices.iter().enumerate() {
        let Value::Table(v) = voices.get(ctx, n as i64) else {
            continue;
        };
        v.set(ctx, "envx", vv.envx as i64).unwrap();
        v.set(ctx, "outx", vv.outx as i64).unwrap();
        v.set(ctx, "ended", vv.ended).unwrap();
    }
}

/// Write `voice[0..7]`/`dsp` back to the DSP registers, wholesale and
/// idempotent — the DSL tables are the single source of truth, rebuilt into
/// registers every frame exactly like `write_state` does for the PPU side.
/// Never writes ENDX (0x7c) or the read-only ENVX/OUTX (n8/n9); KON/KOFF are
/// flushed separately by the caller (`render_frame_audio`), after this.
fn write_dsp_regs(ctx: piccolo::Context<'_>, dsp: &mut Dsp) {
    fn geti<'gc>(ctx: piccolo::Context<'gc>, t: Table<'gc>, k: &'static str) -> Option<i64> {
        t.get(ctx, k).to_int()
    }
    // Rule: unsigned fields mask/wrap to their register width (sample wraps
    // mod 256, pitch/noise_clock/gain mask their bit width); signed fields
    // (vol/feedback/FIR) and delay clamp to their range instead — so -200
    // lands at -128, not wrapping to +56.
    let clamp_i8 = |v: i64| -> u8 { v.clamp(-128, 127) as i8 as u8 };

    let mut non = 0u8;
    let mut pmon = 0u8;
    let mut eon = 0u8;

    if let Value::Table(voices) = ctx.get_global("voice") {
        for n in 0i64..8 {
            let Value::Table(v) = voices.get(ctx, n) else {
                continue;
            };
            let base = (n as u8) << 4;

            if v.get(ctx, "noise").to_bool() {
                non |= 1 << n;
            }
            if v.get(ctx, "pmod").to_bool() {
                pmon |= 1 << n;
            }
            if v.get(ctx, "echo").to_bool() {
                eon |= 1 << n;
            }

            dsp.write(base | 0x04, geti(ctx, v, "sample").unwrap_or(0) as u8);

            let pitch = geti(ctx, v, "pitch").unwrap_or(0) & 0x3fff;
            dsp.write(base | 0x02, (pitch & 0xff) as u8);
            dsp.write(base | 0x03, (pitch >> 8) as u8);

            if let Value::Table(vol) = v.get(ctx, "vol") {
                dsp.write(base, clamp_i8(geti(ctx, vol, "l").unwrap_or(0)));
                dsp.write(base | 0x01, clamp_i8(geti(ctx, vol, "r").unwrap_or(0)));
            } else {
                dsp.write(base, 0);
                dsp.write(base | 0x01, 0);
            }

            let (a, d, s, r) = if let Value::Table(adsr) = v.get(ctx, "adsr") {
                (
                    geti(ctx, adsr, "a").unwrap_or(0) & 0x0f,
                    geti(ctx, adsr, "d").unwrap_or(0) & 0x07,
                    geti(ctx, adsr, "s").unwrap_or(0) & 0x07,
                    geti(ctx, adsr, "r").unwrap_or(0) & 0x1f,
                )
            } else {
                (0, 0, 0, 0)
            };
            let gain = geti(ctx, v, "gain");
            let adsr1 = match gain {
                Some(_) => ((d as u8) << 4) | (a as u8), // GAIN mode: bit7 clear
                None => 0x80 | ((d as u8) << 4) | (a as u8), // ADSR mode: bit7 set
            };
            dsp.write(base | 0x05, adsr1);
            dsp.write(base | 0x06, ((s as u8) << 5) | (r as u8));
            if let Some(g) = gain {
                dsp.write(base | 0x07, (g & 0xff) as u8);
            }
        }
    }
    dsp.write(0x3d, non);
    dsp.write(0x2d, pmon);
    dsp.write(0x4d, eon);

    if let Value::Table(d) = ctx.get_global("dsp") {
        if let Value::Table(mvol) = d.get(ctx, "mvol") {
            dsp.write(0x0c, clamp_i8(geti(ctx, mvol, "l").unwrap_or(0)));
            dsp.write(0x1c, clamp_i8(geti(ctx, mvol, "r").unwrap_or(0)));
        }
        if let Value::Table(evol) = d.get(ctx, "evol") {
            dsp.write(0x2c, clamp_i8(geti(ctx, evol, "l").unwrap_or(0)));
            dsp.write(0x3c, clamp_i8(geti(ctx, evol, "r").unwrap_or(0)));
        }

        let mut delay = 0i64;
        if let Value::Table(echo) = d.get(ctx, "echo") {
            dsp.write(0x0d, clamp_i8(geti(ctx, echo, "feedback").unwrap_or(0)));
            if let Value::Table(fir) = echo.get(ctx, "fir") {
                for i in 0i64..8 {
                    let c = fir.get(ctx, i + 1).to_int().unwrap_or(0);
                    dsp.write(((i as u8) << 4) | 0x0f, clamp_i8(c));
                }
            } else {
                for i in 0i64..8 {
                    dsp.write(((i as u8) << 4) | 0x0f, 0);
                }
            }
            delay = geti(ctx, echo, "delay").unwrap_or(0).clamp(0, 15);
        }
        dsp.write(0x7d, delay as u8);
        // Echo RAM sits at the TOP of ARAM: delay 0 is the documented "no
        // echo writes" sentinel (ESA 0xff); every other delay ends its
        // buffer exactly at 0x10000 (ESA*0x100 + delay*0x800 == 0x10000).
        // ESA is the echo region's start page, from the same formula
        // `echo_region` uses to reject a sample `dma()` placement.
        let esa = (echo_region(delay).0 >> 8) as u8;
        dsp.write(0x6d, esa);

        let noise_clock = geti(ctx, d, "noise_clock").unwrap_or(0) & 0x1f;
        let mute = d.get(ctx, "mute").to_bool();
        let flg = ((mute as u8) << 6) | (((delay == 0) as u8) << 5) | (noise_clock as u8);
        dsp.write(0x6c, flg);
    }
}

/// Register values are integers to the chip but numbers to the author —
/// `t`-driven math hands over floats everywhere — so every value read floors
/// a float instead of silently dropping the write. Table KEYS stay exact.
/// The DSL's one number-to-integer rule: floats floor. Applies to every
/// register VALUE and every memory-table KEY (`cgram[i]`, `vram[a]`,
/// `bg[n].map[c][r]`, `m7.map[y][x]`), so `cgram[i / 2]` lands on entry
/// `floor(i / 2)` instead of silently vanishing as a non-integer key.
trait ToInt {
    fn to_int(self) -> Option<i64>;
}
impl ToInt for Value<'_> {
    fn to_int(self) -> Option<i64> {
        match self {
            Value::Integer(i) => Some(i),
            Value::Number(f) if f.is_finite() => Some(f.floor() as i64),
            _ => None,
        }
    }
}

/// A string Lua runtime error raised from a native callback (piccolo has no
/// tracebacks; per-file attribution happens at the chunk/frame/hook seam).
fn lua_err<'gc>(ctx: piccolo::Context<'gc>, msg: &str) -> piccolo::Error<'gc> {
    Value::String(ctx.intern(msg.as_bytes())).into()
}

/// Install the `dma(name, opts?)` global — the init-stage VRAM placement
/// primitive (M11). Callable only while `rec.active` (top-level chunk code +
/// `init()`, i.e. during set_sources); anywhere else it raises. Validates the
/// source name against the live store LOUDLY (a typo is an init error, not a
/// silent blank layer), resolves `{char, map, pal}` opts (defaults 0x1000 /
/// 0x0000 / 0; pal is the CGRAM entry index the palette block starts at),
/// records the placement for per-frame replay, and returns the resolved table
/// so registers wire from it. An m7 source takes NO opts — its chars+map live
/// interleaved at the fixed 0x0000 region (palette at CGRAM 1) on real
/// hardware, so explicit addresses are an error; it returns
/// `{char = 0, map = 0, pal = 1, tiles_w, tiles_h}`.
fn install_dma(
    ctx: piccolo::Context<'_>,
    store: Rc<RefCell<HashMap<String, crate::source::SourcePayload>>>,
    rec: Rc<DmaRecorder>,
) {
    use crate::source::SourceKind;
    let timer_rec = rec.clone();
    let dma = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        if !rec.active.get() {
            return Err(lua_err(
                ctx,
                "dma runs during setup — call it from top-level code, not frame() or hooks",
            ));
        }
        let name = match stack.get(0) {
            Value::String(s) => String::from_utf8_lossy(s.as_bytes()).into_owned(),
            _ => {
                return Err(lua_err(
                    ctx,
                    "dma: first argument must be a source name (string)",
                ))
            }
        };
        let source = match store
            .borrow()
            .get(&name)
            .cloned()
            .or_else(|| crate::bank::get(&name))
        {
            Some(p) => p,
            None => return Err(lua_err(ctx, &format!("dma: no source named '{name}'"))),
        };
        let kind = source.kind();
        let opts = match stack.get(1) {
            Value::Table(t) => Some(t),
            Value::Nil => None,
            _ => return Err(lua_err(ctx, "dma: opts must be a table")),
        };
        let opt = |key: &'static str| opts.map_or(Value::Nil, |t| t.get(ctx, key));
        let (char_base, map_base, cgram_base) =
            if kind == SourceKind::M7 || kind == SourceKind::Sample {
                for k in ["char", "map", "pal"] {
                    if !matches!(opt(k), Value::Nil) {
                        let msg = if kind == SourceKind::M7 {
                            format!(
                                "dma: '{name}' is an m7 source — chars+map always live \
                             interleaved at 0x0000 (palette at CGRAM 1), so it takes no '{k}' opt"
                            )
                        } else {
                            format!(
                            "dma: '{name}' is a sample source — it takes only 'addr', not '{k}'"
                        )
                        };
                        return Err(lua_err(ctx, &msg));
                    }
                }
                (0, 0, 0)
            } else {
                let int_opt = |key: &'static str, default: i64, max: i64| match opt(key) {
                    Value::Nil => Ok(default),
                    v => match v.to_int() {
                        Some(n) if (0..=max).contains(&n) => Ok(n),
                        _ => Err(lua_err(
                            ctx,
                            &format!("dma: opts.{key} must be an integer in 0..{max:#x}"),
                        )),
                    },
                };
                (
                    int_opt("char", 0x1000, 0x7fff)?,
                    int_opt("map", 0x0000, 0x7fff)?,
                    int_opt("pal", 0, if kind == SourceKind::Obj { 7 } else { 0xff })?,
                )
            };

        let align_up = |n: usize, align: usize| (n + align - 1) & !(align - 1);
        let mut vram = Vec::<(usize, usize)>::new();
        let cgram;
        let cgram_end;
        let ret = Table::new(&ctx);
        match &source {
            crate::source::SourcePayload::Bg(src) => {
                if char_base & 0x0fff != 0 || map_base & 0x03ff != 0 {
                    return Err(lua_err(
                        ctx,
                        "dma: bg char must align to 0x1000 and map to 0x400",
                    ));
                }
                let char_end = char_base as usize + src.char_words.len();
                let map_end = map_base as usize + src.tilemap_words.len();
                let stride = if src.bit_depth == 2 {
                    4
                } else if src.bit_depth == 8 {
                    256
                } else {
                    16
                };
                let pal_end = cgram_base as usize + stride * src.palettes.len();
                vram.extend([(char_base as usize, char_end), (map_base as usize, map_end)]);
                cgram_end = pal_end;
                cgram = (cgram_base as usize, pal_end);
                ret.set(ctx, "char", char_base).unwrap();
                ret.set(ctx, "map", map_base).unwrap();
                ret.set(ctx, "pal", cgram_base).unwrap();
                ret.set(ctx, "next_char", align_up(char_end, 0x1000) as i64)
                    .unwrap();
                ret.set(ctx, "next_map", align_up(map_end, 0x400) as i64)
                    .unwrap();
                ret.set(ctx, "next_pal", pal_end as i64).unwrap();
                ret.set(ctx, "bit_depth", src.bit_depth as i64).unwrap();
                ret.set(ctx, "screen_size", src.screen_size as i64).unwrap();
            }
            crate::source::SourcePayload::Sheet(src) => {
                if char_base & 0x0fff != 0 {
                    return Err(lua_err(ctx, "dma: sheet char must align to 0x1000"));
                }
                let char_end = char_base as usize + src.char_words.len();
                let stride = if src.bit_depth == 2 {
                    4
                } else if src.bit_depth == 8 {
                    256
                } else {
                    16
                };
                let pal_end = cgram_base as usize + stride * src.palettes.len();
                vram.push((char_base as usize, char_end));
                cgram_end = pal_end;
                cgram = (cgram_base as usize, pal_end);
                ret.set(ctx, "char", char_base).unwrap();
                ret.set(ctx, "pal", cgram_base).unwrap();
                ret.set(ctx, "next_char", align_up(char_end, 0x1000) as i64)
                    .unwrap();
                ret.set(ctx, "next_pal", pal_end as i64).unwrap();
                ret.set(ctx, "bit_depth", src.bit_depth as i64).unwrap();
            }
            crate::source::SourcePayload::Obj(src) => {
                let base = match rec.obj_base.get() {
                    Some(base) => base,
                    None => {
                        if char_base & 0x1fff != 0 {
                            return Err(lua_err(ctx, "dma: first obj char must align to 0x2000"));
                        }
                        rec.obj_base.set(Some(char_base as u16));
                        char_base as u16
                    }
                };
                let char_end = char_base as usize + src.char_words.len();
                if char_base < base as i64
                    || (char_base - base as i64) % 16 != 0
                    || char_end > base as usize + 0x2000
                {
                    return Err(lua_err(
                        ctx,
                        "dma: obj chars must fit the shared 512-tile region",
                    ));
                }
                let pal_end = cgram_base as usize + src.palettes.len();
                vram.push((char_base as usize, char_end));
                cgram_end = 128 + pal_end * 16;
                cgram = (128 + cgram_base as usize * 16, cgram_end);
                ret.set(ctx, "char", char_base).unwrap();
                ret.set(ctx, "pal", cgram_base).unwrap();
                ret.set(ctx, "tile", (char_base - base as i64) / 16)
                    .unwrap();
                ret.set(ctx, "next_char", align_up(char_end, 16) as i64)
                    .unwrap();
                ret.set(ctx, "next_pal", pal_end as i64).unwrap();
                ret.set(ctx, "cell_size", src.cell_size as i64).unwrap();
                let cells = Table::new(&ctx);
                for (i, cell) in src.cells.iter().enumerate() {
                    let t = Table::new(&ctx);
                    t.set(ctx, "tile", cell.tile as i64).unwrap();
                    t.set(ctx, "pal", cell.pal as i64).unwrap();
                    t.set(ctx, "flip_x", cell.flip_x).unwrap();
                    t.set(ctx, "flip_y", cell.flip_y).unwrap();
                    cells.set(ctx, i as i64 + 1, t).unwrap();
                }
                ret.set(ctx, "cells", cells).unwrap();
            }
            crate::source::SourcePayload::M7(src) => {
                vram.push((0, 0x4000));
                // The plane is 8bpp, but it only WRITES entries 1..=colors;
                // OBJ palettes (128+) and offset BG bases past that end can
                // coexist with it, exactly as on hardware.
                cgram_end = 1 + src.palette.len();
                cgram = (0, cgram_end);
                ret.set(ctx, "char", 0).unwrap();
                ret.set(ctx, "map", 0).unwrap();
                ret.set(ctx, "pal", 1).unwrap();
                ret.set(ctx, "tiles_w", src.tiles_w as i64).unwrap();
                ret.set(ctx, "tiles_h", src.tiles_h as i64).unwrap();
            }
            // Samples are PCM written into ARAM, not VRAM/CGRAM graphics data
            // — `dma()`'s char/map/pal placement doesn't apply; they take an
            // `addr` opt instead and are recorded into `rec.samples` (never
            // `rec.placements` — samples are placed once at compile time in
            // `set_sources`, not replayed per frame).
            crate::source::SourcePayload::Sample(src) => {
                let recorded = rec.samples.borrow().len();
                if recorded >= 256 {
                    return Err(lua_err(
                        ctx,
                        &format!("dma: '{name}' exceeds the 256-entry sample directory"),
                    ));
                }
                let id = recorded as u8;
                // No addr: chain below the highest placement so far, so a
                // sequence of default dma() calls never overlaps.
                let addr: u32 = match opt("addr") {
                    Value::Nil => rec
                        .samples
                        .borrow()
                        .iter()
                        .map(|s| s.end)
                        .max()
                        .unwrap_or(SAMPLE_DIR_END)
                        .max(SAMPLE_DIR_END),
                    v => match v.to_int() {
                        Some(n) if (0..=0xffff).contains(&n) => n as u32,
                        _ => {
                            return Err(lua_err(
                                ctx,
                                "dma: opts.addr must be an integer in 0..0xffff",
                            ))
                        }
                    },
                };
                let bytes = src.brr.len() as u32;
                let end = addr + bytes;
                if end > 0x10000 {
                    return Err(lua_err(
                        ctx,
                        &format!("dma: '{name}' placement exceeds sound RAM"),
                    ));
                }
                // Reserved sample directory page: [SAMPLE_DIR, SAMPLE_DIR_END).
                if addr < SAMPLE_DIR_END && SAMPLE_DIR < end {
                    return Err(lua_err(
                        ctx,
                        &format!(
                            "dma: '{name}' overlaps the sample directory page (0x{SAMPLE_DIR:04x}-0x{:04x})",
                            SAMPLE_DIR_END - 1
                        ),
                    ));
                }
                // Echo region, derived from `dsp.echo.delay` as it stands
                // right now (see `echo_region`'s doc for the formula, which
                // mirrors `write_dsp_regs`'s ESA computation).
                let delay = match ctx.get_global("dsp") {
                    Value::Table(d) => match d.get(ctx, "echo") {
                        Value::Table(echo) => echo.get(ctx, "delay").to_int().unwrap_or(0),
                        _ => 0,
                    },
                    _ => 0,
                }
                .clamp(0, 15);
                let (echo_lo, echo_hi) = echo_region(delay);
                if addr < echo_hi && echo_lo < end {
                    return Err(lua_err(
                        ctx,
                        &format!(
                            "dma: '{name}' overlaps the echo region (0x{echo_lo:04x}-0x{hi:04x}, \
                             dsp.echo.delay = {delay})",
                            hi = echo_hi - 1
                        ),
                    ));
                }
                if let Some(other) = rec
                    .samples
                    .borrow()
                    .iter()
                    .find(|s| addr < s.end && (s.addr as u32) < end)
                {
                    return Err(lua_err(
                        ctx,
                        &format!(
                            "dma: '{name}' overlaps sample '{}' in sound RAM",
                            other.name
                        ),
                    ));
                }
                let loop_addr = match src.loop_block {
                    Some(lb) => (addr + lb as u32 * 9) as u16,
                    None => addr as u16,
                };
                rec.samples.borrow_mut().push(SamplePlacement {
                    name: name.clone(),
                    id,
                    addr: addr as u16,
                    end,
                    loop_addr,
                });
                ret.set(ctx, "id", id as i64).unwrap();
                ret.set(ctx, "addr", addr as i64).unwrap();
                ret.set(ctx, "next_addr", end as i64).unwrap();
                stack.clear();
                stack.replace(ctx, ret);
                return Ok(CallbackReturn::Return);
            }
        }
        if vram.iter().any(|&(s, e)| e > 0x8000 || s >= e) || cgram_end > 256 {
            return Err(lua_err(
                ctx,
                &format!("dma: '{name}' placement exceeds PPU memory"),
            ));
        }
        for (i, &(s, e)) in vram.iter().enumerate() {
            if vram[..i].iter().any(|&(a, b)| s < b && a < e)
                || rec
                    .vram_ranges
                    .borrow()
                    .iter()
                    .any(|(_, a, b)| s < *b && *a < e)
            {
                return Err(lua_err(
                    ctx,
                    &format!("dma: '{name}' overlaps VRAM already in use"),
                ));
            }
        }
        if rec
            .cgram_ranges
            .borrow()
            .iter()
            .any(|(_, start, end)| cgram.0 != *start && cgram.0 < *end && *start < cgram.1)
        {
            return Err(lua_err(
                ctx,
                &format!("dma: '{name}' conflicts with CGRAM already in use"),
            ));
        }
        rec.vram_ranges
            .borrow_mut()
            .extend(vram.into_iter().map(|(s, e)| (name.clone(), s, e)));
        rec.cgram_ranges
            .borrow_mut()
            .push((name.clone(), cgram.0, cgram.1));
        rec.placements.borrow_mut().push(DmaPlacement {
            name,
            char_base: char_base as u16,
            map_base: map_base as u16,
            cgram_base: cgram_base as u8,
        });
        stack.clear();
        stack.replace(ctx, ret);
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("dma", dma).unwrap();

    // timer(n, div, fn): register an SPC700 timer hook (M12/audio). Same
    // init-window gate as `dma` — top-level chunks + `init()` only; a call
    // from frame() or any hook raises the same-shaped error. `n` selects
    // which of the three hardware timers (0/1 tick at 8 kHz, 2 at 64 kHz);
    // `div` (1..=255) sets the period in ticks. See `render_frame_audio` for
    // how registrations become the per-frame segment walker.
    let timer = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        if !timer_rec.active.get() {
            return Err(lua_err(
                ctx,
                "timer runs during setup — call it from top-level code, not frame() or hooks",
            ));
        }
        let n = match stack.get(0).to_int() {
            Some(n) if (0..=2).contains(&n) => n as u8,
            _ => return Err(lua_err(ctx, "timer: n must be an integer in 0..2")),
        };
        let div = match stack.get(1).to_int() {
            Some(d) if (1..=255).contains(&d) => d as u8,
            _ => return Err(lua_err(ctx, "timer: div must be an integer in 1..255")),
        };
        let f = match stack.get(2) {
            Value::Function(f) => f,
            _ => return Err(lua_err(ctx, "timer: third argument must be a function")),
        };
        let file = function_chunk_name(&f);
        stack.clear();
        timer_rec
            .timers
            .borrow_mut()
            .push((n, div, ctx.stash(f), file));
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("timer", timer).unwrap();
}

/// Re-run the init-recorded `dma()` placements in call order into the frame's
/// zeroed VRAM/CGRAM. The payload's committed kind decides the layout — NOT
/// the frame-wide mode (the whole point of M11: an m7 payload and a tile bg
/// coexist in one frame). Names resolve against the LIVE store: an
/// `add_source` under a placed name flows in next frame; a removed one
/// degrades to a Mismatch report (mirrors the old binding-path UX) and places
/// nothing.
fn replay_dma(
    placements: &[DmaPlacement],
    store: &HashMap<String, crate::source::SourcePayload>,
    reports: &mut Vec<ImportBudget>,
    mem: &mut Memory,
) {
    use crate::source::SourcePayload;
    for p in placements {
        match store.get(&p.name) {
            Some(SourcePayload::Bg(src)) => {
                crate::source::place_bg(src, mem, p.map_base, p.char_base, p.cgram_base as usize)
            }
            Some(SourcePayload::Sheet(src)) => {
                crate::source::place_sheet(src, mem, p.char_base, p.cgram_base as usize)
            }
            Some(SourcePayload::M7(src)) => crate::source::place_m7(src, mem),
            Some(SourcePayload::Obj(src)) => {
                crate::source::place_obj(src, mem, p.char_base, p.cgram_base as usize)
            }
            // Samples are written once by `LuaEngine::set_sources` after
            // `init()`, never replayed per frame — a `dma()` placement
            // naming one is a no-op here.
            Some(SourcePayload::Sample(_)) => {}
            None => reports.push(ImportBudget::Mismatch {
                layer: None,
                slot: p.name.clone(),
                expected: "dma placement".into(),
                found: MISSING_SOURCE.into(),
            }),
        }
    }
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

/// Every Lua string the per-scanline glue (`write_state` / hook /
/// `read_state` / `take_cgram_pokes`) touches, interned ONCE at engine
/// construction and re-fetched per `enter`. Passing a `&'static str` to
/// piccolo re-interns it on every access (a hash-set probe per key per
/// line); a fetched `Value::String` skips that entirely. Field names are
/// the Lua spelling (raw register mnemonics stay uppercase).
macro_rules! keys {
    ($($f:ident: $s:literal),* $(,)?) => {
        #[allow(non_snake_case)]
        struct StashedKeys { $($f: piccolo::registry::StashedString),* }
        #[allow(non_snake_case)]
        #[derive(Clone, Copy)]
        struct Keys<'gc> { $($f: Value<'gc>),* }
        impl StashedKeys {
            fn new(ctx: piccolo::Context<'_>) -> Self {
                Self { $($f: ctx.stash(ctx.intern_static($s.as_bytes()))),* }
            }
            fn fetch<'gc>(&self, ctx: piccolo::Context<'gc>) -> Keys<'gc> {
                Keys { $($f: Value::String(ctx.fetch(&self.$f))),* }
            }
        }
    };
}
keys! {
    mode: "mode", bg3_priority: "bg3_priority", brightness: "brightness",
    TM: "TM", TS: "TS", WH0: "WH0", WH1: "WH1", WH2: "WH2", WH3: "WH3",
    W12SEL: "W12SEL", W34SEL: "W34SEL", WOBJSEL: "WOBJSEL", WBGLOG: "WBGLOG",
    WOBJLOG: "WOBJLOG", TMW: "TMW", TSW: "TSW", CGWSEL: "CGWSEL",
    CGADSUB: "CGADSUB", COLDATA: "COLDATA", mosaic: "mosaic",
    direct_color: "direct_color", force_blank: "force_blank",
    screen: "screen", screen_base: "__screen_base", tm: "tm", ts: "ts",
    main: "main", sub: "sub", bg1: "bg1", bg2: "bg2", bg3: "bg3", bg4: "bg4",
    obj: "obj", backdrop: "backdrop", color: "color",
    win: "win", win_base: "__win_base", w1: "w1", w2: "w2", lo: "lo", hi: "hi",
    invert: "invert", combine: "combine", or_: "OR", and_: "AND", xor: "XOR",
    xnor: "XNOR", wh0: "wh0", wh1: "wh1", wh2: "wh2", wh3: "wh3",
    w12sel: "w12sel", w34sel: "w34sel", wobjsel: "wobjsel", wbglog: "wbglog",
    wobjlog: "wobjlog", tmw: "tmw", tsw: "tsw",
    color_base: "__color_base", cgwsel: "cgwsel", cgadsub: "cgadsub",
    coldata: "coldata", op: "op", half: "half", on: "on", addend: "addend",
    region: "region", fixed: "fixed", add: "add", everywhere: "everywhere",
    inside: "inside", outside: "outside", never: "never",
    bg: "bg", scroll: "scroll", x: "x", y: "y", visible: "visible",
    tile_size: "tile_size", map_base: "map_base", screen_size: "screen_size",
    char_base: "char_base",
    m7: "m7", a: "a", b: "b", c: "c", d: "d", cx: "cx", cy: "cy", wrap: "wrap",
    flip_x: "flip_x", flip_y: "flip_y", extbg: "extbg",
    cgram: "cgram",
}

impl<'gc> Keys<'gc> {
    /// The five TM/TS layer-enable fields, LSB-first.
    fn screen_layers(&self) -> [(Value<'gc>, u8); 5] {
        [
            (self.bg1, 0x01),
            (self.bg2, 0x02),
            (self.bg3, 0x04),
            (self.bg4, 0x08),
            (self.obj, 0x10),
        ]
    }
    /// CGADSUB layer-enable fields (`color.on.*`), LSB-first.
    fn color_on(&self) -> [(Value<'gc>, u8); 6] {
        [
            (self.bg1, 0x01),
            (self.bg2, 0x02),
            (self.bg3, 0x04),
            (self.bg4, 0x08),
            (self.obj, 0x10),
            (self.backdrop, 0x20),
        ]
    }
    /// `__win_base` keys in `WinBytes` order.
    fn win_base_keys(&self) -> [Value<'gc>; 11] {
        [
            self.wh0,
            self.wh1,
            self.wh2,
            self.wh3,
            self.w12sel,
            self.w34sel,
            self.wobjsel,
            self.wbglog,
            self.wobjlog,
            self.tmw,
            self.tsw,
        ]
    }
    /// The raw window mnemonics (WH0-3, W12SEL, W34SEL, WOBJSEL, WBGLOG,
    /// WOBJLOG, TMW, TSW) in `WinBytes` order — same order as
    /// `win_base_keys`, but the globals' own names, not `__win_base`'s.
    fn win_raw_keys(&self) -> [Value<'gc>; 11] {
        [
            self.WH0,
            self.WH1,
            self.WH2,
            self.WH3,
            self.W12SEL,
            self.W34SEL,
            self.WOBJSEL,
            self.WBGLOG,
            self.WOBJLOG,
            self.TMW,
            self.TSW,
        ]
    }
    /// `win.<layer>` keys in `WIN_LAYERS` order.
    fn win_names(&self) -> [Value<'gc>; 6] {
        [self.bg1, self.bg2, self.bg3, self.bg4, self.obj, self.color]
    }
    /// win.w1/.w2 edge fields in WinBytes order (WH0, WH1, WH2, WH3).
    fn win_edges(&self) -> [(Value<'gc>, Value<'gc>); 4] {
        [
            (self.w1, self.lo),
            (self.w1, self.hi),
            (self.w2, self.lo),
            (self.w2, self.hi),
        ]
    }
    /// The eleven enumerated-string values `Raw::S`/`S_*` index into, in
    /// `S_*` order (add, sub, fixed, everywhere, inside, outside, never,
    /// OR, AND, XOR, XNOR) — the one spelling of this set; `string` and
    /// `rd_s` both index it instead of repeating the list.
    fn strings(&self) -> [Value<'gc>; 11] {
        [
            self.add,
            self.sub,
            self.fixed,
            self.everywhere,
            self.inside,
            self.outside,
            self.never,
            self.or_,
            self.and_,
            self.xor,
            self.xnor,
        ]
    }
    /// `strings()[i]` — the friendly string value for an `S_*`/`Raw::S` index.
    fn string(&self, i: u8) -> Value<'gc> {
        self.strings()[i as usize]
    }
}

/// CGWSEL bits owned by the friendly `color` namespace: bit1 addend,
/// bits 4-5 prevent-math region. Bit 0 (direct_color) and bits 6-7
/// (clip-to-black) are NOT color's — leave them to the raw byte.
const COLOR_CGWSEL_MASK: u8 = 0x32;

/// One register global's RAW Lua value as of the last `read_state` /
/// `write_state`: the diff-on-write baseline. `None` means "unknown, or
/// something the row cannot reproduce exactly" (nil, a float in an integer
/// slot, an unrecognised string, a table that was out of reach) and forces
/// the next write, so a hook that leaves a wrong-typed or out-of-range value
/// behind is still reset exactly as a full write would.
#[derive(Clone, Copy, PartialEq)]
enum Raw {
    I(i64),
    B(bool),
    /// `f64::to_bits`, so -0.0 / NaN payloads compare exactly, not by value.
    F(u64),
    /// Index into `Keys::string`.
    S(u8),
}
type Slot = Option<Raw>;

// `Raw::S` indices (`Keys::string`); the four combine ops are contiguous
// from `S_OR` in WBGLOG/WOBJLOG slot order (OR, AND, XOR, XNOR).
const S_ADD: u8 = 0;
const S_SUB: u8 = 1;
const S_FIXED: u8 = 2;
const S_EVERYWHERE: u8 = 3;
const S_INSIDE: u8 = 4;
const S_OUTSIDE: u8 = 5;
const S_NEVER: u8 = 6;
const S_OR: u8 = 7;

/// The raw values every register global held after the last `read_state`
/// / `write_state` (see [`Raw`]). Arrays follow the iteration order of the
/// reader/writer pair that owns them. `Default` = all unknown = the next
/// `write_state` writes everything.
#[derive(Default)]
struct Mirror {
    mode: Slot,
    bg3_priority: Slot,
    brightness: Slot,
    tm: Slot,
    ts: Slot,
    /// WH0-3, W12SEL, W34SEL, WOBJSEL, WBGLOG, WOBJLOG, TMW, TSW — the raw
    /// mnemonics in `WinBytes`/`Keys::win_raw_keys` order.
    win_raw: [Slot; 11],
    cgwsel: Slot,
    cgadsub: Slot,
    coldata: Slot,
    mosaic: Slot,
    direct_color: Slot,
    force_blank: Slot,
    /// `screen.main` / `screen.sub` x `Keys::screen_layers`.
    screen: [[Slot; 5]; 2],
    /// `__screen_base.tm/.ts`.
    screen_base: [Slot; 2],
    /// `win.w1.lo/.hi`, `win.w2.lo/.hi` (`Keys::win_edges`).
    win_edge: [Slot; 4],
    /// `WIN_LAYERS` x (w1, w2, invert, combine, main, sub).
    win: [[Slot; 6]; 6],
    /// `__win_base` in `WinBytes` order.
    win_base: [Slot; 11],
    /// `color.op/.half/.addend/.region/.fixed`.
    color: [Slot; 5],
    /// `color.on` x `Keys::color_on`.
    color_on: [Slot; 6],
    /// `__color_base.cgwsel/.cgadsub/.coldata`.
    color_base: [Slot; 3],
    /// `bg[n]` x (scroll.x, scroll.y, visible, tile_size, map_base,
    /// screen_size, char_base, mosaic).
    bg: [[Slot; 8]; 4],
    /// `m7` x (a, b, c, d, cx, cy, wrap, flip_x, flip_y, extbg).
    m7: [Slot; 10],
}

/// Record a raw read into its mirror slot; returns the value untouched so
/// the existing normalisation (`to_int` / `to_bool` / `to_number`) stays.
fn rd<'gc>(v: Value<'gc>, slot: &mut Slot) -> Value<'gc> {
    *slot = match v {
        Value::Integer(i) => Some(Raw::I(i)),
        Value::Boolean(b) => Some(Raw::B(b)),
        Value::Number(f) => Some(Raw::F(f.to_bits())),
        _ => None,
    };
    v
}

/// `rd` for the enumerated-string fields: the `Keys::string` index of the
/// value if it is one of them (also recorded in the slot), else `None`.
/// Matches against `k.strings()` — the single spelling of the enumerated
/// set, shared with `Keys::string`/`put` — by content (piccolo `String` is
/// `PartialEq` by bytes, not identity).
fn rd_s<'gc>(v: Value<'gc>, k: &Keys<'gc>, slot: &mut Slot) -> Option<u8> {
    let id = match v {
        Value::String(s) => k
            .strings()
            .iter()
            .position(|t| matches!(t, Value::String(t) if s == *t))
            .map(|i| i as u8),
        _ => None,
    };
    *slot = id.map(Raw::S);
    id
}

/// Write `raw` to `t[key]` unless the mirror says the slot already holds
/// exactly that, then record it.
fn put<'gc>(
    ctx: piccolo::Context<'gc>,
    k: &Keys<'gc>,
    t: Table<'gc>,
    key: Value<'gc>,
    raw: Raw,
    slot: &mut Slot,
) {
    if *slot == Some(raw) {
        return;
    }
    let v = match raw {
        Raw::I(i) => Value::Integer(i),
        Raw::B(b) => Value::Boolean(b),
        Raw::F(bits) => Value::Number(f64::from_bits(bits)),
        Raw::S(i) => k.string(i),
    };
    t.set(ctx, key, v).unwrap();
    *slot = Some(raw);
}

/// The CGWSEL/CGADSUB/COLDATA bytes as of the last install_bindings/
/// write_state — the baseline the friendly `color` fold diffs against.
fn read_color_base<'gc>(
    ctx: piccolo::Context<'gc>,
    k: &Keys<'gc>,
    m: &mut Mirror,
) -> (u8, u8, u16) {
    match ctx.get_global(k.color_base) {
        Value::Table(t) => (
            rd(t.get(ctx, k.cgwsel), &mut m.color_base[0])
                .to_int()
                .unwrap_or(0) as u8,
            rd(t.get(ctx, k.cgadsub), &mut m.color_base[1])
                .to_int()
                .unwrap_or(0) as u8,
            rd(t.get(ctx, k.coldata), &mut m.color_base[2])
                .to_int()
                .unwrap_or(0) as u16,
        ),
        _ => {
            m.color_base = Default::default();
            (0, 0, 0)
        }
    }
}

/// Pack the friendly `color` table into (cgwsel-bits, cgadsub, coldata).
/// Any field that is nil/unrecognized takes its bits from `base` (absent
/// friendly -> the raw register keeps those bits, i.e. "no change").
fn pack_color<'gc>(
    ctx: piccolo::Context<'gc>,
    k: &Keys<'gc>,
    m: &mut Mirror,
    base: (u8, u8, u16),
) -> (u8, u8, u16) {
    let (base_w, base_a, base_c) = base;
    let Value::Table(color) = ctx.get_global(k.color) else {
        m.color = Default::default();
        m.color_on = Default::default();
        return base;
    };
    let bit = |v: Value<'_>, mask: u8, base_byte: u8| -> u8 {
        match v {
            Value::Boolean(true) => mask,
            Value::Boolean(false) => 0,
            _ => base_byte & mask,
        }
    };
    let mut a = match rd_s(color.get(ctx, k.op), k, &mut m.color[0]) {
        Some(S_ADD) => 0,
        Some(S_SUB) => 0x80,
        _ => base_a & 0x80,
    };
    a |= bit(rd(color.get(ctx, k.half), &mut m.color[1]), 0x40, base_a);
    if let Value::Table(on) = color.get(ctx, k.on) {
        for (i, (name, mask)) in k.color_on().into_iter().enumerate() {
            a |= bit(rd(on.get(ctx, name), &mut m.color_on[i]), mask, base_a);
        }
    } else {
        m.color_on = Default::default();
        a |= base_a & 0x3f;
    }
    let mut w = match rd_s(color.get(ctx, k.addend), k, &mut m.color[2]) {
        Some(S_SUB) => 0x02,
        Some(S_FIXED) => 0,
        _ => base_w & 0x02,
    };
    w |= match rd_s(color.get(ctx, k.region), k, &mut m.color[3]) {
        Some(S_EVERYWHERE) => 0x00,
        Some(S_INSIDE) => 0x10,  // prevent-math OUTSIDE the window
        Some(S_OUTSIDE) => 0x20, // prevent-math INSIDE the window
        Some(S_NEVER) => 0x30,   // always prevent
        _ => base_w & 0x30,
    };
    let c = match rd(color.get(ctx, k.fixed), &mut m.color[4]).to_int() {
        Some(v) => (v as u16) & 0x7fff,
        None => base_c,
    };
    (w, a, c)
}

/// Unpack CGWSEL/CGADSUB/COLDATA into the friendly `color` fields (mirror
/// live values so hooks can read them — HDMA persistence, matching m7) and
/// record the bytes in `__color_base` for read_state's change detection.
fn sync_color<'gc>(
    ctx: piccolo::Context<'gc>,
    k: &Keys<'gc>,
    m: &mut Mirror,
    cgwsel: u8,
    cgadsub: u8,
    coldata: u16,
) {
    if let Value::Table(color) = ctx.get_global(k.color) {
        let op = if cgadsub & 0x80 != 0 { S_SUB } else { S_ADD };
        put(ctx, k, color, k.op, Raw::S(op), &mut m.color[0]);
        put(
            ctx,
            k,
            color,
            k.half,
            Raw::B(cgadsub & 0x40 != 0),
            &mut m.color[1],
        );
        if let Value::Table(on) = color.get(ctx, k.on) {
            for (i, (name, mask)) in k.color_on().into_iter().enumerate() {
                put(
                    ctx,
                    k,
                    on,
                    name,
                    Raw::B(cgadsub & mask != 0),
                    &mut m.color_on[i],
                );
            }
        }
        let addend = if cgwsel & 0x02 != 0 { S_SUB } else { S_FIXED };
        put(ctx, k, color, k.addend, Raw::S(addend), &mut m.color[2]);
        let region = match (cgwsel >> 4) & 0x03 {
            0 => S_EVERYWHERE,
            1 => S_INSIDE,
            2 => S_OUTSIDE,
            _ => S_NEVER,
        };
        put(ctx, k, color, k.region, Raw::S(region), &mut m.color[3]);
        let fixed = Raw::I((coldata & 0x7fff) as i64);
        put(ctx, k, color, k.fixed, fixed, &mut m.color[4]);
    }
    if let Value::Table(b) = ctx.get_global(k.color_base) {
        put(
            ctx,
            k,
            b,
            k.cgwsel,
            Raw::I(cgwsel as i64),
            &mut m.color_base[0],
        );
        put(
            ctx,
            k,
            b,
            k.cgadsub,
            Raw::I(cgadsub as i64),
            &mut m.color_base[1],
        );
        let coldata = Raw::I((coldata & 0x7fff) as i64);
        put(ctx, k, b, k.coldata, coldata, &mut m.color_base[2]);
    }
}

/// TM/TS bits owned by the friendly `screen` namespace: bits 0-4 =
/// BG1..BG4, OBJ. Bits 5-7 are unused by hardware — leave them to the raw byte.
const SCREEN_MASK: u8 = 0x1f;

/// The TM/TS bytes as of the last install_bindings/write_state — the
/// baseline the friendly `screen` fold diffs against.
fn read_screen_base<'gc>(ctx: piccolo::Context<'gc>, k: &Keys<'gc>, m: &mut Mirror) -> (u8, u8) {
    match ctx.get_global(k.screen_base) {
        Value::Table(t) => (
            rd(t.get(ctx, k.tm), &mut m.screen_base[0])
                .to_int()
                .unwrap_or(0) as u8,
            rd(t.get(ctx, k.ts), &mut m.screen_base[1])
                .to_int()
                .unwrap_or(0) as u8,
        ),
        _ => {
            m.screen_base = Default::default();
            (0, 0)
        }
    }
}

/// Pack the friendly `screen` table into (tm, ts). Any field that is
/// nil/non-boolean takes its bits from `base` (absent friendly -> the raw
/// register keeps those bits, i.e. "no change").
fn pack_screen<'gc>(
    ctx: piccolo::Context<'gc>,
    k: &Keys<'gc>,
    m: &mut Mirror,
    base: (u8, u8),
) -> (u8, u8) {
    fn side<'gc>(
        ctx: piccolo::Context<'gc>,
        k: &Keys<'gc>,
        slots: &mut [Slot; 5],
        screen: Table<'gc>,
        name: Value<'gc>,
        base_byte: u8,
    ) -> u8 {
        let Value::Table(t) = screen.get(ctx, name) else {
            *slots = Default::default();
            return base_byte & SCREEN_MASK;
        };
        let mut out = 0u8;
        for (i, (field, mask)) in k.screen_layers().into_iter().enumerate() {
            out |= match rd(t.get(ctx, field), &mut slots[i]) {
                Value::Boolean(true) => mask,
                Value::Boolean(false) => 0,
                _ => base_byte & mask,
            };
        }
        out
    }
    let (base_tm, base_ts) = base;
    let Value::Table(screen) = ctx.get_global(k.screen) else {
        m.screen = Default::default();
        return base;
    };
    let [main, sub] = &mut m.screen;
    (
        side(ctx, k, main, screen, k.main, base_tm),
        side(ctx, k, sub, screen, k.sub, base_ts),
    )
}

/// Unpack TM/TS into the friendly `screen` fields (mirror live values so
/// hooks can read them — HDMA persistence, matching `color`) and record the
/// bytes in `__screen_base` for read_state's change detection.
fn sync_screen<'gc>(ctx: piccolo::Context<'gc>, k: &Keys<'gc>, m: &mut Mirror, tm: u8, ts: u8) {
    if let Value::Table(screen) = ctx.get_global(k.screen) {
        for (si, (name, byte)) in [(k.main, tm), (k.sub, ts)].into_iter().enumerate() {
            if let Value::Table(t) = screen.get(ctx, name) {
                for (i, (field, mask)) in k.screen_layers().into_iter().enumerate() {
                    put(
                        ctx,
                        k,
                        t,
                        field,
                        Raw::B(byte & mask != 0),
                        &mut m.screen[si][i],
                    );
                }
            }
        }
    }
    if let Value::Table(b) = ctx.get_global(k.screen_base) {
        put(ctx, k, b, k.tm, Raw::I(tm as i64), &mut m.screen_base[0]);
        put(ctx, k, b, k.ts, Raw::I(ts as i64), &mut m.screen_base[1]);
    }
}

/// Friendly `win` byte order (indexes `WinBytes`, `Keys::win_base_keys`,
/// `WIN_MASKS`): WH0-3, W12SEL, W34SEL, WOBJSEL, WBGLOG, WOBJLOG, TMW, TSW.
type WinBytes = [u8; 11];

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

/// One friendly window layer: its SEL nibble, LOG slot and TMW/TSW bit.
/// `sel`/`log` offset into the WinBytes SEL (4..=6) / LOG (7..=8) bytes;
/// `name` indexes `Keys::win_names` and `Mirror::win`.
struct WinLayer {
    name: usize,
    sel: usize,
    sel_shift: u8,
    log: usize,
    log_shift: u8,
    /// TMW/TSW bit; None for the color window (no mask-enable bit in hw).
    mask_bit: Option<u8>,
}

#[rustfmt::skip]
const WIN_LAYERS: [WinLayer; 6] = [
    WinLayer { name: 0, sel: 0, sel_shift: 0, log: 0, log_shift: 0, mask_bit: Some(0) }, // bg1
    WinLayer { name: 1, sel: 0, sel_shift: 4, log: 0, log_shift: 2, mask_bit: Some(1) }, // bg2
    WinLayer { name: 2, sel: 1, sel_shift: 0, log: 0, log_shift: 4, mask_bit: Some(2) }, // bg3
    WinLayer { name: 3, sel: 1, sel_shift: 4, log: 0, log_shift: 6, mask_bit: Some(3) }, // bg4
    WinLayer { name: 4, sel: 2, sel_shift: 0, log: 1, log_shift: 0, mask_bit: Some(4) }, // obj
    WinLayer { name: 5, sel: 2, sel_shift: 4, log: 1, log_shift: 2, mask_bit: None },    // color
];

/// The eleven window-register bytes as of the last install_bindings/
/// write_state — the baseline the friendly `win` fold diffs against.
fn read_win_base<'gc>(ctx: piccolo::Context<'gc>, k: &Keys<'gc>, m: &mut Mirror) -> WinBytes {
    let mut out = [0u8; 11];
    if let Value::Table(t) = ctx.get_global(k.win_base) {
        for (i, key) in k.win_base_keys().into_iter().enumerate() {
            out[i] = rd(t.get(ctx, key), &mut m.win_base[i])
                .to_int()
                .unwrap_or(0) as u8;
        }
    } else {
        m.win_base = Default::default();
    }
    out
}

/// Pack the friendly `win` table into the eleven register bytes. Any field
/// that is nil/unrecognized takes its bits from `base` (absent friendly ->
/// the raw register keeps those bits, i.e. "no change"). The shared invert
/// pair is base-aware: decode is lossy (either bit set reads true), so an
/// UNCHANGED bool reproduces the base bits verbatim — only a moved bool
/// expands to both bits / neither.
fn pack_win<'gc>(
    ctx: piccolo::Context<'gc>,
    k: &Keys<'gc>,
    m: &mut Mirror,
    base: &WinBytes,
) -> WinBytes {
    let mut out = *base;
    let Value::Table(win) = ctx.get_global(k.win) else {
        m.win_edge = Default::default();
        m.win = Default::default();
        return out;
    };
    let bit = |v: Value<'_>, mask: u8, base_bits: u8| -> u8 {
        match v {
            Value::Boolean(true) => mask,
            Value::Boolean(false) => 0,
            _ => base_bits & mask,
        }
    };
    for (i, (w, edge)) in k.win_edges().into_iter().enumerate() {
        if let Value::Table(t) = win.get(ctx, w) {
            if let Some(v) = rd(t.get(ctx, edge), &mut m.win_edge[i]).to_int() {
                out[i] = v.clamp(0, 255) as u8;
            }
        } else {
            m.win_edge[i] = None;
        }
    }
    let names = k.win_names();
    for l in &WIN_LAYERS {
        let s = &mut m.win[l.name];
        let Value::Table(t) = win.get(ctx, names[l.name]) else {
            *s = Default::default();
            continue;
        };
        let sel_i = 4 + l.sel;
        let base_nib = (base[sel_i] >> l.sel_shift) & 0xf;
        let mut nib = bit(rd(t.get(ctx, k.w1), &mut s[0]), WIN_W1_ENABLE, base_nib)
            | bit(rd(t.get(ctx, k.w2), &mut s[1]), WIN_W2_ENABLE, base_nib);
        nib |= match rd(t.get(ctx, k.invert), &mut s[2]) {
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
        let slot = match rd_s(t.get(ctx, k.combine), k, &mut s[3]) {
            Some(id) if id >= S_OR => id - S_OR,
            _ => base_slot,
        };
        out[log_i] = (out[log_i] & !(0x3 << l.log_shift)) | (slot << l.log_shift);
        if let Some(b) = l.mask_bit {
            let mask = 1u8 << b;
            out[9] = (out[9] & !mask) | bit(rd(t.get(ctx, k.main), &mut s[4]), mask, base[9]);
            out[10] = (out[10] & !mask) | bit(rd(t.get(ctx, k.sub), &mut s[5]), mask, base[10]);
        }
    }
    out
}

/// Unpack the window registers into the friendly `win` fields (mirror live
/// values so hooks can read them — HDMA persistence, matching `color`/
/// `screen`) and record the bytes in `__win_base` for read_state's change
/// detection. Shared decode: `invert` reads true if EITHER invert bit is set.
fn sync_win<'gc>(ctx: piccolo::Context<'gc>, k: &Keys<'gc>, m: &mut Mirror, bytes: &WinBytes) {
    if let Value::Table(win) = ctx.get_global(k.win) {
        for (i, (w, edge)) in k.win_edges().into_iter().enumerate() {
            if let Value::Table(t) = win.get(ctx, w) {
                put(ctx, k, t, edge, Raw::I(bytes[i] as i64), &mut m.win_edge[i]);
            }
        }
        let names = k.win_names();
        for l in &WIN_LAYERS {
            let Value::Table(t) = win.get(ctx, names[l.name]) else {
                continue;
            };
            let s = &mut m.win[l.name];
            let nib = (bytes[4 + l.sel] >> l.sel_shift) & 0xf;
            put(ctx, k, t, k.w1, Raw::B(nib & WIN_W1_ENABLE != 0), &mut s[0]);
            put(ctx, k, t, k.w2, Raw::B(nib & WIN_W2_ENABLE != 0), &mut s[1]);
            put(
                ctx,
                k,
                t,
                k.invert,
                Raw::B(nib & WIN_INVERT_BITS != 0),
                &mut s[2],
            );
            let slot = (bytes[7 + l.log] >> l.log_shift) & 0x3;
            put(ctx, k, t, k.combine, Raw::S(S_OR + slot), &mut s[3]);
            if let Some(b) = l.mask_bit {
                put(
                    ctx,
                    k,
                    t,
                    k.main,
                    Raw::B(bytes[9] & (1 << b) != 0),
                    &mut s[4],
                );
                put(
                    ctx,
                    k,
                    t,
                    k.sub,
                    Raw::B(bytes[10] & (1 << b) != 0),
                    &mut s[5],
                );
            }
        }
    }
    if let Value::Table(base) = ctx.get_global(k.win_base) {
        for (i, key) in k.win_base_keys().into_iter().enumerate() {
            put(
                ctx,
                k,
                base,
                key,
                Raw::I(bytes[i] as i64),
                &mut m.win_base[i],
            );
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
/// The `cgram` table as frame() left it: one slot per entry, None = unset.
fn snapshot_cgram<'gc>(ctx: piccolo::Context<'gc>, k: &Keys<'gc>) -> [Option<i64>; 256] {
    let mut out = [None; 256];
    if let Value::Table(cg) = ctx.get_global(k.cgram) {
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = cg.get(ctx, i as i64).to_int();
        }
    }
    out
}

/// After a hook ran for one line: every `cgram[i]` that differs from the
/// frame snapshot is that line's poke, and the table is put back so the
/// write reaches neither the next line nor the next frame. A hook setting
/// an entry to nil is "no override".
fn take_cgram_pokes<'gc>(
    ctx: piccolo::Context<'gc>,
    k: &Keys<'gc>,
    snap: &[Option<i64>; 256],
) -> Vec<(u8, u16)> {
    let mut pokes = Vec::new();
    let Value::Table(cg) = ctx.get_global(k.cgram) else {
        return pokes;
    };
    // 256 direct probes. A live-entry walk (iterate the table, then restore
    // the snapshot entries it missed) was measured slower on the film drafts,
    // whose frame() fills all 256 entries: piccolo's `next` is dearer than a
    // `get` per slot and the second pass is an extra 256 anyway. It only won
    // on toys with a sparse cgram, which are already far under budget.
    for (i, was) in snap.iter().enumerate() {
        let now = cg.get(ctx, i as i64).to_int();
        if now == *was {
            continue;
        }
        if let Some(c) = now {
            pokes.push((i as u8, (c as u16) & 0x7fff));
        }
        cg.set(ctx, i as i64, was.map_or(Value::Nil, Value::Integer))
            .unwrap();
    }
    pokes
}

fn read_state<'gc>(ctx: piccolo::Context<'gc>, k: &Keys<'gc>, m: &mut Mirror) -> LineTableRow {
    let mut row = LineTableRow::default();
    let g = ctx.globals();
    row.bg3_priority = rd(g.get(ctx, k.bg3_priority), &mut m.bg3_priority).to_bool();
    if let Some(v) = rd(g.get(ctx, k.mode), &mut m.mode).to_int() {
        row.mode = v as u8; // wrap; quantize::mode masks to 3 bits at build
    }
    if let Some(b) = rd(g.get(ctx, k.brightness), &mut m.brightness).to_int() {
        row.brightness = b as u8; // wrap; quantize::brightness masks to 4 bits
    }
    if let Some(v) = rd(g.get(ctx, k.TM), &mut m.tm).to_int() {
        row.tm = v as u8; // wrap; quantize::screen_mask masks to 5 bits at build
    }
    if let Some(v) = rd(g.get(ctx, k.TS), &mut m.ts).to_int() {
        row.ts = v as u8;
    }
    // Friendly `screen.*` fold — same coexistence contract as the `color`
    // fold below: XOR the packed friendly fields against the `__screen_base`
    // baseline recorded by install_bindings/write_state. Bits the user moved
    // via screen.main/.sub are authoritative (set AND clear, and beat a
    // same-cycle raw write); untouched bits keep the raw TM/TS byte, so
    // raw-only scripts (incl. inside hooks) and the both-off power-on state
    // stay byte-identical.
    let sbase = read_screen_base(ctx, k, m);
    let (f_tm, f_ts) = pack_screen(ctx, k, m, sbase);
    let changed_tm = (f_tm ^ sbase.0) & SCREEN_MASK;
    row.tm = (row.tm & !changed_tm) | (f_tm & changed_tm);
    let changed_ts = (f_ts ^ sbase.1) & SCREEN_MASK;
    row.ts = (row.ts & !changed_ts) | (f_ts & changed_ts);
    let mut wrow = row_win_bytes(&row);
    for (i, key) in k.win_raw_keys().into_iter().enumerate() {
        if let Some(v) = rd(g.get(ctx, key), &mut m.win_raw[i]).to_int() {
            wrow[i] = v as u8;
        }
    }
    // Friendly `win.*` fold — same coexistence contract as the `screen`/
    // `color` folds: XOR the packed friendly bytes against the `__win_base`
    // baseline recorded by install_bindings/write_state; only bits the user
    // moved through `win` override the raw mnemonics (set AND clear, and
    // beat a same-cycle raw write), masked so raw-only bits (WOBJLOG 4-7,
    // TMW/TSW 5-7) pass through untouched. Exception: the WH edges (indices
    // 0..3) are scalar coordinates, not bitfields — a moved friendly edge
    // replaces the byte whole (the COLDATA precedent), never a bitwise blend.
    let wbase = read_win_base(ctx, k, m);
    let fwin = pack_win(ctx, k, m, &wbase);
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
    if let Some(v) = rd(g.get(ctx, k.CGWSEL), &mut m.cgwsel).to_int() {
        row.cgwsel = v as u8;
    }
    // Friendly alias: direct_color=true forces CGWSEL bit 0 (raw CGWSEL still works;
    // OR keeps both authoring styles valid and both-off byte-identical).
    if rd(g.get(ctx, k.direct_color), &mut m.direct_color).to_bool() {
        row.cgwsel |= 0x01;
    }
    row.force_blank = rd(g.get(ctx, k.force_blank), &mut m.force_blank).to_bool();
    if let Some(v) = rd(g.get(ctx, k.CGADSUB), &mut m.cgadsub).to_int() {
        row.cgadsub = v as u8;
    }
    if let Some(v) = rd(g.get(ctx, k.COLDATA), &mut m.coldata).to_int() {
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
    let base = read_color_base(ctx, k, m);
    let (f_w, f_a, f_c) = pack_color(ctx, k, m, base);
    let changed_w = (f_w ^ base.0) & COLOR_CGWSEL_MASK;
    row.cgwsel = (row.cgwsel & !changed_w) | (f_w & changed_w);
    let changed_a = f_a ^ base.1;
    row.cgadsub = (row.cgadsub & !changed_a) | (f_a & changed_a);
    if f_c != base.2 {
        row.coldata = f_c;
    }
    if let Some(v) = rd(g.get(ctx, k.mosaic), &mut m.mosaic).to_int() {
        row.mosaic_size = v as u8; // wrap; quantize::mosaic_size masks to 4 bits at build
    }
    if let Value::Table(bg) = ctx.get_global(k.bg) {
        for i in 0..4 {
            let s = &mut m.bg[i];
            let Value::Table(layer) = bg.get(ctx, (i + 1) as i64) else {
                *s = Default::default();
                continue;
            };
            if let Value::Table(scroll) = layer.get(ctx, k.scroll) {
                if let Some(x) = rd(scroll.get(ctx, k.x), &mut s[0]).to_number() {
                    row.bg[i].scroll_x = x as f32;
                }
                if let Some(y) = rd(scroll.get(ctx, k.y), &mut s[1]).to_number() {
                    row.bg[i].scroll_y = y as f32;
                }
            } else {
                s[0] = None;
                s[1] = None;
            }
            row.bg[i].visible = match rd(layer.get(ctx, k.visible), &mut s[2]) {
                Value::Nil => true,
                v => v.to_bool(),
            };
            // Binding registers (quantize-on-write at RegRow build time).
            if let Some(v) = rd(layer.get(ctx, k.tile_size), &mut s[3]).to_int() {
                row.bg[i].tile_size = v as u8;
            }
            if let Some(v) = rd(layer.get(ctx, k.map_base), &mut s[4]).to_int() {
                row.bg[i].map_base = v as u32;
            }
            if let Some(v) = rd(layer.get(ctx, k.screen_size), &mut s[5]).to_int() {
                row.bg[i].screen_size = v as u8;
            }
            if let Some(v) = rd(layer.get(ctx, k.char_base), &mut s[6]).to_int() {
                row.bg[i].char_base = v as u32;
            }
            // MOSAIC per-BG enable; unset/nil -> false (off, matches default).
            row.mosaic_enable[i] = rd(layer.get(ctx, k.mosaic), &mut s[7]).to_bool();
        }
    } else {
        m.bg = Default::default();
    }
    if let Value::Table(m7) = ctx.get_global(k.m7) {
        let s = &mut m.m7;
        let num = |key: Value<'gc>, slot: &mut Slot| rd(m7.get(ctx, key), slot).to_number();
        if let Some(v) = num(k.a, &mut s[0]) {
            row.m7.a = v as f32;
        }
        if let Some(v) = num(k.b, &mut s[1]) {
            row.m7.b = v as f32;
        }
        if let Some(v) = num(k.c, &mut s[2]) {
            row.m7.c = v as f32;
        }
        if let Some(v) = num(k.d, &mut s[3]) {
            row.m7.d = v as f32;
        }
        if let Some(v) = num(k.cx, &mut s[4]) {
            row.m7.cx = v as f32;
        }
        if let Some(v) = num(k.cy, &mut s[5]) {
            row.m7.cy = v as f32;
        }
        // M7SEL binding registers (`wrap` = spec's `m7.repeat`, keyword-renamed).
        if let Some(v) = rd(m7.get(ctx, k.wrap), &mut s[6]).to_int() {
            row.m7.repeat = v as u8;
        }
        row.m7.flip_x = rd(m7.get(ctx, k.flip_x), &mut s[7]).to_bool();
        row.m7.flip_y = rd(m7.get(ctx, k.flip_y), &mut s[8]).to_bool();
        // SETINI.6 EXTBG: fold the DSL bool into the register byte's bit 6.
        let extbg = rd(m7.get(ctx, k.extbg), &mut s[9]).to_bool();
        row.setini = (row.setini & !0x40) | ((extbg as u8) << 6);
    } else {
        m.m7 = Default::default();
    }
    row
}

/// Write a `LineTableRow` back into the per-scanline register globals (used to
/// re-baseline globals before each hook and to restore sticky state after
/// build). Diff-on-write: a global the mirror says already holds exactly the
/// target raw value is skipped — see [`Mirror`]; the caller resets the mirror
/// wherever globals were written behind its back.
fn write_state<'gc>(ctx: piccolo::Context<'gc>, k: &Keys<'gc>, m: &mut Mirror, row: &LineTableRow) {
    // Diff-on-write assumes every mirrored table is reached by exactly one
    // path (bg[1].scroll, screen.main, __win_base, ...). A hook that aliases
    // two of those paths to the same table — `bg[1] = bg[2]`, `screen.main =
    // screen.sub`, `win.bg1 = win.bg2`, `bg[1] = _G` — breaks that: `put`
    // writes the first path, then skips the second because the shared
    // table's slot already matches, leaving the aliased table holding the
    // first path's value instead of the old code's last-path-wins result.
    // Detect any aliasing among the ~25 tables this function and its sync_*
    // helpers write into (a cheap O(n^2) scan — `Table`'s `PartialEq` is
    // `Gc::ptr_eq`, i.e. identity) and, if found, wipe the mirror so every
    // `put` below is a full write — exact by construction, since a full
    // write is what the pre-mirror code always did regardless of aliasing.
    {
        let g = ctx.globals();
        let get_t = |t: Table<'gc>, key: Value<'gc>| match t.get(ctx, key) {
            Value::Table(t) => Some(t),
            _ => None,
        };
        let screen = get_t(g, k.screen);
        let win = get_t(g, k.win);
        let bg_tbl = get_t(g, k.bg);
        let color = get_t(g, k.color);
        let bg1 = bg_tbl.and_then(|t| get_t(t, Value::Integer(1)));
        let bg2 = bg_tbl.and_then(|t| get_t(t, Value::Integer(2)));
        let bg3 = bg_tbl.and_then(|t| get_t(t, Value::Integer(3)));
        let bg4 = bg_tbl.and_then(|t| get_t(t, Value::Integer(4)));
        let tables: [Option<Table<'gc>>; 25] = [
            Some(g),
            get_t(g, k.screen_base),
            get_t(g, k.win_base),
            get_t(g, k.color_base),
            screen.and_then(|t| get_t(t, k.main)),
            screen.and_then(|t| get_t(t, k.sub)),
            win.and_then(|t| get_t(t, k.w1)),
            win.and_then(|t| get_t(t, k.w2)),
            win.and_then(|t| get_t(t, k.bg1)),
            win.and_then(|t| get_t(t, k.bg2)),
            win.and_then(|t| get_t(t, k.bg3)),
            win.and_then(|t| get_t(t, k.bg4)),
            win.and_then(|t| get_t(t, k.obj)),
            win.and_then(|t| get_t(t, k.color)),
            color,
            color.and_then(|t| get_t(t, k.on)),
            bg1,
            bg2,
            bg3,
            bg4,
            bg1.and_then(|t| get_t(t, k.scroll)),
            bg2.and_then(|t| get_t(t, k.scroll)),
            bg3.and_then(|t| get_t(t, k.scroll)),
            bg4.and_then(|t| get_t(t, k.scroll)),
            get_t(g, k.m7),
        ];
        let mut aliased = false;
        'outer: for i in 0..tables.len() {
            if let Some(a) = tables[i] {
                for b in tables[i + 1..].iter().flatten() {
                    if a == *b {
                        aliased = true;
                        break 'outer;
                    }
                }
            }
        }
        if aliased {
            *m = Mirror::default();
        }
    }
    let g = ctx.globals();
    put(ctx, k, g, k.mode, Raw::I(row.mode as i64), &mut m.mode);
    put(
        ctx,
        k,
        g,
        k.bg3_priority,
        Raw::B(row.bg3_priority),
        &mut m.bg3_priority,
    );
    put(
        ctx,
        k,
        g,
        k.brightness,
        Raw::I(row.brightness as i64),
        &mut m.brightness,
    );
    put(ctx, k, g, k.TM, Raw::I(row.tm as i64), &mut m.tm);
    put(ctx, k, g, k.TS, Raw::I(row.ts as i64), &mut m.ts);
    sync_screen(ctx, k, m, row.tm, row.ts);
    let wrow = row_win_bytes(row);
    for (i, key) in k.win_raw_keys().into_iter().enumerate() {
        put(ctx, k, g, key, Raw::I(wrow[i] as i64), &mut m.win_raw[i]);
    }
    sync_win(ctx, k, m, &wrow);
    put(
        ctx,
        k,
        g,
        k.CGWSEL,
        Raw::I(row.cgwsel as i64),
        &mut m.cgwsel,
    );
    put(
        ctx,
        k,
        g,
        k.CGADSUB,
        Raw::I(row.cgadsub as i64),
        &mut m.cgadsub,
    );
    put(
        ctx,
        k,
        g,
        k.COLDATA,
        Raw::I(row.coldata as i64),
        &mut m.coldata,
    );
    put(
        ctx,
        k,
        g,
        k.mosaic,
        Raw::I(row.mosaic_size as i64),
        &mut m.mosaic,
    );
    let direct_color = Raw::B((row.cgwsel & 0x01) != 0);
    put(ctx, k, g, k.direct_color, direct_color, &mut m.direct_color);
    sync_color(ctx, k, m, row.cgwsel, row.cgadsub, row.coldata);
    put(
        ctx,
        k,
        g,
        k.force_blank,
        Raw::B(row.force_blank),
        &mut m.force_blank,
    );
    if let Value::Table(bg) = ctx.get_global(k.bg) {
        for i in 0..4 {
            if let Value::Table(layer) = bg.get(ctx, (i + 1) as i64) {
                let s = &mut m.bg[i];
                let r = &row.bg[i];
                if let Value::Table(scroll) = layer.get(ctx, k.scroll) {
                    put(
                        ctx,
                        k,
                        scroll,
                        k.x,
                        Raw::F((r.scroll_x as f64).to_bits()),
                        &mut s[0],
                    );
                    put(
                        ctx,
                        k,
                        scroll,
                        k.y,
                        Raw::F((r.scroll_y as f64).to_bits()),
                        &mut s[1],
                    );
                }
                put(ctx, k, layer, k.visible, Raw::B(r.visible), &mut s[2]);
                put(
                    ctx,
                    k,
                    layer,
                    k.tile_size,
                    Raw::I(r.tile_size as i64),
                    &mut s[3],
                );
                put(
                    ctx,
                    k,
                    layer,
                    k.map_base,
                    Raw::I(r.map_base as i64),
                    &mut s[4],
                );
                put(
                    ctx,
                    k,
                    layer,
                    k.screen_size,
                    Raw::I(r.screen_size as i64),
                    &mut s[5],
                );
                put(
                    ctx,
                    k,
                    layer,
                    k.char_base,
                    Raw::I(r.char_base as i64),
                    &mut s[6],
                );
                put(
                    ctx,
                    k,
                    layer,
                    k.mosaic,
                    Raw::B(row.mosaic_enable[i]),
                    &mut s[7],
                );
            }
        }
    }
    if let Value::Table(m7) = ctx.get_global(k.m7) {
        let s = &mut m.m7;
        let r = &row.m7;
        let f = |v: f32| Raw::F((v as f64).to_bits());
        put(ctx, k, m7, k.a, f(r.a), &mut s[0]);
        put(ctx, k, m7, k.b, f(r.b), &mut s[1]);
        put(ctx, k, m7, k.c, f(r.c), &mut s[2]);
        put(ctx, k, m7, k.d, f(r.d), &mut s[3]);
        put(ctx, k, m7, k.cx, f(r.cx), &mut s[4]);
        put(ctx, k, m7, k.cy, f(r.cy), &mut s[5]);
        put(ctx, k, m7, k.wrap, Raw::I(r.repeat as i64), &mut s[6]);
        put(ctx, k, m7, k.flip_x, Raw::B(r.flip_x), &mut s[7]);
        put(ctx, k, m7, k.flip_y, Raw::B(r.flip_y), &mut s[8]);
        put(
            ctx,
            k,
            m7,
            k.extbg,
            Raw::B(row.setini & 0x40 != 0),
            &mut s[9],
        );
    }
}

/// `found` label for a `dma()` placement whose source is gone from the store —
/// a removed source deserves a diagnostic, not nothing. (An init-time typo is
/// a loud `dma` error; this covers removal AFTER placement.)
const MISSING_SOURCE: &str = "no source with this name";

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

/// Flush the DSL memory-poke surfaces into `Memory`. Runs AFTER the dma replay
/// has laid down the init placements, so manual pokes compose on top under
/// last-write-wins. Order: structured tilemap/char pokes, then the raw `vram[]`
/// table as the FINAL authority (a raw word write always wins). `cgram[]` is
/// applied as an override on top of the placed palette (only set entries).
/// VRAM is NOT zeroed here — `frame()` zeroes it before the replay.
fn read_memory(ctx: piccolo::Context<'_>, mem: &mut Memory) {
    // cgram[] overrides the placed palette: apply only the entries the user
    // actually set, so a dma placement's colors survive where unpoked.
    // (mem.cgram was zeroed and any placed palette written before this runs.)
    if let Value::Table(cg) = ctx.get_global("cgram") {
        for (k, v) in cg {
            if let (Some(i), Some(c)) = (k.to_int(), v.to_int()) {
                if (0..256).contains(&i) {
                    mem.cgram[i as usize] = (c as u16) & 0x7fff;
                }
            }
        }
    }
    if let Value::Table(obj) = ctx.get_global("obj") {
        mem.obsel.char_base = crate::quantize::obj_char_base(
            obj.get(ctx, "char_base").to_int().unwrap_or(0).max(0) as u32,
        );
        mem.obsel.size_sel =
            crate::quantize::obj_size_sel(obj.get(ctx, "size_sel").to_int().unwrap_or(0) as u8);
        mem.obsel.name_select = crate::quantize::obj_name_select(
            obj.get(ctx, "name_select").to_int().unwrap_or(0) as u8,
        );
        mem.priority_rotate = obj.get(ctx, "priority_rotate").to_bool();
        mem.oam_addr = (obj.get(ctx, "oam_addr").to_int().unwrap_or(0).max(0) as u16) & 0x1ff;
        // Friendly sugar: obj.first = N turns rotation on and points OAMADD at
        // sprite N (word address N<<1). Overrides the raw fields when present.
        if let Some(n) = obj.get(ctx, "first").to_int() {
            mem.priority_rotate = true;
            mem.oam_addr = ((n.max(0) as u16 & 0x7f) << 1) & 0x1ff;
        }
        for i in 0..128 {
            if let Value::Table(o) = obj.get(ctx, i as i64) {
                let e = &mut mem.oam[i];
                e.x = crate::quantize::sprite_x(o.get(ctx, "x").to_number().unwrap_or(0.0) as f32);
                e.y = crate::quantize::sprite_y(o.get(ctx, "y").to_number().unwrap_or(0.0) as f32);
                e.tile = o.get(ctx, "tile").to_int().unwrap_or(0) as u16;
                e.pal = o.get(ctx, "pal").to_int().unwrap_or(0) as u8;
                e.prio = o.get(ctx, "prio").to_int().unwrap_or(0) as u8;
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
                layer.get(ctx, "map_base").to_int().unwrap_or(0) as u32,
            );
            let screen_size = crate::quantize::bg_screen_size(
                layer.get(ctx, "screen_size").to_int().unwrap_or(0) as u8,
            );
            let Value::Table(map) = layer.get(ctx, "map") else {
                continue;
            };
            for (ck, cv) in map {
                let (Some(col), Value::Table(rowt)) = (ck.to_int(), cv) else {
                    continue;
                };
                for (rk, rv) in rowt {
                    let (Some(row_i), Value::Table(cell)) = (rk.to_int(), rv) else {
                        continue;
                    };
                    let tile = cell.get(ctx, "tile").to_int().unwrap_or(0) as u16 & 0x03ff;
                    let pal = cell.get(ctx, "pal").to_int().unwrap_or(0) as u16 & 0x07;
                    let prio = cell.get(ctx, "prio").to_int().unwrap_or(0) as u16 & 0x01;
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
                let (Some(ty), Value::Table(rowt)) = (yk.to_int(), yv) else {
                    continue;
                };
                for (xk, xv) in rowt {
                    if let (Some(tx), Some(tile)) = (xk.to_int(), xv.to_int()) {
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
            let (Some(tile), Value::Table(pix)) = (tk.to_int(), tv) else {
                continue;
            };
            for (pk, pv) in pix {
                if let (Some(off), Some(idx)) = (pk.to_int(), pv.to_int()) {
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
            if let (Some(addr), Some(word)) = (k.to_int(), v.to_int()) {
                if (0..0x8000).contains(&addr) {
                    mem.vram[addr as usize] = word as u16;
                }
            }
        }
    }
}

#[cfg(test)]
mod alias_tests {
    use super::*;

    /// F1: `bg[1] = bg[2]` inside an `hdma` hook aliases the two tables
    /// `write_state` mirrors independently. Without the aliasing guard,
    /// diff-on-write's `put` writes bg[1]'s path first and then skips
    /// bg[2]'s path because the (now-shared) table's slot already matches,
    /// so bg[2]'s own values never land. The pre-mirror code always fully
    /// rewrote every path in order, so the correct result — reproduced here
    /// — is last-path-wins: once aliased, both `bg[1]` and `bg[2]` read back
    /// as bg[2]'s values.
    #[test]
    fn aliased_bg_tables_resolve_last_path_wins() {
        let mut e = LuaEngine::new();
        e.set_sources(&[(
            "main.lua",
            r#"
function frame(t, f)
  bg[1].scroll.x = 10
  bg[2].scroll.x = 20
  bg[2].tile_size = 16
  hdma(0, 223, function(y) bg[1] = bg[2] end)
end
"#,
        )])
        .unwrap();
        let lt = e.frame(0.0, 0).unwrap();
        for y in [1usize, 100] {
            let row = &lt.rows[y];
            assert_eq!(row.bg[0].scroll_x, 20, "row {y} bg[1] (aliased to bg[2])");
            assert_eq!(row.bg[0].tile_size, 16, "row {y} bg[1] tile_size");
            assert_eq!(row.bg[1].scroll_x, 20, "row {y} bg[2]");
            assert_eq!(row.bg[1].tile_size, 16, "row {y} bg[2] tile_size");
        }
    }
}
