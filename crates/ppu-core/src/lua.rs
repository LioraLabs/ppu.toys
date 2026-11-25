//! piccolo Lua VM + flat-global DSL binding. Runs `frame(t,f)` once to populate
//! frame-wide defaults + CGRAM/OAM, registers `hdma` hooks, then (Phase A)
//! applies the live pokes chunk's `__ppu_pokes` as a synthetic frame-wide `hdma`
//! hook spliced in after the program's own hooks, then resolves the
//! LineTable by invoking each covering hook per scanline (later call wins).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use piccolo::{
    Callback, CallbackReturn, Closure, Executor, FromMultiValue, Function, Lua, PrototypeError,
    StashedFunction, StashedTable, StaticError, Table, Value,
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
    /// The `dma()` result's other integer fields (`next_char`, `bit_depth`, ...),
    /// so the SNES compiler resolves them as the preview does.
    pub fields: Vec<(&'static str, i64)>,
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
    /// Seconds this voice's note lasts with no key-off on the way: it was keyed on by `kon()` (not
    /// `sfx()`) on a looping sample and not keyed off since. `f64::INFINITY` when it holds forever;
    /// absent otherwise. Only [`LuaEngine::dsp_view`] fills it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sustains: Option<f64>,
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
/// [`LuaEngine::score_view`]. `tick` is the next tick to play, `0..length-1`
/// (`length` for a finished `loop = false` song; a loop wraps to `loop_tick`);
/// `song` is the name of the song source `score{ song = "<name>" }` plays.
/// `slot`/`step` are the arrangement slot `tick` falls in and the GLOBAL
/// 16th step within it — the same index `song_analyze::Analysis::slot_steps`
/// uses — from `Timing::position` (`None` when `tick == length`: a finished
/// song, or a `loop = false` one in the window before its finishing fire —
/// see `finished`).
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct ScoreView {
    pub tick: i64,
    pub length: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub song: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<u32>,
    /// The timeline time (seconds) the song's tick 0 is anchored to: the
    /// anchor every jump (seek, scrub, loop wrap) resolves position against.
    /// Tick `k` of loop pass `p` plays at `anchor + (unwrap(p, k) + 1) / 250`
    /// s ([`crate::song::Timing::unwrap`]; pass 0 is `k` itself). A live
    /// reload keeps musical position by playing against a whole-tick-shifted
    /// anchor until the next jump; the view does not expose that shift
    /// (the staff position comes from `tick`/`slot`/`step`). `None` until a
    /// `play()` is anchored at the next rendered frame.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<f64>,
    /// The tick the looped body restarts at (the loop-start slot's first
    /// tick), when the song loops; `tick` wraps to here, not to 0.
    #[serde(rename = "loopTick", skip_serializing_if = "Option::is_none")]
    pub loop_tick: Option<i64>,
    /// A `loop = false` song that played to its end (anchored, neither
    /// looping nor playing). It keeps its `anchor` so the studio can map
    /// steps to time and seek back into it; `tick` is `length` and
    /// `slot`/`step` are `None`. A playing `loop = false` song can also
    /// report `tick == length` with no `slot`/`step` (and `finished` false)
    /// for the one 4 ms window right before its finishing fire.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub finished: bool,
}

/// The label errors from the live pokes chunk carry (see [`LuaEngine::set_pokes`]): not a file.
const POKES: &str = "pokes";

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
        self.file.get_or_insert_with(|| name.to_string());
        self
    }
}

/// The embedded Lua VM plus the captured `frame`/`init` entry points and mirrored
/// PPU memory. Globals persist across frames (sticky registers).
/// Video frames per update a `---@snes frames N` line (N from 1 to 4) declares in any of `files`; 1 without one.
fn frames_note(files: &[(&str, &str)]) -> u32 {
    files
        .iter()
        .flat_map(|(_, source)| source.lines())
        .find_map(
            |line| match line.split_whitespace().collect::<Vec<_>>()[..] {
                ["---@snes", "frames", n] => n.parse().ok().filter(|n| (1..=4).contains(n)),
                _ => None,
            },
        )
        .unwrap_or(1)
}

pub struct LuaEngine {
    lua: Rc<RefCell<Lua>>,
    /// The last good load's VM and main chunks, which the next load carries state from; `None`
    /// at a power-on (`reset`, [`Self::forget_run`], a new engine).
    prev_run: Option<crate::carry::Run>,
    /// Variables the last load left at their new initial value ([`Self::not_carried`]).
    not_carried: Vec<String>,
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
    /// The program calls `sprites.draw`: each frame() ends with the library's flush.
    sprites: bool,
    /// The pokes chunk's `__ppu_pokes`, stashed from the tracked
    /// `__ppu_controls_env` (see [`Self::load_controls`]). `None` without
    /// one: `frame()`'s Phase A is then a no-op.
    controls_fn: Option<StashedFunction>,
    /// Painted tiles captured from the pokes chunk's `__ppu_pokes_vram()` at load;
    /// overlaid onto VRAM every frame after all Lua has run.
    controls_vram: Vec<(u16, u16)>,
    /// The Studio's live pokes (see [`Self::set_pokes`]): a chunk loaded first on every load,
    /// never part of the program's files.
    pokes: String,
    memory: Memory,
    /// The source store: decoded `addSource` payloads keyed by name — the
    /// graphics-data home (kind + depth + palettes + tiles + tilemap all
    /// self-described by the payload). App-level; survives recompiles (NOT
    /// cleared by set_sources, which owns Lua files). Shared with the `dma`
    /// callback installed in the VM (init-time name/kind validation).
    source_store: Rc<RefCell<HashMap<String, crate::source::SourcePayload>>>,
    /// Each sheet source's palette per char (its `SourceMeta.cells`), which the payload does not
    /// carry; `world.load` needs it to colour a map. Survives recompiles, like the store.
    sheet_pals: Rc<RefCell<HashMap<String, Vec<u8>>>>,
    /// The program's project files (`frontend::is_project_file`: Tiled maps and tilesets), by
    /// name, as the last `set_sources` gave them.
    project_files: Rc<RefCell<HashMap<String, String>>>,
    /// Placements recorded by `dma()` during the current program's init
    /// (top-level chunks + `init()`), replayed into VRAM/CGRAM each frame.
    /// Swapped wholesale on a successful `set_sources` (recompile).
    dma: Rc<DmaRecorder>,
    /// Per-layer import/diagnostic reports produced by the most recent `frame()`.
    reports: Vec<ImportBudget>,
    program_sources: Vec<(String, String)>,
    source_dirty: bool,
    /// Run fixed-point arithmetic even without a makeFixed contract in the sources.
    fixed_point: bool,
    /// Off: `frame()` leaves `memory()` as it was (the Lua tables' CGRAM, OAM and VRAM pokes unread).
    picture_memory: bool,
    /// Controller state for the next frame: a PAD_* bitmask mirrored into the
    /// Lua `pad` table before frame() runs. JS owns the key/gamepad mapping.
    pad: u16,
    pad2: u16,
    /// The last frame() run for `pad.pressed`/`pad.released`: its f, both masks, and the masks it compared
    /// with, so a repeat of that frame sees the same edges. None at the program's and each scene's start,
    /// whose first frame reports no edges (a button held across a switch is not a press).
    pads_before: Option<(u32, (u16, u16), (u16, u16))>,
    /// The live S-DSP core (M12/audio). Survives recompiles and `Memory::new()`
    /// resets — only `render_frame_audio` writes to it.
    dsp: Dsp,
    /// The 64 KB ARAM the DSP renders from/into. Boxed so `LuaEngine` doesn't
    /// carry a 64 KB inline array. Survives recompiles. Its writers: the
    /// `aram[]` Lua table (drained once per frame), sample placement at a
    /// successful compile (`set_sources`), the echo-region clear on a jump
    /// (`render_frame_audio`), and the echo unit itself.
    aram: Box<[u8; 0x10000]>,
    /// The most recently rendered frame's interleaved stereo audio (L,R per
    /// sample). Empty before the first `frame()`.
    audio: Vec<i16>,
    /// The last `f` whose audio span was rendered (`None` after `new()`/
    /// `reset()`). A repeat of it renders no audio, unless it is the first
    /// frame after a recompile (`recompiled`). Every rendered span
    /// starts with `realign`; when `last_f` is `Some` and `f` isn't
    /// `last + 1`, that is a jump, which also keys off the anchored songs
    /// and clears the echo buffer. The first frame after `new()`/`reset()`
    /// is not a key-off jump (nothing is sounding yet).
    last_f: Option<u32>,
    /// Set by a successful full `set_sources` (not the controls-only fast
    /// path, which runs no `init()`), cleared by every rendered span. While
    /// set, `frame(t, last_f)` is a replay, not an inert repeat: span
    /// `last_f` renders again so `init()`'s and the new `frame()`'s keys
    /// (and a recompile's song key-offs) reach the chip — not a jump, so no
    /// song key-off and no echo clear.
    recompiled: bool,
    /// The host is paused ([`Self::set_hold`]): an update captures the program's file-level state
    /// before it runs, and the same update drawn again (a poke, a panel's change) starts from it.
    hold: bool,
    /// The update `held` was captured before, and that state.
    held: Option<(u32, crate::carry::Snap)>,
    /// `timer(n, div, fn)` hooks registered by the current program's init
    /// window (top-level chunks + `init()`). Dropped and re-registered
    /// wholesale on every `set_sources`, then re-phased onto the timeline at
    /// the current position (see `realign`).
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
    /// The program's scenes (`scenes.add`), or `None` for one implied scene.
    scenes: Option<Scenes>,
    /// The scene file every load boots into instead of the program's own boot scene (the Studio's
    /// scene switcher); a file that is no scene of the program leaves the normal boot.
    start_scene: Option<String>,
    /// Booting into `start_scene` is a Scene view entry, which runs its `preview`.
    start_preview: bool,
    /// The project's update target (`targetHz`: 25 or 30 run one update every second video frame).
    target_hz: Option<u8>,
    /// The run frame (from power-on) the host's `f = 0` stands for: a host that counts a scene it
    /// entered from 0 ([`Self::enter_pending_scene`]) names its entry here, so t rounds as the cart's.
    run_base: u32,
    /// Video frames per update a `---@snes frames N` line in the sources declares (1 without one).
    frames_note: u32,
    /// The last update run between updates of a program slower than the video rate: its program
    /// frame and what it returned, which the video frames up to the next update show again.
    shown: Option<(u32, Result<LineTable, LuaError>)>,
    /// What the VM's contract checks found (bounded.rs), not yet stamped with scene and frame.
    violation_sink: crate::bounded::Violations,
    /// Broken contract stores since the last `reset`, one per file:line and store.
    violations: Vec<crate::bounded::Violation>,
    /// The program frame of the update running, which stamps its violations.
    update_f: u32,
}

/// One `scenes.add(require("./x"))`: its module file and the functions the module returned.
struct Scene {
    file: String,
    init: Option<StashedFunction>,
    frame: StashedFunction,
    frame_file: Option<String>,
    /// The module's `preview`: run only on a Scene view entry (the switcher's start scene).
    preview: Option<StashedFunction>,
}

/// luau.lua's fade state (`scenes.go(s, {fade = n})`) and its pending switch (set the update a fade-out ends),
/// which an edit that stays in the running scene carries.
const SCENE_FADE: [&str; 6] = ["__ppu_scene_next", "__ppu_scene_out", "__ppu_scene_in", "__ppu_scene_fade", "__ppu_scene_after", "__ppu_scene_shown"];

/// A program of scenes. Entering one is a cold boot (see [`LuaEngine::enter_scene`]): its frame()
/// and the audio timeline count from the update it was entered on, `f0`.
struct Scenes {
    list: Vec<Scene>,
    /// The scene the program boots into: the first added, or one a file-level `scenes.go` chose.
    boot: usize,
    /// The scene a load enters: `boot`, or the one a recompile found running.
    entry: usize,
    /// `boot` is the switcher's start scene: entering it is a Scene view entry, which runs its preview.
    chosen: bool,
    current: usize,
    f0: u32,
    t0: f64,
    /// What the program's load recorded, every scene's (each entry names its owner).
    placements: Vec<DmaPlacement>,
    timers: Vec<TimerReg>,
    /// The recorder's lengths after the load: an entry's init records past them.
    ranges: (usize, usize),
    obj_bases: usize,
    samples: usize,
    songs: usize,
}

/// What a registered timer slot fires: a Lua closure (`timer(n, div, fn)`),
/// or a native song player (`score{ song = }`'s `__song_start`) played
/// entirely in Rust by `render_frame_audio` — see [`LuaEngine::run_song_tick`].
/// A song hook is registered in the SAME list as a Lua one and in call
/// order, so it lands in the same slot a `timer(0, 32, fn)` call would.
#[derive(Clone)]
enum Hook {
    Lua(StashedFunction),
    /// Index into `DmaRecorder::songs`.
    Song(usize),
}

/// A recorded `timer(n, div, fn)` (or song) registration: timer, div, hook, defining chunk, and
/// the scene module whose load or init made it (`None`: the whole program's).
type TimerReg = (u8, u8, Hook, Option<String>, Option<String>);

/// A `dma()` or `world.load` result is setup the next load rebuilds from the sources: a hot swap
/// never carries it (see `carry`), so an edited image or map lands.
fn mark_setup_handle<'gc>(ctx: piccolo::Context<'gc>, rec: &DmaRecorder, handle: Table<'gc>) {
    let mut slot = rec.setup_mark.borrow_mut();
    let mark = match &*slot {
        Some(m) => ctx.fetch(m),
        None => {
            let m = Table::new(&ctx);
            *slot = Some(ctx.stash(m));
            m
        }
    };
    handle.set_metatable(&ctx, Some(mark));
}

/// Recorded registrations as hooks, each first due one period from t = 0 (see `realign`).
fn timer_hooks(regs: impl Iterator<Item = TimerReg>) -> Vec<TimerHook> {
    regs.map(|(n, div, hook, file, _)| {
        let period_h = div as u64 * if n == 2 { 1 } else { 8 };
        TimerHook {
            period_h,
            due_h: period_h,
            hook,
            file,
        }
    })
    .collect()
}

/// One `timer(n, div, fn)` registration, resolved to half-sample (`h`) units
/// at 32 kHz output (see `render_frame_audio`'s segment-walker doc comment):
/// `period_h` is `div * 8` for timers 0/1 (8 kHz) or `div * 1` for timer 2
/// (64 kHz); `due_h` is the next expiry as an ABSOLUTE timeline position in
/// half-samples from t = 0 (a timer fires at every `k * period_h`, k >= 1).
struct TimerHook {
    period_h: u64,
    due_h: u64,
    hook: Hook,
    file: Option<String>,
}

/// First sample of frame `f`'s audio span: `round(32000 * f / 60)` on the
/// 60 Hz frame grid, in exact integer math.
fn audio_start_s(f: u64) -> u64 {
    (1600 * f + 1) / 3
}

/// Size cap of the serialized `sram` blob — a real cartridge's battery RAM.
pub const SRAM_MAX_BYTES: usize = 32 * 1024;
const SRAM_MAX_DEPTH: usize = 32;

/// The resolved result of one init-stage `dma(name, opts?)` call. Replay
/// resolves `name` against the LIVE source store each frame, so an
/// `add_source` under the same name flows into the placement without a
/// recompile, and a `remove_source` degrades to a Mismatch report (same UX
/// as the old binding path) instead of a stale copy.
#[derive(Clone)]
struct DmaPlacement {
    name: String,
    /// The scene module whose load (or init) made it; `None` is the whole program's.
    owner: Option<String>,
    char_base: u16,
    map_base: u16,
    cgram_base: u8,
    /// The palette source whose colours replace the art's own, resolved each frame like `name`.
    palette: Option<String>,
    /// The result's other integer fields (`next_char`, `bit_depth`, ...).
    fields: Vec<(&'static str, i64)>,
}

impl DmaPlacement {
    fn view(&self) -> PlacementView {
        PlacementView {
            name: self.name.clone(),
            char: self.char_base,
            map: self.map_base,
            pal: self.cgram_base,
            fields: self.fields.clone(),
        }
    }
}

/// A `dma()` made from frame() while blanked (a stage load): replayed after
/// the setup placements, in call order, from the update that made it on.
/// `vram`/`cgram` are the word and entry spans it writes (end exclusive).
struct DmaLoad {
    placement: DmaPlacement,
    vram: Vec<(usize, usize)>,
    cgram: (usize, usize),
}

/// One stage load as [`LuaEngine::loads`] reports it: the placement and the
/// VRAM word spans and CGRAM entry span it writes (ends exclusive).
#[derive(Clone, Debug, serde::Serialize)]
pub struct LoadView {
    pub placement: PlacementView,
    pub vram: Vec<(usize, usize)>,
    pub cgram: (usize, usize),
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
    loops: bool,
}

/// A native `score{ song = "<name>" }` player: PSNG events plus the live
/// per-tick state one timer slot (see [`Hook::Song`]) advances every 32nd
/// tick, exactly as kit.lua's own `score{}` timer hook advances a Lua one —
/// see [`LuaEngine::run_song_tick`]. Pushed by `__song_load` (not playing,
/// no timer registered yet) and filled in by `__song_start`.
struct SongPlayer {
    name: String,
    events: Vec<crate::song::Event>,
    /// Song length in ticks (`Compiled::timing.length`).
    length: i64,
    /// The song's voice mask — a voice outside it is never touched.
    mask: u8,
    looping: bool,
    playing: bool,
    /// Where the song sits on the timeline — see [`Anchor`].
    anchor: Anchor,
    /// The anchor setup declared (`score{ at = }`): a scene's cold boot puts it back.
    declared: Anchor,
    /// Whole-tick anchor shift (half-samples) live reloads applied since the
    /// anchor was last set: `anchor - shift_h` is the declared (or
    /// `play()`) one a jump goes back to.
    shift_h: i64,
    /// Stopped because setup declared `at = false`, untouched since (any
    /// `play()`/`stop()` clears it): only such a player takes over a song
    /// the old program had sounding (see [`LuaEngine::carry_songs`]).
    at_false: bool,
    /// The next tick to play, wrapped into `0..=length` (it equals `length`
    /// right before a wrap or the finish).
    tick: i64,
    /// The UNWRAPPED next tick: `n` of the next fire, counted from the
    /// anchor across loop passes. A fire whose `n` differs is a
    /// discontinuity (a jump, a recompile) and re-seeks the cursor; -1
    /// forces that.
    next: i64,
    /// Index into `events` of the next event to apply.
    cursor: usize,
    /// Per-voice end tick of the note currently sounding there, if any.
    ends: [Option<i64>; 8],
    /// Per-row instrument table, stashed by `__song_start` (index = row).
    insts: Vec<StashedTable>,
    /// Per-row pitch, computed by kit.lua's `score{}` before `__song_start`
    /// (index = row).
    pitches: Vec<i64>,
    /// The song and its timing map, kept so a reload can find where this
    /// player is in it (see [`LuaEngine::reload_song`]).
    song: crate::song::Song,
    timing: crate::song::Timing,
}

/// A native song's place on the timeline. Its fire `n` (n >= 0) plays tick
/// `n` (wrapped when looping) at `anchor + (n + 1) * SONG_PERIOD_H`: the
/// song's position is `(t - anchor) * 250` ticks, silent before the anchor.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Anchor {
    /// Set up with `at = false`, or `stop()`ped: never fires.
    Stopped,
    /// `play()`ed: anchors at the start of the next rendered span (see
    /// [`LuaEngine::realign`]).
    Pending,
    /// Anchored this many half-samples from t = 0 (may be negative).
    At(i64),
}

/// A song timer's period: timer 0, div 32 = 256 half-samples = 4 ms.
const SONG_PERIOD_H: i64 = 256;

impl SongPlayer {
    /// Set the anchor: the declared (or `play()`) one, so any live-reload
    /// shift ends. Reload adds to `shift_h` and `carry_songs` copies both,
    /// so those write the fields directly.
    fn set_anchor(&mut self, a: Anchor) {
        self.anchor = a;
        self.shift_h = 0;
    }

    /// Point the cursor at the first event starting at or after `tick`, and
    /// `ends` at the notes still sounding there (later events win).
    fn seek(&mut self, tick: i64) {
        self.cursor = self.events.partition_point(|e| e.start < tick);
        self.ends = [None; 8];
        for e in &self.events[..self.cursor] {
            if e.end > tick {
                self.ends[e.voice as usize] = Some(e.end);
            }
        }
    }

    /// Whether the voice choice counts this song: playing, or play()ed and starting with the next span.
    fn sounds_for_effects(&self) -> bool {
        self.playing && matches!(self.anchor, Anchor::At(_) | Anchor::Pending)
    }

    /// Back to the top: tick 0, first fire next, nothing sounding.
    fn rewind(&mut self) {
        self.tick = 0;
        self.next = 0;
        self.seek(0);
    }
}

/// Fixed ARAM home of the 256-entry sample directory (DIR pinned to page
/// 0x01 — see `LuaEngine::new`'s DSP power-on write of `dsp.write(0x5d, ..)`).
const SAMPLE_DIR: u32 = 0x0100;
/// End (exclusive) of the sample directory page: 256 entries * 4 bytes.
const SAMPLE_DIR_END: u32 = SAMPLE_DIR + 256 * 4;

/// A program's t at frame `f` (from power-on), as the cartridge counts it: whole 1/1024 s steps, rounded down (60 frames
/// a second). A scene's t is its frame's less its entry frame's, so it can be a step longer than its own f gives.
pub fn cart_t(f: u32) -> f64 {
    (u64::from(f) * 1024 / 60) as f64 / 1024.0
}

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
    /// The metatable that marks a `dma()` / `world.load` result as setup (see `mark_setup_handle`).
    setup_mark: RefCell<Option<piccolo::StashedTable>>,
    raster: crate::lua_raster::Raster,
    active: Cell<bool>,
    /// The program's `world.load` maps, by the handle `__ppu_world_load` returned (index + 1).
    worlds: RefCell<Vec<World>>,
    placements: RefCell<Vec<DmaPlacement>>,
    /// Stage loads (`dma()` from frame() under force_blank), in call order.
    loads: RefCell<Vec<DmaLoad>>,
    /// True while the program's own frame() body runs: the only place a load may be made.
    in_frame: Cell<bool>,
    /// This update made a load, so force_blank must still be true when it ends.
    loaded: Cell<bool>,
    /// `allow_overlaps`: placements conflict with nothing.
    overlaps: Cell<bool>,
    /// Setup spans `(name, start, end, owner)`: placements conflict only within one scene.
    vram_ranges: RefCell<Vec<(String, usize, usize, Option<String>)>>,
    cgram_ranges: RefCell<Vec<(String, usize, usize, Option<String>)>>,
    /// The OBJ base each scene's first sprite placement chose (`None`: the whole program's).
    obj_base: RefCell<Vec<(Option<String>, u16)>>,
    /// Sample placements recorded during the init window, in call order.
    /// Never replayed — `replay_dma`'s `Sample(_) => {}` arm is a deliberate
    /// no-op: samples are written once by `LuaEngine::set_sources` after
    /// `init()`, not per frame.
    samples: RefCell<Vec<SamplePlacement>>,
    /// `timer(n, div, fn)` (and `__song_start`'s native equivalent)
    /// registrations recorded during the init window: (timer index 0..=2,
    /// div 1..=255, hook, defining chunk — `None` for a song hook, which
    /// can't error). Moved into `LuaEngine::timers` (with computed
    /// period_h/due_h) once `set_sources` confirms `init()` succeeded.
    timers: RefCell<Vec<TimerReg>>,
    /// Native `score{ song = }` players, indexed by [`Hook::Song`] and by
    /// the id `__song_load`/`__song_start`/`__song_get`/`__song_play`/
    /// `__song_stop` pass across the Lua boundary. Survives across the init
    /// window (unlike `timers`, `__song_get`/`play`/`stop` are callable any
    /// time — frame(), hooks) but is dropped wholesale on the next
    /// `set_sources` along with the rest of this recorder.
    songs: RefCell<Vec<SongPlayer>>,
    /// sfx() key-off countdowns in frames, per voice (0: none); see `tick_sfx_holds`. Carried
    /// across a recompile like the voices themselves.
    sfx_hold: Cell<[u16; 8]>,
    /// Voices sfx() keyed on this frame: their countdown starts next frame.
    sfx_armed: Cell<u8>,
    /// Frames each voice stays an effect's (0: the songs'; `u16::MAX`: until koff()). A song's
    /// key-ons, key-offs and register writes skip a voice an effect holds: it borrows the voice,
    /// and the song's next note after it ends plays. Carried like `sfx_hold`.
    sfx_own: Cell<[u16; 8]>,
    /// Voices held by automatic effects, and when each started (`sfx_seq` at the time).
    sfx_auto: Cell<u8>,
    sfx_since: Cell<[u16; 8]>,
    sfx_seq: Cell<u16>,
    /// Voices keyed on by kon() with no key-off on the way (the check's sustain warning).
    kon_open: Cell<u8>,
    /// True while the engine runs `__ppu_world_hotswap`: not a game frame, so w:enter skips the
    /// screen-off check. Held here, where a program can't set it.
    hot_swap: Cell<bool>,
    /// The layer last streamed into each VRAM map (`__ppu_world_strip`) since the scene began, whose tile
    /// animations play: (map, world, layer, its sheet's char base). See `animate_worlds`.
    world_animated: RefCell<Vec<(u16, usize, usize, u16)>>,
}

impl DmaRecorder {
    /// Voices an effect holds: a song leaves them alone until the effect ends.
    fn owned(&self) -> u8 {
        self.sfx_own
            .get()
            .iter()
            .enumerate()
            .fold(0, |m, (v, &f)| if f > 0 { m | 1 << v } else { m })
    }

    /// The voice choice's view of the 8 voices: which ones the playing songs use, how long each
    /// rests before its next key-on (known only up to the current slot's end, as on a cartridge,
    /// whose driver sees one slot's streams), and which effects hold them.
    fn voice_view(&self) -> [ppu_rules::voices::Voice; 8] {
        use ppu_rules::voices::{Held, Voice};
        let mut view = [Voice::FREE; 8];
        let (own, auto, since, seq) = (
            self.sfx_own.get(),
            self.sfx_auto.get(),
            self.sfx_since.get(),
            self.sfx_seq.get(),
        );
        let Ok(songs) = self.songs.try_borrow() else {
            return view;
        };
        for p in songs.iter().filter(|p| p.sounds_for_effects()) {
            // A song play()ed this update is seen from its top, where realign rewinds it: the cartridge's driver
            // starts it on its play command, before the effects sent after it.
            let (tick, cursor, ends) = match p.anchor {
                Anchor::Pending => (0, 0, [None; 8]),
                _ => (p.tick, p.cursor, p.ends),
            };
            let t = &p.timing;
            let slot_end = t
                .slots
                .iter()
                .map(|s| t.tick(s.pos + s.len as u64))
                .find(|&end| end > tick)
                .unwrap_or(t.length);
            let mut rest: [Option<i64>; 8] = [None; 8];
            for e in &p.events[cursor..] {
                if e.start >= slot_end {
                    break;
                }
                rest[e.voice as usize].get_or_insert(e.start - tick);
            }
            let used = p.events.iter().fold(0u8, |m, e| m | 1 << e.voice);
            for (i, v) in view
                .iter_mut()
                .enumerate()
                .filter(|(i, _)| used & 1 << i != 0)
            {
                let r = match (ends[i], rest[i]) {
                    (None, Some(r)) => Some(r.clamp(0, u16::MAX as i64) as u16),
                    _ => None,
                };
                v.rest = if v.song {
                    v.rest.zip(r).map(|(a, b)| a.min(b))
                } else {
                    r
                };
                v.song = true;
            }
        }
        for (i, v) in view.iter_mut().enumerate() {
            if own[i] > 0 {
                // Smaller is older: the oldest effect has the largest age.
                v.held = if auto & 1 << i != 0 {
                    Held::Auto(u16::MAX - seq.wrapping_sub(since[i]))
                } else {
                    Held::Explicit
                };
            }
        }
        view
    }

    /// The voices the playing songs' kept parts play, which no automatic effect takes (the cartridge's KEEP).
    fn keep_mask(&self) -> u8 {
        let Ok(songs) = self.songs.try_borrow() else {
            return 0;
        };
        (songs.iter())
            .filter(|p| p.sounds_for_effects())
            .fold(0, |m, p| m | crate::song::keep_mask(&p.song, &p.events))
    }
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
        let sheet_pals: Rc<RefCell<HashMap<String, Vec<u8>>>> = Rc::default();
        let project_files: Rc<RefCell<HashMap<String, String>>> = Rc::default();
        {
            let (store, pals, files, rec) = (
                source_store.clone(),
                sheet_pals.clone(),
                project_files.clone(),
                dma.clone(),
            );
            lua.enter(move |ctx| install_world(ctx, store, pals, files, rec));
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
            sprites: false,
            controls_fn: None,
            controls_vram: Vec::new(),
            pokes: String::new(),
            memory: Memory::new(),
            source_store,
            sheet_pals,
            project_files,
            dma,
            reports: Vec::new(),
            program_sources: Vec::new(),
            source_dirty: false,
            fixed_point: false,
            picture_memory: true,
            pad: 0,
            pad2: 0,
            pads_before: None,
            dsp,
            aram: Box::new([0u8; 0x10000]),
            audio: Vec::new(),
            last_f: None,
            recompiled: false,
            prev_run: None,
            hold: false,
            held: None,
            not_carried: Vec::new(),
            timers: Vec::new(),
            sram_json: "{}".to_string(),
            sram_dirty: false,
            saved_mix: AudioMix::default(),
            live_mix: None,
            unmixed_dsp: None,
            mix_kon: 0,
            mix_koff: 0,
            scenes: None,
            target_hz: None,
            run_base: 0,
            frames_note: 1,
            shown: None,
            violation_sink: Rc::default(),
            violations: Vec::new(),
            update_f: 0,
            start_scene: None,
            start_preview: false,
        }
    }

    /// Mirrored PPU memory after the most recent `frame()`.
    pub fn memory(&self) -> &Memory {
        &self.memory
    }

    /// The palette of each char of sheet source `name`, in sheet order: the `pal` of each of its
    /// `SourceMeta.cells`. A Tiled map drawn from the sheet takes its colours from these.
    pub fn set_sheet_palettes(&mut self, name: &str, pals: &[u8]) {
        self.sheet_pals
            .borrow_mut()
            .insert(name.to_string(), pals.to_vec());
        self.source_dirty = !self.program_sources.is_empty();
    }

    /// Decode + register a source payload under `name` (the `addSource` core).
    /// The store is the graphics-data home; `dma()` validates against it at
    /// init and the per-frame replay places from it (no importer on this path).
    /// Re-adding a song source a score plays reloads it live, without the
    /// recompile any other change triggers (see `reload_song`).
    pub fn add_source(
        &mut self,
        name: &str,
        payload: &[u8],
    ) -> Result<(), crate::source::PayloadError> {
        let p = crate::source::SourcePayload::decode(payload)?;
        let live = match &p {
            crate::source::SourcePayload::Song(bytes) => self.reload_song(name, bytes),
            _ => false,
        };
        self.source_store.borrow_mut().insert(name.to_string(), p);
        if !live {
            self.source_dirty = !self.program_sources.is_empty();
        }
        Ok(())
    }

    /// A song source re-added under the same name, applied without a
    /// recompile when it can be: every native player of `name` swaps in the
    /// new song in place, keeping its musical position (see
    /// [`crate::song::remap`]). A song past its new end wraps, or stops
    /// rewound (loop = false). The voices of a playing one key off and it
    /// re-seeks to the next event due. An anchored song's position comes
    /// from time, so keeping it plays against an anchor shifted by whole
    /// ticks: an edit that changes timing shifts it, a note-only edit leaves
    /// it exactly as it was. The shift lives in `shift_h` (which
    /// `ScoreView::anchor` does not show) until the next jump (seek, scrub,
    /// loop rewind) or recompile (Run) goes back to the declared `at`. The
    /// loop pass is kept while the playing slot stays inside the body; a
    /// song past its new end wraps to the loop start, and a playing slot
    /// that became intro drops to the first pass. A finished
    /// loop = false song stays finished. A player whose song decodes equal is
    /// left untouched, and an update to a song source no player plays
    /// touches nothing. Returns false, leaving everything as it was, when
    /// the change needs the full recompile: a pending one, a first-time
    /// add, a song that doesn't decode or compile (setup reports it), or a
    /// row naming a sound nothing placed at setup.
    fn reload_song(&mut self, name: &str, bytes: &[u8]) -> bool {
        use crate::source::SourcePayload;
        if self.source_dirty
            || self.program_sources.is_empty()
            || !matches!(
                self.source_store.borrow().get(name),
                Some(SourcePayload::Song(_))
            )
        {
            return false;
        }
        let dma = self.dma.clone();
        if !dma.songs.borrow().iter().any(|p| p.name == name) {
            return true;
        }
        let Ok(song) = crate::song::decode(bytes) else {
            return false;
        };
        let Ok(compiled) = crate::song::compile(&song) else {
            return false;
        };
        let changed = |p: &SongPlayer| p.name == name && p.song != song;
        if !dma.songs.borrow().iter().any(changed) {
            return true;
        }

        let mut l = self.lua.borrow_mut();
        let ex = l.enter(|ctx| {
            let Value::Function(f) = ctx.get_global("__song_insts") else {
                panic!("kit.lua must define __song_insts");
            };
            ctx.stash(Executor::start(ctx, f, (song_rows(ctx, &song),)))
        });
        l.finish(&ex);
        let resolved = l.try_enter(|ctx| {
            let (insts, pitches) = ctx.fetch(&ex).take_result::<(Value, Value)>(ctx)??;
            let (Value::Table(insts), Value::Table(pitches)) = (insts, pitches) else {
                return Ok(None);
            };
            let mut out = (Vec::new(), Vec::new());
            for i in 1..=song.rows.len() as i64 {
                let Value::Table(inst) = insts.get(ctx, i) else {
                    return Ok(None);
                };
                out.0.push(ctx.stash(inst));
                out.1.push(pitches.get(ctx, i).to_int().unwrap_or(0x1000));
            }
            Ok(Some(out))
        });
        let Ok(Some((insts, pitches))) = resolved else {
            return false;
        };

        l.enter(|ctx| {
            let new_t = &compiled.timing;
            let new_len = new_t.length;
            for p in dma.songs.borrow_mut().iter_mut().filter(|p| changed(p)) {
                let old = (&p.song, &p.timing);
                let remapped = crate::song::remap(old, (&song, new_t), p.tick);
                if p.playing {
                    song_koff(ctx, p.mask & !dma.owned());
                }
                // `next < 0`: no fire since a jump; the next one re-seeks
                // by time, so there is no position to keep.
                let anchored = matches!(p.anchor, Anchor::At(_)) && p.next >= 0;
                // After fire `n` the player holds `next = n + 1`: the pass is
                // the one fire `n` played in.
                let pass = if p.looping && p.next > 0 {
                    p.timing.pass(p.next - 1)
                } else {
                    0
                };
                // (unwrapped next tick, wrapped next tick, playing).
                let (n_new, tick, playing) = match () {
                    // Finished (an anchored loop = false song not playing):
                    // stays past the end, silent.
                    _ if anchored && !p.playing && !p.looping => (p.next.max(new_len), 0, false),
                    // The playing slot is now in the intro: it plays on once.
                    _ if remapped < new_len && pass >= 1 && remapped < new_t.loop_tick => {
                        (remapped, remapped, p.playing)
                    }
                    _ if remapped < new_len => (new_t.unwrap(pass, remapped), remapped, p.playing),
                    // Past the new end: a loop wraps to the next pass's loop start…
                    _ if p.looping => (
                        new_t.unwrap(pass + 1, new_t.loop_tick),
                        new_t.loop_tick,
                        p.playing,
                    ),
                    // …a loop = false song finishes, as `run_song_tick` leaves one.
                    _ => (remapped, 0, false),
                };
                if let (true, Anchor::At(a)) = (anchored, p.anchor) {
                    let delta = (p.next - n_new) * SONG_PERIOD_H;
                    p.anchor = Anchor::At(a + delta);
                    p.shift_h += delta;
                    p.next = n_new;
                }
                p.playing = playing;
                p.tick = tick;
                p.events = compiled.events.clone();
                p.seek(tick);
                p.length = new_len;
                p.mask = song.voice_mask;
                p.insts = insts.clone();
                p.pitches = pitches.clone();
                p.song = song.clone();
                p.timing = new_t.clone();
            }
        });
        true
    }

    /// Forget the source registered under `name` (the `removeSource` core).
    /// The next frame reruns setup, so a remaining `dma()` reference fails
    /// loudly like any other missing source.
    pub fn remove_source(&mut self, name: &str) -> bool {
        let removed = self.source_store.borrow_mut().remove(name).is_some();
        self.source_dirty |= removed && !self.program_sources.is_empty();
        removed
    }

    /// The program's scene files in `scenes.add` order; a program without scenes is one implied
    /// scene, its main file. Changes only with a load.
    pub fn scenes(&self) -> Vec<String> {
        match &self.scenes {
            Some(sc) => sc.list.iter().map(|s| s.file.clone()).collect(),
            None => {
                let names = || self.program_sources.iter().map(|(n, _)| n.as_str());
                let main = ["main.luau", "main.lua"]
                    .into_iter()
                    .find(|m| names().any(|n| n == *m));
                let first = names().find(|n| *n != AUDIO_MIX_FILE);
                vec![main.or(first).unwrap_or("main.luau").to_string()]
            }
        }
    }

    /// The running scene: an index into [`Self::scenes`]. It follows `scenes.go` from the update
    /// the switch lands on.
    pub fn current_scene(&self) -> usize {
        self.scenes.as_ref().map_or(0, |sc| sc.current)
    }

    /// Boot every later load (a recompile, [`Self::reset`]) straight into scene `file`, as if the
    /// program had switched to it before its first update: file level and every `scenes.add` run,
    /// no other scene's init or frame does. With `preview` it is a Scene view entry: the scene's
    /// `preview` runs before its init; without, the load only lands where the program was (a scene
    /// `scenes.go` entered), never staging it. `None`, or a file that is no scene, boots as the
    /// program does. Takes effect at the next load, which this does not start.
    pub fn set_start_scene(&mut self, file: Option<String>, preview: bool) {
        self.start_scene = file;
        self.start_preview = preview;
    }

    /// The scene a `scenes.go` asked for and the next new update would enter: an index into
    /// [`Self::scenes`]. The Studio's Scene view holds the switch here instead of following it.
    pub fn pending_scene(&self) -> Option<usize> {
        let count = self.scenes.as_ref()?.list.len() as i64;
        let next = self
            .lua
            .borrow_mut()
            .enter(|ctx| ctx.get_global("__ppu_scene_next").to_int());
        next.filter(|n| (1..=count).contains(n))
            .map(|n| n as usize - 1)
    }

    /// Enter the [`Self::pending_scene`] now, as `scenes.go` would but with its clock starting
    /// over: the next update is `frame(0, 0)` and the scene sees t = 0, f = 0. No preview runs.
    /// False when no switch waits.
    pub fn enter_pending_scene(&mut self) -> Result<bool, LuaError> {
        let Some(k) = self.pending_scene() else {
            return Ok(false);
        };
        self.enter_scene(k, Some((0.0, 0)), false)?;
        Ok(true)
    }

    /// Per-layer import budgets from the most recent `frame()` (m4/inspector).
    pub fn import_reports(&self) -> &[ImportBudget] {
        &self.reports
    }

    /// Every VRAM/CGRAM span and placement the last recompile's `dma()`
    /// calls recorded (samples excluded — they live in ARAM).
    pub fn memory_map(&self) -> MemoryMap {
        let range = |(name, start, end, _): &(String, usize, usize, Option<String>)| MemoryRange {
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
                .map(DmaPlacement::view)
                .collect(),
        }
    }

    /// The stage loads (`dma()` calls from frame() under force_blank) replayed
    /// this frame, in call order; a load is dropped once a later one covers
    /// every word and entry it wrote. Not part of [`Self::memory_map`], which
    /// stays the setup layout.
    pub fn loads(&self) -> Vec<LoadView> {
        let loads = self.dma.loads.borrow();
        loads
            .iter()
            .map(|l| LoadView {
                placement: l.placement.view(),
                vram: l.vram.clone(),
                cgram: l.cgram,
            })
            .collect()
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

    /// The 64 KB sound RAM — read-only; writes go through `aram[]`.
    pub fn aram(&self) -> &[u8] {
        &self.aram[..]
    }

    /// The live S-DSP core — read-only; all writes go through the `dsp`/
    /// `voice[]` DSL tables, flushed by `render_frame_audio`.
    /// Start logging every DSP register write with its 32 kHz sample time.
    pub fn start_dsp_log(&mut self) {
        self.dsp.log = Some(Vec::new());
    }

    /// The writes logged since `start_dsp_log` (or the last take).
    pub fn take_dsp_log(&mut self) -> Vec<(u64, u8, u8)> {
        self.dsp
            .log
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    }

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
        let open = self.dma.kon_open.get();
        let samples = self.dma.samples.borrow();
        for (n, v) in view.voices.iter_mut().enumerate() {
            let loops = samples
                .iter()
                .rev()
                .find(|s| s.id == v.sample)
                .is_some_and(|s| s.loops);
            if open & 1 << n == 0 || !loops {
                continue;
            }
            v.sustains = match v.gain {
                // Direct and increasing GAIN hold; decreasing GAIN fades out on its own.
                Some(g) if g & 0x80 == 0 || g >> 5 >= 6 => Some(f64::INFINITY),
                Some(_) => None,
                None => Some(crate::dsp::sustain_seconds(v.adsr.s, v.adsr.r)),
            };
        }
        view
    }

    /// The most recently started `score{}` (kit.lua publishes its handle as
    /// `__score`), or None when there is none or it is stopped. A `loop =
    /// false` song that played out is not gone: it reports `finished` (an
    /// anchored player that is neither looping nor playing), keeping its
    /// anchor.
    pub fn score_view(&self) -> Option<ScoreView> {
        self.lua.borrow_mut().enter(|ctx| {
            let id = score_id(ctx)?;
            let songs = self.dma.songs.borrow();
            let p = songs.get(id as usize)?;
            let anchored = matches!(p.anchor, Anchor::At(_));
            if p.length <= 0 || p.anchor == Anchor::Stopped || (!p.playing && !anchored) {
                return None;
            }
            // An anchored player that isn't playing is a finished
            // loop = false song: it reports `length` and keeps its anchor.
            let finished = !p.looping && !p.playing;
            let tick = if finished {
                p.length
            } else if p.looping {
                p.timing.wrap(p.tick)
            } else {
                p.tick
            };
            // Global step = the target slot's own step (from `position`)
            // plus every earlier slot's step count — the same prefix
            // `song_analyze::analyze` builds into `slot_steps`.
            let (slot, step) = p
                .timing
                .position(tick)
                .map(|(slot, step, _)| {
                    let earlier: u64 = (0..slot).map(|s| p.timing.steps(s)).sum();
                    (slot as u32, (earlier + step) as u32)
                })
                .unzip();
            Some(ScoreView {
                tick,
                length: p.length,
                song: Some(p.name.clone()),
                slot,
                step,
                anchor: match p.anchor {
                    Anchor::At(a) => Some(
                        (a - p.shift_h) as f64 / 64000.0
                            + self.scenes.as_ref().map_or(0.0, |s| s.t0),
                    ),
                    _ => None,
                },
                loop_tick: p.looping.then_some(p.timing.loop_tick),
                finished,
            })
        })
    }

    /// Single-file sugar for [`Self::set_sources`]; the chunk keeps its
    /// historical name `"source"` so existing diagnostics are unchanged.
    /// Opt every following `set_sources` into makeFixed's arithmetic (see
    /// `frontend::fixed_point`), as if the program declared a makeFixed contract.
    /// A tool evaluating a derived copy of such a program uses this.
    pub fn set_fixed_point(&mut self, on: bool) {
        self.fixed_point = on;
    }

    /// Whether `frame()` rebuilds `memory()` from the program's `cgram`, `obj`, `bg[n].map`, `m7` and `vram` tables
    /// (on by default). A run that only watches for errors turns it off: nothing it checks reads that memory.
    pub fn set_picture_memory(&mut self, on: bool) {
        self.picture_memory = on;
    }

    pub fn set_source(&mut self, src: &str) -> Result<(), LuaError> {
        self.set_sources(&[("source", src)])
    }

    /// The sequencer sugar prelude (`note`/`instrument`/`sfx`/`song`), run as
    /// the `"kit"` chunk before every user chunk in [`Self::set_sources`] —
    /// see `kit.lua`'s own doc comment.
    const KIT_LUA: &'static str = include_str!("kit.lua");

    /// Luau's `math.random` signatures over `random.lua`'s generator, which a cartridge
    /// compiles too (so this wrapper stays out of that file): `()` a fraction in [0, 1),
    /// `(n)` an integer 1..n, `(m, n)` an integer m..n.
    const MATH_RANDOM_LUA: &'static str = r#"
function math.random(m, n)
  if m == nil then return (__ppu_random_next() - 1) / 65535 end
  if n == nil then m, n = 1, m end
  if m > n then error("math.random: the interval is empty", 2) end
  return __ppu_random(m, n)
end
math.randomseed = __ppu_randomseed
"#;

    /// `math.hash(seed, x, y)`: 0 to 255 from a seed and two whole numbers, table lookups over a fixed permutation of
    /// 0..255, which a cartridge bakes the same (the cartridge compiler).
    const MATH_HASH_LUA: &'static str = r#"
local perm = {}
for i = 0, 255 do
  local a = i * (2 * i + 1) % 256
  local b = a % 16 * 16 + a // 16
  perm[i] = b * (2 * b + 1) % 256
end
function math.hash(seed, x, y)
  if seed % 1 ~= 0 or x % 1 ~= 0 or y % 1 ~= 0 then
    error("math.hash(seed, x, y) takes three whole numbers, got " .. tostring(seed) .. ", " .. tostring(x) .. ", " .. tostring(y), 2)
  end
  local a = perm[(seed + x) % 256]
  local b = perm[(seed // 256 + y) % 256]
  return perm[(perm[(a + y) % 256] + perm[(b + x) % 256]) % 256]
end
"#;

    /// The change-tracking environment the live pokes chunk is compiled against —
    /// see `controls_env.lua`'s own doc comment and [`Self::load_controls`].
    const CONTROLS_ENV_LUA: &'static str = include_str!("controls_env.lua");

    /// For a tool that loads a program to read what it builds, never to draw it: `dma()` places over anything.
    pub fn allow_overlaps(&mut self) {
        self.dma.overlaps.set(true);
    }

    /// Every VRAM span the loaded program's file-level `dma()` calls placed, in words: (name,
    /// first, end, the scene module that placed it; `None` for the whole program's).
    pub fn vram_spans(&self) -> Vec<(String, usize, usize, Option<String>)> {
        self.dma.vram_ranges.borrow().clone()
    }

    /// The maps the loaded program's `world.load` calls read, in call order (a world's handle is
    /// its index + 1), each tile layer as binding it in the world's own scene would draw it.
    pub fn worlds(&self) -> Vec<WorldView> {
        // Every scene's placements: entering a scene keeps only its own in `dma.placements`.
        let current = self.dma.placements.borrow();
        let placements = self
            .scenes
            .as_ref()
            .map_or(&current[..], |sc| &sc.placements[..]);
        self.dma
            .worlds
            .borrow()
            .iter()
            .map(|w| WorldView {
                name: w.name.clone(),
                owner: w.owner.clone(),
                map: w.map.clone(),
                layers: (0..w.map.layers.len())
                    .map(|li| {
                        if w.map.layers[li].mode7 {
                            m7_layer(placements, w, li, &w.owner)?;
                            let words = w.words[li].clone().unwrap_or_default();
                            return Ok(WorldLayerView { words, char_base: 0, palettes: 0, tile_palettes: true, extbg: w.extbg[li], char_words: 0, pal: 0 });
                        }
                        let (char_base, pal) = sheet_layer(placements, w, li, &w.owner)?;
                        // A whole-program world bound in a scene draws with that scene's latest placement, which
                        // this answer cannot know: it holds only when every scene places the sheet the same way.
                        if w.owner.is_none() {
                            let mut scenes = placements.iter().filter_map(|p| p.owner.clone());
                            if let Some(scene) = scenes.find(|s| sheet_layer(placements, w, li, &Some(s.clone())).ok() != Some((char_base, pal))) {
                                return Err(format!(
                                    "layer {} draws from a sheet scene {scene} places at other chars or CGRAM than the program does, so it draws differently in each scene: load the world in the scene that binds it, or place the sheet once for the whole program",
                                    w.map.layers[li].name
                                ));
                            }
                        }
                        let words = w.words[li].iter().flatten().map(|&word| word + (pal << 10)).collect();
                        let (char_words, tile_palettes) = w.map.layers[li].source.as_ref().and_then(|s| match self.source_store.borrow().get(s) {
                            Some(crate::source::SourcePayload::Sheet(sheet)) => Some((
                                sheet.bit_depth as usize * 4,
                                sheet.palettes.len() <= 1 || !char_pals(sheet, self.sheet_pals.borrow().get(s)).is_empty(),
                            )),
                            _ => None,
                        }).unwrap_or((0, true));
                        Ok(WorldLayerView { words, char_base, palettes: w.palettes[li], tile_palettes, extbg: false, char_words, pal })
                    })
                    .collect(),
            })
            .collect()
    }

    /// Compile and load a multi-file sketch (PICO-8 scope): builds a fresh VM,
    /// installs bindings, then executes each `(name, source)` chunk **in list
    /// order** into ONE shared global environment, each compiled with its file
    /// name as chunk name. `frame`/`init` are resolved only after every chunk
    /// has run (`main.lua` is convention, not special-cased); `init()` runs
    /// once if present. Errors carry `{file, line?, message}`.
    /// A file named by `require("./name")` in any file runs as a module (see `luau.lua`), and so
    /// does an engine library named by `require("@ppu/name")` (lib/name.luau).
    pub fn set_sources(&mut self, files: &[(&str, &str)]) -> Result<(), LuaError> {
        let loaded = self.load_sources(files);
        self.note_violations(None);
        loaded
    }

    /// What the last load did not carry over from the running program, one line per variable
    /// that kept its new initial value ("main.luau: enemies kept its new value: ..."). Empty after
    /// a load that carried everything, and after any power-on.
    pub fn not_carried(&self) -> &[String] {
        &self.not_carried
    }

    /// The host paused (true) or plays (false). Paused, drawing the same update again does not
    /// advance the program: each repeat starts from the state the update first started from, so
    /// a poke shows its change and nothing else moves. A coroutine (a `routine`) still steps.
    pub fn set_hold(&mut self, on: bool) {
        if self.hold != on {
            self.hold = on;
            self.held = None;
        }
    }

    /// Another toy opens: the next load powers on instead of carrying the running program's state.
    pub fn forget_run(&mut self) {
        self.prev_run = None;
        self.not_carried.clear();
    }

    /// Broken contract stores since the last `reset` (a power-on or a scene's restart), first
    /// seen first. The preview never stops on one: the value stays as written.
    pub fn violations(&self) -> &[crate::bounded::Violation] {
        &self.violations
    }

    /// Stamp what the contract checks found with the running scene and `frame`, folding repeats
    /// of one file:line and store into its first.
    fn note_violations(&mut self, frame: Option<u32>) {
        let found = std::mem::take(&mut *self.violation_sink.borrow_mut());
        if found.is_empty() {
            return;
        }
        let scene = self.scenes().swap_remove(self.current_scene());
        for v in found {
            match self
                .violations
                .iter_mut()
                .find(|o| (&o.file, o.line, &o.store) == (&v.file, v.line, &v.store))
            {
                Some(o) => o.count += 1,
                None => self.violations.push(crate::bounded::Violation {
                    scene: scene.clone(),
                    frame,
                    ..v
                }),
            }
        }
    }

    fn load_sources(&mut self, files: &[(&str, &str)]) -> Result<(), LuaError> {
        // Tiled maps and tilesets are data `world.load` reads, never Luau.
        let (project, files): (Vec<_>, Vec<_>) = files
            .iter()
            .copied()
            .partition(|(name, _)| crate::frontend::is_project_file(name));
        let files = files.as_slice();
        let project: HashMap<String, String> = project
            .into_iter()
            .map(|(n, s)| (n.to_string(), s.to_string()))
            .collect();
        if *self.project_files.borrow() != project {
            self.source_dirty |= !self.program_sources.is_empty();
            *self.project_files.borrow_mut() = project;
        }
        let libraries =
            crate::frontend::with_libraries(files).map_err(|(file, message)| LuaError {
                file: Some(file),
                line: None,
                message,
            })?;
        let files = libraries.as_slice();
        let prepared: Vec<_> = files
            .iter()
            .map(|&(name, source)| {
                let source = if name == AUDIO_MIX_FILE {
                    std::borrow::Cow::Borrowed(source)
                } else {
                    crate::frontend::prepare(source).map_err(|e| LuaError {
                        file: Some(name.into()),
                        line: Some(e.line),
                        message: e.message,
                    })?
                };
                Ok((name, source))
            })
            .collect::<Result<_, LuaError>>()?;
        // After prepare: it refuses nesting too deep for the parse fixed_contracts makes.
        let fixed_point = self.fixed_point
            || files.iter().any(|(name, source)| {
                *name != AUDIO_MIX_FILE && crate::frontend::fixed_contracts(source)
            });
        let files: Vec<_> = prepared
            .iter()
            .map(|(name, source)| (*name, source.as_ref()))
            .collect();
        let files = files.as_slice();
        let precision_files: Vec<_> = files
            .iter()
            .copied()
            .filter(|(n, _)| *n != AUDIO_MIX_FILE)
            .collect();
        // Helpers that cut art become their art at each file-level call, as the cartridge reads them.
        let expanded = crate::sprites::expand_helpers(&precision_files);
        let precision_files: Vec<(&str, &str)> = match &expanded {
            Some(e) => e.iter().map(|(n, s)| (n.as_str(), s.as_str())).collect(),
            None => precision_files,
        };
        let sprite_plan = crate::sprites::plan(&precision_files).map_err(|e| LuaError {
            file: Some(e.file),
            line: Some(e.line),
            message: format!("error[snes.sprites]: {}", e.message),
        })?;
        // Each sprites.art and sprites.group call passes its number in the plan.
        let numbered: Vec<(&str, String)> = match &sprite_plan {
            Some(plan) => precision_files
                .iter()
                .map(|(n, s)| (*n, crate::sprites::number_sites(plan, n, s)))
                .collect(),
            None => vec![],
        };
        let precision_files: Vec<(&str, &str)> = if sprite_plan.is_some() {
            numbered.iter().map(|(n, s)| (*n, s.as_str())).collect()
        } else {
            precision_files
        };
        // The Luau surface runs makeFixed's law as rewritten source, bindings included; a Lua
        // program with canonical annotations or fixed bindings keeps canonical precision.
        let surface = precision_files
            .iter()
            .any(|(n, s)| n.ends_with(".luau") || !crate::frontend::requires(s).is_empty());
        let fixed_point = fixed_point
            || surface
                && precision_files
                    .iter()
                    .any(|(_, s)| crate::frontend::fixed_calls(s));
        let dynamic = fixed_point
            && (surface
                || !precision_files
                    .iter()
                    .any(|(_, s)| crate::precision::enabled(s)));
        let fixed_sources: Vec<(&str, std::borrow::Cow<str>)> = precision_files
            .iter()
            .map(|&(name, source)| {
                let source = if dynamic {
                    crate::frontend::fixed_point(source)
                } else {
                    std::borrow::Cow::Borrowed(source)
                };
                (name, source)
            })
            .collect();
        let precision_files: Vec<(&str, &str)> = fixed_sources
            .iter()
            .map(|(n, s)| (*n, s.as_ref()))
            .collect();
        let precision_chunks =
            crate::precision::prepare_sources_with(&precision_files, dynamic && surface).map_err(
                |(file, e)| LuaError {
                    file: Some(file),
                    line: Some(e.line as u32),
                    message: format!("{} {}", e.message, e.help),
                },
            )?;
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
        // Saving only the mix must preserve the VM, timer phase, and sounding
        // notes: the fast path below reloads it without a full recompile. A
        // song source's own in-place reload goes through
        // `add_source`/`reload_song`, not this Lua-file fast path.
        let run_chunk = |lua: &mut Lua, name: &str, src: &str| -> Result<(), LuaError> {
            let load = lua.try_enter(|ctx| {
                let closure = Closure::load(ctx, Some(name), src.as_bytes())?;
                Ok(ctx.stash(Executor::start(ctx, closure.into(), ())))
            });
            load.and_then(|ex| lua.execute::<()>(&ex))
                .map_err(|e| static_error_to_lua(e).in_file(name))
        };
        let live = |n: &str| n == AUDIO_MIX_FILE;
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
        {
            saved_mix
                .validate_samples(&self.dsp_view().samples)
                .map_err(|message| LuaError {
                    message,
                    line: None,
                    file: Some(AUDIO_MIX_FILE.into()),
                })?;
            self.saved_mix = saved_mix;
            self.program_sources = files
                .iter()
                .map(|(n, s)| ((*n).into(), (*s).into()))
                .collect();
            return Ok(());
        }
        self.not_carried.clear();
        let mut mains: Vec<(String, piccolo::StashedClosure)> = vec![];
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
        rec.overlaps.set(self.dma.overlaps.get());
        rec.sfx_hold.set(self.dma.sfx_hold.get());
        rec.sfx_own.set(self.dma.sfx_own.get());
        rec.sfx_auto.set(self.dma.sfx_auto.get());
        rec.sfx_since.set(self.dma.sfx_since.get());
        rec.sfx_seq.set(self.dma.sfx_seq.get());
        rec.kon_open.set(self.dma.kon_open.get());
        rec.active.set(true);
        {
            let (store, rec) = (self.source_store.clone(), rec.clone());
            lua.enter(move |ctx| install_dma(ctx, store, rec));
        }
        {
            let (store, pals, project, rec) = (
                self.source_store.clone(),
                self.sheet_pals.clone(),
                self.project_files.clone(),
                rec.clone(),
            );
            lua.enter(move |ctx| install_world(ctx, store, pals, project, rec));
        }
        // Run once per fresh VM, right before the chunk loop: installs the
        // change-tracking `__ppu_controls_env` (a proxy of globals) and the
        // `__ppu_controls_begin/dirty/restore` hooks the pokes chunk's
        // `__ppu_pokes` runs under — see `controls_env.lua` and
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

        if precision_chunks.is_some() {
            lua.enter(crate::precision::install);
            let sink = self.violation_sink.clone();
            lua.enter(move |ctx| crate::bounded::install(ctx, sink));
        }
        run_chunk(&mut lua, "kit", Self::KIT_LUA)?;
        run_chunk(&mut lua, "ramps", include_str!("ramps.lua"))?;
        run_chunk(&mut lua, "random", include_str!("random.lua"))?;
        run_chunk(&mut lua, "math_random", Self::MATH_RANDOM_LUA)?;
        run_chunk(&mut lua, "math_hash", Self::MATH_HASH_LUA)?;
        run_chunk(&mut lua, "vram_helpers", include_str!("vram_helpers.lua"))?;
        run_chunk(&mut lua, "hud", include_str!("hud.lua"))?;
        run_chunk(&mut lua, "sprites", include_str!("sprites.lua"))?;
        if let Some(plan) = &sprite_plan {
            lua.enter(|ctx| {
                let free = Table::new(&ctx);
                for (i, slot) in plan.free().into_iter().enumerate() {
                    free.set(ctx, i as i64 + 1, slot as i64).unwrap();
                }
                ctx.set_global("__ppu_sprites_free", free).unwrap();
            });
        }

        // The pokes chunk runs FIRST, before any of the sketch's files, inside the
        // init window, so its `dma()` lines are setup placements like any other.
        // It compiles against the tracked env installed above, not the plain
        // globals table the other chunks share — see `Self::load_controls`.
        let controls_fn = if self.pokes.is_empty() {
            None
        } else {
            Self::load_controls(&mut lua, &self.pokes)?
        };
        let controls_vram = match controls_fn {
            Some(_) => Self::read_controls_vram(&mut lua),
            None => Vec::new(),
        };

        {
            let sink = self.violation_sink.clone();
            lua.enter(move |ctx| crate::bounded::install_records(ctx, sink));
        }
        run_chunk(&mut lua, "luau", include_str!("luau.lua"))?;
        let scene_files = crate::frontend::scene_files(&precision_files);
        lua.enter(|ctx| {
            let Value::Table(t) = ctx.get_global("__ppu_scene_files") else {
                unreachable!("luau.lua defines the scene files")
            };
            for file in &scene_files {
                t.set(ctx, ctx.intern(file.as_bytes()), true).unwrap();
            }
            let Value::Table(o) = ctx.get_global("__ppu_scene_owners") else {
                unreachable!("luau.lua defines the scene owners")
            };
            for (file, k) in crate::sprites::scene_owners(&precision_files) {
                o.set(
                    ctx,
                    ctx.intern(file.as_bytes()),
                    ctx.intern(scene_files[k - 1].as_bytes()),
                )
                .unwrap();
            }
        });

        // A file another file requires is a module: it runs on its first require, not in tab order.
        let modules = crate::frontend::required_files(&precision_files);
        fn compile<'gc>(
            ctx: piccolo::Context<'gc>,
            compiled: Option<&piccolo::compiler::CompiledPrototype<Rc<[u8]>>>,
            name: &str,
            src: &str,
        ) -> Result<Closure<'gc>, piccolo::Error<'gc>> {
            Ok(match compiled {
                Some(compiled) => {
                    let prototype = piccolo::FunctionPrototype::from_compiled_map_strings(
                        &ctx,
                        ctx.intern(name.as_bytes()),
                        compiled,
                        |s| ctx.intern(s),
                    );
                    Closure::new(&ctx, prototype, Some(ctx.globals()))?
                }
                None => Closure::load(ctx, Some(name), src.as_bytes())?,
            })
        }
        let chunk = |i: usize| precision_chunks.as_ref().map(|c| &c[i]);
        for (i, (name, src)) in precision_files.iter().enumerate() {
            if modules.contains(*name as &str) {
                let main = lua
                    .try_enter(|ctx| {
                        let closure = compile(ctx, chunk(i), name, src)?;
                        let Value::Table(registry) = ctx.get_global("__ppu_modules") else {
                            unreachable!("luau.lua defines the module registry")
                        };
                        registry.set(ctx, ctx.intern(name.as_bytes()), closure)?;
                        Ok(ctx.stash(closure))
                    })
                    .map_err(|e| static_error_to_lua(e).in_file(name))?;
                mains.push((name.to_string(), main));
            }
        }
        for (i, (name, src)) in precision_files.iter().enumerate() {
            if modules.contains(*name as &str) {
                continue;
            }
            let load = lua.try_enter(|ctx| {
                let closure = compile(ctx, chunk(i), name, src)?;
                Ok((
                    ctx.stash(Executor::start(ctx, closure.into(), ())),
                    ctx.stash(closure),
                ))
            });
            let (ex, main) = load.map_err(|e| static_error_to_lua(e).in_file(name))?;
            lua.execute::<()>(&ex)
                .map_err(|e| located(&mut lua, &ex, e).in_file(name))?;
            mains.push((name.to_string(), main));
        }

        // The groups' slots, now that every art has run: the size pair and each art's pieces, then the
        // plan's layout by each art's pieces.
        if let Some(plan) = &sprite_plan {
            lua.try_enter(|ctx| {
                let Value::Function(finish) = ctx.get_global("__ppu_sprites_finish") else {
                    unreachable!("sprites.lua defines the finish")
                };
                let pin = plan
                    .size_sel
                    .map_or(Value::Nil, |p| Value::Integer(p as i64));
                let args = (
                    pin,
                    plan.arts.len() as i64,
                    plan.sets_char_base,
                    plan.sets_obj_layer,
                );
                Ok(ctx.stash(Executor::start(ctx, finish, args)))
            })
            .and_then(|ex| lua.execute::<()>(&ex))
            .map_err(static_error_to_lua)?;
            let failed: Option<(i64, String)> = lua.enter(|ctx| {
                match (
                    ctx.get_global("__ppu_sprites_fail_site"),
                    ctx.get_global("__ppu_sprites_fail"),
                ) {
                    (Value::Integer(site), Value::String(m)) => {
                        Some((site, m.to_str().unwrap_or("").to_string()))
                    }
                    _ => None,
                }
            });
            if let Some((site, message)) = failed {
                let at = &plan.arts[(site.max(1) - 1) as usize];
                return Err(LuaError {
                    file: Some(at.file.clone()),
                    line: Some(at.line),
                    message,
                });
            }
            let cells: Vec<u64> = lua.enter(|ctx| {
                let Value::Table(t) = ctx.get_global("__ppu_sprites_cells") else {
                    unreachable!("sprites.lua defines the cells")
                };
                (1..=plan.arts.len())
                    .map(|k| match t.get(ctx, k as i64) {
                        Value::Integer(n) => n.max(0) as u64,
                        Value::Number(n) => n.max(0.0) as u64,
                        _ => 0,
                    })
                    .collect()
            });
            let layout = plan.layout(&cells).map_err(|e| LuaError {
                file: Some(e.file),
                line: Some(e.line),
                message: format!("error[snes.sprites]: {}", e.message),
            })?;
            lua.enter(|ctx| {
                let t = Table::new(&ctx);
                for (i, (first, size)) in layout.iter().enumerate() {
                    let g = Table::new(&ctx);
                    g.set(ctx, "first", *first as i64).unwrap();
                    g.set(ctx, "size", *size as i64).unwrap();
                    g.set(ctx, "sort", plan.groups[i].sort).unwrap();
                    g.set(ctx, "keep", plan.groups[i].keep).unwrap();
                    // A scene's group shows only while one of its scenes runs (another scene's may share its slots).
                    if !plan.groups[i].scenes.is_empty() {
                        let scenes = Table::new(&ctx);
                        for k in &plan.groups[i].scenes {
                            scenes
                                .set(ctx, ctx.intern(plan.scenes[k - 1].0.as_bytes()), true)
                                .unwrap();
                        }
                        g.set(ctx, "scene", scenes).unwrap();
                    }
                    t.set(ctx, i as i64 + 1, g).unwrap();
                }
                ctx.set_global("__ppu_sprites_layout", t).unwrap();
            });
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
        let start_scene = &self.start_scene;
        let start_preview = self.start_preview;
        let prev_scene = self
            .prev_run
            .as_ref()
            .and(self.scenes.as_ref())
            .map(|sc| (sc.list[sc.current].file.clone(), sc.f0, sc.t0));
        // A fade under way goes on through an edit that stays in the running scene (restored after its entry).
        let prev_fade: Option<Vec<Option<i64>>> = prev_scene.as_ref().map(|_| {
            self.lua.borrow_mut().enter(|ctx| SCENE_FADE.iter().map(|n| ctx.get_global(*n).to_int()).collect())
        });
        let mut stayed = false;
        let scenes = lua.enter(|ctx| {
            ctx.set_global("__ppu_scenes_open", false).unwrap();
            let Value::Table(list) = ctx.get_global("__ppu_scenes") else {
                return None;
            };
            let list: Vec<Scene> = (1..=list.length())
                .filter_map(|i| match list.get(ctx, i) {
                    Value::Table(s) => Some(s),
                    _ => None,
                })
                .filter_map(|s| {
                    let Value::Function(frame) = s.get(ctx, "frame") else {
                        return None;
                    };
                    Some(Scene {
                        file: s.get(ctx, "file").to_string(),
                        init: match s.get(ctx, "init") {
                            Value::Function(f) => Some(ctx.stash(f)),
                            _ => None,
                        },
                        frame: ctx.stash(frame),
                        frame_file: function_chunk_name(&frame),
                        preview: match s.get(ctx, "preview") {
                            Value::Function(f) => Some(ctx.stash(f)),
                            _ => None,
                        },
                    })
                })
                .collect();
            let chosen = start_scene
                .as_ref()
                .and_then(|file| list.iter().position(|s| s.file == *file));
            // A recompile of the running program stays in the running scene, on its own clock.
            let carried = prev_scene
                .as_ref()
                .filter(|_| chosen.is_none())
                .and_then(|(file, ..)| list.iter().position(|s| s.file == *file));
            stayed = carried.is_some();
            let (f0, t0) = carried
                .and(prev_scene.as_ref())
                .map_or((0, 0.0), |p| (p.1, p.2));
            let boot = match (chosen, ctx.get_global("__ppu_scene_next").to_int()) {
                (Some(k), _) => k,
                (None, Some(n)) if n >= 1 && (n as usize) <= list.len() => n as usize - 1,
                _ => 0,
            };
            ctx.set_global("__ppu_scene_next", Value::Nil).unwrap();
            (!list.is_empty()).then_some(Scenes {
                list,
                boot,
                entry: carried.unwrap_or(boot),
                chosen: chosen.is_some() && start_preview,
                current: carried.unwrap_or(boot),
                f0,
                t0,
                placements: vec![],
                timers: vec![],
                ranges: (0, 0),
                obj_bases: 0,
                samples: 0,
                songs: 0,
            })
        });
        if scenes.is_some() {
            if let Some(err) = [(&frame_fn, &frame_file, "frame"), (&init_fn, &init_file, "init")]
                .into_iter()
                .find_map(|(f, file, name)| f.as_ref().map(|_| (file.clone(), name)))
                .map(|(file, name)| LuaError {
                    file,
                    line: None,
                    message: format!("error[snes.scenes]: a program of scenes runs each scene's own {name}(): make this {name}() local to a scene module and return it, return {{init = init, frame = frame}}"),
                })
            {
                return Err(err);
            }
        }

        let prev_score = self.lua.borrow_mut().enter(score_id);
        // The audio timeline counted from the scene's entry: back to program time.
        if let Some(old) = &self.scenes {
            let f0 = scenes.as_ref().map_or(0, |s| s.f0);
            self.last_f = self.last_f.map(|f| (f + old.f0).saturating_sub(f0));
        }
        self.scenes = scenes;
        self.lua = Rc::new(RefCell::new(lua));
        self.keys = keys;
        *self.mirror.borrow_mut() = Mirror::default();
        self.frame_fn = frame_fn;
        self.frame_file = frame_file;
        self.sprites = sprite_plan.is_some();
        self.init_fn = init_fn;
        self.controls_fn = controls_fn;
        self.controls_vram = controls_vram;
        // Snapshot the OLD recorder's placements before swapping it out: if
        // this compile fails below, nothing new was actually written to
        // ARAM, so these are what ARAM still holds — restoring them (instead
        // of clearing) keeps `dsp_view()` truthful on the failure path.
        let prev_samples = self.dma.samples.borrow().clone();
        let prev_dma = std::mem::replace(&mut self.dma, rec.clone());
        self.memory = Memory::new();
        // The old stashed hooks belong to the dead VM just swapped out —
        // drop them now regardless of whether init() below succeeds.
        self.timers.clear();

        // The program's own `init()`. On failure the init window closes, and
        // nothing was written to ARAM yet (that happens after the `?` below),
        // so the OLD program's placements (recorded above, before this
        // compile's dma() calls) are restored — a failed setup keeps
        // reporting what ARAM still actually holds.
        let mut res = Ok(());
        if let Some(func) = self.init_fn.clone() {
            let mut l = self.lua.borrow_mut();
            let ex = l.enter(|ctx| {
                let f = ctx.fetch(&func);
                ctx.stash(Executor::start(ctx, f, ()))
            });
            if let Err(e) = l.execute::<()>(&ex) {
                let mut err = located(&mut l, &ex, e);
                err.file = err.file.or(init_file.clone());
                res = Err(err);
            }
        }
        // A program of scenes boots into its first: the load's records are every scene's.
        if let (Ok(()), Some(sc)) = (&res, self.scenes.as_mut()) {
            sc.placements = rec.placements.borrow().clone();
            sc.timers = rec.timers.borrow().clone();
            sc.ranges = (
                rec.vram_ranges.borrow().len(),
                rec.cgram_ranges.borrow().len(),
            );
            sc.obj_bases = rec.obj_base.borrow().len();
            sc.samples = rec.samples.borrow().len();
            sc.songs = rec.songs.borrow().len();
            let entry = sc.entry;
            res = self.enter_scene(entry, None, true);
            if let (Ok(()), true, Some(fade)) = (&res, stayed, &prev_fade) {
                self.lua.borrow_mut().enter(|ctx| {
                    for (name, v) in SCENE_FADE.iter().zip(fade) {
                        ctx.set_global(*name, v.map_or(Value::Nil, Value::Integer)).unwrap();
                    }
                });
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
                    "error[snes.dma]: dma: '{name}' overlaps the echo region (0x{echo_lo:04x}-0x{hi:04x}, \
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
        self.place_samples();
        // Recompile == drop + re-register, then re-phase onto the timeline
        // at the current position (not at compile time).
        self.timers = timer_hooks(rec.timers.borrow_mut().drain(..));
        self.carry_songs(&prev_dma, prev_score);
        self.realign(self.last_f.map_or(0, |f| 2 * audio_start_s(f as u64 + 1)));
        self.program_sources = files
            .iter()
            .map(|(name, source)| ((*name).to_string(), (*source).to_string()))
            .collect();
        self.frames_note = frames_note(files);
        self.source_dirty = false;
        self.recompiled = true;
        // The load is good: the running program's file-level state moves into it, and it becomes
        // the program the next load carries from.
        let run = crate::carry::Run {
            lua: self.lua.clone(),
            mains,
            setup_mark: rec.setup_mark.borrow().clone(),
            roots: self
                .timers
                .iter()
                .filter_map(|t| match &t.hook {
                    Hook::Lua(f) => Some(f.clone()),
                    Hook::Song(_) => None,
                })
                .collect(),
        };
        self.not_carried = self.prev_run.take().map_or_else(Vec::new, |old| {
            let shown = call_world_hook(&mut old.lua.borrow_mut(), "__ppu_world_shown", "");
            let mut lines = crate::carry::carry(&old, &run);
            // The author's camera came back over init()'s: libraries that stream from it redraw,
            // and a world that had entered another map goes back to it.
            rec.hot_swap.set(true);
            let kept = call_world_hook(&mut self.lua.borrow_mut(), "__ppu_world_hotswap", &shown);
            rec.hot_swap.set(false);
            lines.extend(kept.lines().map(str::to_string));
            lines
        });
        self.prev_run = Some(run);
        self.held = None;
        Ok(())
    }

    /// Compile the live pokes chunk against the tracked
    /// `__ppu_controls_env` (installed by `controls_env.lua` — see
    /// `set_sources`'s fresh-VM setup) and stash `__ppu_pokes` if the chunk
    /// defines one (`None` is treated exactly like no chunk at all). Errors
    /// carry the label [`POKES`].
    ///
    /// The whole load runs inside its own begin/restore bracket: a
    /// RUNTIME failure partway through the chunk has already executed some
    /// of its top-level writes straight onto the real globals (only logged
    /// for undo) — `__ppu_controls_restore` unwinds exactly those before the
    /// error returns, so a partially-applied controls text never leaves
    /// stray globals live in the VM. Before executing the chunk, any
    /// previously-stashed `__ppu_pokes` is nil'd out THROUGH the tracked env
    /// too, so a text that no longer defines one doesn't leave the old
    /// chunk's function re-stashed below, and that clear is itself undone by
    /// the same restore on a load failure. On success the bracket is
    /// re-begun (fresh, empty log) so nothing done here is later undone by
    /// `frame()`'s own bracket.
    fn load_controls(lua: &mut Lua, src: &str) -> Result<Option<StashedFunction>, LuaError> {
        call_controls_hook::<()>(lua, "__ppu_controls_begin");

        // Nil the previous entry points THROUGH the tracked env
        // first, so a text that no longer defines one doesn't re-stash the old
        // chunk's function below — logged for undo like any other write, so
        // a load failure below puts the old one right back.
        let clear = lua.try_enter(|ctx| {
            let env = controls_env(ctx);
            let closure = Closure::load_with_env(
                ctx,
                None,
                "__ppu_pokes = nil; __ppu_pokes_vram = nil".as_bytes(),
                env,
            )?;
            Ok(ctx.stash(Executor::start(ctx, closure.into(), ())))
        });
        clear
            .and_then(|ex| lua.execute::<()>(&ex))
            .unwrap_or_else(|e| panic!("clearing __ppu_pokes must not fail: {e}"));

        let load = lua.try_enter(|ctx| {
            let env = controls_env(ctx);
            let closure = Closure::load_with_env(ctx, Some(POKES), src.as_bytes(), env)?;
            Ok(ctx.stash(Executor::start(ctx, closure.into(), ())))
        });
        if let Err(e) = load.and_then(|ex| lua.execute::<()>(&ex)) {
            call_controls_hook::<()>(lua, "__ppu_controls_restore");
            return Err(static_error_to_lua(e).in_file(POKES));
        }
        call_controls_hook::<()>(lua, "__ppu_controls_begin");
        // Painted tiles: run `__ppu_pokes_vram()` once, capturing its `vr()` words
        // (see `__ppu_controls_capture_vram`); `read_controls_vram` lifts them
        // into Rust. Any other write it made is undone like a failed load's.
        let cap = lua.try_enter(|ctx| match ctx.get_global("__ppu_controls_capture_vram") {
            Value::Function(f) => Ok(ctx.stash(Executor::start(ctx, f, ()))),
            _ => panic!("controls_env.lua must define __ppu_controls_capture_vram"),
        });
        let captured = cap.and_then(|ex| lua.execute::<()>(&ex));
        call_controls_hook::<()>(lua, "__ppu_controls_restore");
        if let Err(e) = captured {
            return Err(static_error_to_lua(e).in_file(POKES));
        }
        call_controls_hook::<()>(lua, "__ppu_controls_begin");
        Ok(lua.enter(|ctx| match ctx.get_global("__ppu_pokes") {
            Value::Function(f) => Some(ctx.stash(f)),
            _ => None,
        }))
    }

    /// Hand the engine the Studio's live pokes: a chunk of optional top-level `dma("name", {...})`
    /// placements, an optional `function __ppu_pokes_vram()` (painted tiles, captured once per
    /// load) and an optional `function __ppu_pokes()` applied through a tracked,
    /// restore-every-frame pass. Every load runs it first, inside the init window, so its `dma()`
    /// lines are setup placements; it is kept across every later load (recompile, reset) and is
    /// never one of the program's files. Errors carry `file: "pokes"`. "" drops the pokes.
    /// On a running program a chunk whose `dma(` lines changed is stored and the next frame
    /// recompiles (its errors surface there); any other chunk is reloaded in place, and one that
    /// fails to load is refused while the previous one keeps applying. Before the first program it
    /// is kept unchecked.
    pub fn set_pokes(&mut self, chunk: &str) -> Result<(), LuaError> {
        if chunk == self.pokes {
            return Ok(());
        }
        self.shown = None; // a poke shows at once, between updates too
                           // No program yet: no tracked env to load into; the first load picks the chunk up.
        if !self.program_sources.is_empty() {
            // dma() only works inside the init window, which an in-place load never reopens.
            if dma_lines(chunk) != dma_lines(&self.pokes) {
                self.source_dirty = true;
            } else if chunk.is_empty() {
                self.controls_fn = None;
                self.controls_vram.clear();
            } else {
                // The unchanged `dma(` lines are already in effect; blanked, line numbers kept.
                let mut l = self.lua.borrow_mut();
                self.controls_fn = Self::load_controls(&mut l, &without_dma_lines(chunk))?;
                self.controls_vram = Self::read_controls_vram(&mut l);
            }
        }
        self.pokes = chunk.to_string();
        Ok(())
    }

    /// The words the last `load_controls` captured from `__ppu_pokes_vram()`.
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

    /// Write the recorded sample placements into ARAM and the sample directory.
    /// The names were validated against the store at `dma()` call time.
    fn place_samples(&mut self) {
        for sp in self.dma.samples.borrow().iter() {
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
    }

    /// Enter scene `k` with a cold boot, as a cartridge does: the PPU at power-on (registers, OAM,
    /// the poke tables) showing the scene's placements and the program's, no sprite group holding
    /// a sprite, then the scene's init() in a setup window of its own, its records replacing the
    /// previous entry's. At load (`at` is `None`) set_sources finishes the boot. Entered on update
    /// `at`, the sound chip power-cycles too: ARAM holds the samples again, and the scene's timers
    /// and songs start over on an audio timeline that, like its `t` and `f`, counts from `at`.
    /// Booting into the switcher's start scene (at load, or a jump back before its entry) is a
    /// Scene view entry: the scene's `preview` runs first, before the cold boot, as if an earlier
    /// scene had set what it sets. Power-on and `scenes.go` never run it.
    /// `staged` is false for a `scenes.go` entry, which never previews.
    fn enter_scene(
        &mut self,
        k: usize,
        at: Option<(f64, u32)>,
        staged: bool,
    ) -> Result<(), LuaError> {
        self.pads_before = None;
        let sc = self.scenes.as_mut().expect("a program of scenes");
        sc.current = k;
        if let Some((t, f)) = at {
            (sc.t0, sc.f0) = (t, f);
        }
        let scene = &sc.list[k];
        let file = scene.file.clone();
        let mine = |o: &Option<String>| o.as_ref().is_none_or(|o| *o == file);
        let rec = self.dma.clone();
        *rec.placements.borrow_mut() = sc
            .placements
            .iter()
            .filter(|p| mine(&p.owner))
            .cloned()
            .collect();
        *rec.timers.borrow_mut() = sc.timers.iter().filter(|t| mine(&t.4)).cloned().collect();
        rec.loads.borrow_mut().clear();
        rec.world_animated.borrow_mut().clear();
        rec.vram_ranges.borrow_mut().truncate(sc.ranges.0);
        rec.cgram_ranges.borrow_mut().truncate(sc.ranges.1);
        rec.obj_base.borrow_mut().truncate(sc.obj_bases);
        rec.samples.borrow_mut().truncate(sc.samples);
        {
            let mut songs = rec.songs.borrow_mut();
            songs.truncate(sc.songs);
            for p in songs.iter_mut() {
                p.set_anchor(p.declared);
                p.playing = p.declared != Anchor::Stopped;
                p.at_false = p.declared == Anchor::Stopped;
                p.rewind();
            }
        }
        let init = scene.init.clone();
        let preview = scene
            .preview
            .clone()
            .filter(|_| staged && sc.chosen && k == sc.boot && at.is_none_or(|a| a == (0.0, 0)));
        self.frame_fn = Some(scene.frame.clone());
        self.frame_file = scene.frame_file.clone();
        *self.mirror.borrow_mut() = Mirror::default();
        if at.is_some() {
            // The sound chip's power cycle, before init() sets up the scene's voices.
            self.unmixed_dsp = None;
            self.mix_kon = 0;
            self.mix_koff = 0;
            self.dma.sfx_hold.set([0; 8]);
            self.dma.sfx_own.set([0; 8]);
            self.dma.sfx_auto.set(0);
            self.dma.kon_open.set(0);
            // A host's write log (start_dsp_log) listens across the power cycle.
            let log = self.dsp.log.take();
            self.dsp = power_on_dsp();
            self.dsp.log = log;
            self.aram.fill(0);
            self.audio.clear();
            self.last_f = None;
            self.recompiled = false;
            let dsp = &self.dsp;
            self.lua.borrow_mut().enter(|ctx| {
                // shortcut: voice[]/dsp are rebuilt as at a recompile, not reset in place.
                seed_dsp_tables(ctx, dsp, None);
                reg_table(ctx, ctx.globals(), "aram", true);
                ctx.set_global("__dsp_kon", 0).unwrap();
                ctx.set_global("__dsp_koff", 0).unwrap();
            });
        }
        let res = {
            let mut l = self.lua.borrow_mut();
            let keys = &self.keys;
            let run = |l: &mut Lua, func: StashedFunction| {
                let ex = l.enter(|ctx| {
                    let f = ctx.fetch(&func);
                    ctx.stash(Executor::start(ctx, f, ()))
                });
                l.execute::<()>(&ex).map_err(|e| {
                    let mut err = located(l, &ex, e);
                    err.file = err.file.or(Some(file.clone()));
                    err
                })
            };
            let mut res = preview.map_or(Ok(()), |p| run(&mut l, p));
            // A load's boot or a jump back before the entry starts with no fade; a switch (the engine's or the
            // Studio's enter_pending_scene) keeps its fade in.
            let unfaded = staged;
            l.enter(|ctx| {
                power_on(ctx, &keys.fetch(ctx));
                ctx.set_global("__ppu_scene_next", Value::Nil).unwrap();
                if unfaded {
                    for (name, v) in [("__ppu_scene_out", 0), ("__ppu_scene_in", 1), ("__ppu_scene_fade", 0)] {
                        ctx.set_global(name, v).unwrap();
                    }
                    ctx.set_global("__ppu_scene_after", Value::Nil).unwrap();
                    ctx.set_global("__ppu_scene_shown", Value::Nil).unwrap();
                }
                // What the scene records from here on, init or a stage load, is its own.
                ctx.set_global("__ppu_scene_file", ctx.intern(file.as_bytes()))
                    .unwrap();
                ctx.set_global("__ppu_sprites_scene", ctx.intern(file.as_bytes()))
                    .unwrap();
            });
            rec.active.set(true);
            for func in [
                l.enter(|ctx| match ctx.get_global("__ppu_sprites_reset") {
                    Value::Function(f) => Some(ctx.stash(f)),
                    _ => None,
                }),
                init,
            ]
            .into_iter()
            .flatten()
            {
                if res.is_err() {
                    break;
                }
                res = run(&mut l, func);
            }
            res
        };
        if at.is_none() {
            return res;
        }
        rec.active.set(false);
        res?;
        // shortcut: the echo region is checked against the samples at load only, not per scene.
        self.place_samples();
        // A score{} set up at load (playing or `at = false`) opened the master volume, which the
        // power cycle closed: open it again, as score{} does, unless the scene's init set a level.
        let scored = self
            .dma
            .timers
            .borrow()
            .iter()
            .any(|t| matches!(t.2, Hook::Song(_)));
        if scored {
            self.lua.borrow_mut().enter(|ctx| {
                if let Value::Table(d) = ctx.get_global("dsp") {
                    let silent = match d.get(ctx, "mvol") {
                        Value::Table(m) => {
                            m.get(ctx, "l").to_int().unwrap_or(0) == 0
                                && m.get(ctx, "r").to_int().unwrap_or(0) == 0
                        }
                        _ => true,
                    };
                    if silent {
                        let m = Table::new(&ctx);
                        m.set(ctx, "l", 127).unwrap();
                        m.set(ctx, "r", 127).unwrap();
                        d.set(ctx, "mvol", m).unwrap();
                    }
                }
            });
        }
        self.timers = timer_hooks(rec.timers.borrow_mut().drain(..));
        Ok(())
    }

    /// Recompile the cached program sources (`frame()`'s dirty-source path and `reset()`).
    fn recompile(&mut self) -> Result<(), LuaError> {
        let files = self.program_sources.clone();
        // set_sources keeps only the project files it is given: hand the maps back with the code.
        let project: Vec<_> = self.project_files.borrow().clone().into_iter().collect();
        let refs: Vec<_> = files
            .iter()
            .chain(project.iter())
            .map(|(n, s)| (n.as_str(), s.as_str()))
            .collect();
        self.set_sources(&refs)
    }

    /// Run (t=0): a full power cycle of the sound chip. Fresh `Dsp`, zeroed
    /// ARAM (echo RAM included, since it's just the top of ARAM), then the
    /// same recompile path `frame()` uses for a dirty source list — so
    /// `dma()` placements are re-written into the freshly-zeroed ARAM and
    /// `timer()` hooks re-register at t = 0 (`last_f` is cleared before the
    /// recompile), exactly like a fresh engine loaded with the same program.
    /// Clearing `dma.samples` first
    /// keeps `dsp_view()` truthful if that recompile itself fails (same
    /// contract as `set_sources`'s own failure path). Contrast `set_sources`
    /// (recompile): that keeps the DSP/ARAM running and re-phases timers to
    /// the current timeline position — it never resets anything.
    pub fn reset(&mut self) -> Result<(), LuaError> {
        self.prev_run = None; // Run powers on: nothing carries
        self.pads_before = None;
        self.violations.clear();
        self.violation_sink.borrow_mut().clear();
        self.unmixed_dsp = None;
        self.mix_kon = 0;
        self.mix_koff = 0;
        self.dma.sfx_hold.set([0; 8]);
        self.dma.sfx_own.set([0; 8]);
        self.dma.sfx_auto.set(0);
        self.dma.kon_open.set(0);
        self.dsp = power_on_dsp();
        self.aram.fill(0);
        self.audio.clear();
        self.last_f = None;
        self.dma.samples.borrow_mut().clear();
        // No song survives a power cycle: with no old players, the
        // recompile has nothing to carry (`carry_songs`), and the voices
        // are already silent in the fresh DSP.
        self.dma.songs.borrow_mut().clear();
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

    /// The second controller's mask, published as `pad2` (same bit order).
    pub fn set_pad2(&mut self, mask: u16) {
        self.pad2 = mask;
    }

    /// Bind the `sram` global to a save blob (JSON object or array). Lands in
    /// the live VM now and seeds every later recompile, so `init()` sees it.
    /// Anything else (invalid JSON, a scalar) resets to an empty table.
    pub fn set_sram(&mut self, json: &str) {
        let v: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
        self.sram_json = if v.is_object() || v.is_array() {
            crate::save::canonical_json(&v)
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
    /// `__ppu_pokes()` call the program made before erroring) never bakes
    /// into the next frame's baseline.
    fn restore_controls(&self) {
        if self.controls_fn.is_some() {
            let mut l = self.lua.borrow_mut();
            call_controls_hook::<()>(&mut l, "__ppu_controls_restore");
        }
    }

    /// Build-time evaluation (the SNES exporter bakes tables with it): call the
    /// global function `name` with integer arguments and return its result, a
    /// Lua sequence of integers. Runs after the program's `init()`.
    pub fn call_ints(&mut self, name: &str, args: &[i64]) -> Result<Vec<i64>, LuaError> {
        self.call_sequence(name, args, "integer", |v| v.to_int())
    }

    /// As `call_ints`, for a sequence of numbers (floats kept as the program computed them).
    pub fn call_numbers(&mut self, name: &str, args: &[i64]) -> Result<Vec<f64>, LuaError> {
        self.call_sequence(name, args, "number", |v| v.to_number())
    }

    fn call_sequence<T>(
        &mut self,
        name: &str,
        args: &[i64],
        what: &str,
        item: impl Fn(Value) -> Option<T>,
    ) -> Result<Vec<T>, LuaError> {
        if self.source_dirty {
            self.recompile()?;
        }
        let mut l = self.lua.borrow_mut();
        let ex = l.enter(|ctx| {
            let f = match ctx
                .globals()
                .get(ctx, piccolo::String::from_slice(&ctx, name.as_bytes()))
            {
                Value::Function(f) => f,
                _ => return Err(format!("no global function {name}")),
            };
            Ok(ctx.stash(Executor::start(ctx, f, piccolo::Variadic(args.to_vec()))))
        });
        let ex = ex.map_err(|message| LuaError {
            message,
            line: None,
            file: None,
        })?;
        l.finish(&ex);
        l.try_enter(|ctx| {
            let Value::Table(t) = ctx.fetch(&ex).take_result::<Value>(ctx)?? else {
                return Ok(Err(format!("{name} must return a sequence of {what}s")));
            };
            let mut out = Vec::new();
            loop {
                match t.get(ctx, out.len() as i64 + 1) {
                    Value::Nil => break,
                    v => match item(v) {
                        Some(n) => out.push(n),
                        None => return Ok(Err(format!("{name} returned a non-{what}"))),
                    },
                }
            }
            Ok(Ok(out))
        })
        .map_err(static_error_to_lua)?
        .map_err(|message| LuaError {
            message,
            line: None,
            file: None,
        })
    }

    /// The project's update target, `targetHz` (25, 30, 50 or 60; `None`: a `---@snes frames N`
    /// line in the sources decides, else one update per video frame). At 25 or 30 Hz `frame()`
    /// runs on every second video frame only, as on the cartridge.
    pub fn set_target_hz(&mut self, hz: Option<u8>) -> Result<(), String> {
        if hz.is_some_and(|hz| ![25, 30, 50, 60].contains(&hz)) {
            return Err(format!(
                "targetHz must be 25, 30, 50 or 60, not {}",
                hz.unwrap_or(0)
            ));
        }
        self.target_hz = hz;
        self.shown = None;
        Ok(())
    }

    /// The run frame the host's `f = 0` stands for (see `run_base`); kept across loads and resets.
    pub fn set_run_base(&mut self, f: u32) {
        self.run_base = f;
    }

    /// Video frames per update (30 Hz: 2): the build setting wins over the sources' annotation. A
    /// `scenes.go` lands on the next update, this many video frames after the one that asked.
    pub fn update_frames(&self) -> u32 {
        match self.target_hz {
            Some(25 | 30) => 2,
            Some(_) => 1,
            None => self.frames_note,
        }
    }

    /// One video frame (`t`, the host's clock at frame `f`, anchors a scene's audio only: frame() gets [`cart_t`]). A program slower than the video rate (30 Hz: an update every second video
    /// frame) runs its `frame()` on the cartridge's updates only, with their t and f, counted from
    /// power-on or the scene's entry: between two updates the picture holds and the sound plays on.
    /// A frame between updates that no update was run for (a jump, an edit) runs the update it
    /// belongs to.
    pub fn frame(&mut self, t: f64, f: u32) -> Result<LineTable, LuaError> {
        let out = self.video_frame(t, f);
        self.note_violations(Some(self.update_f));
        out
    }

    fn video_frame(&mut self, t: f64, f: u32) -> Result<LineTable, LuaError> {
        if self.source_dirty {
            self.recompile()?;
        }
        let n = self.update_frames();
        if n == 1 {
            return self.update(t, f, f);
        }
        let f0 = self.scenes.as_ref().map_or(0, |sc| sc.f0);
        let off = if f >= f0 { (f - f0) % n } else { 0 };
        let u = f - off;
        if let Some((_, shown)) = self
            .shown
            .as_ref()
            .filter(|(s, _)| off != 0 && *s == u && !self.recompiled)
        {
            let shown = shown.clone();
            let video = f - f0;
            if self.last_f == Some(video) {
                self.audio.clear(); // a paused repeat: its span is already rendered
            } else {
                self.render_frame_audio(video)?;
            }
            return shown;
        }
        // The host's clock at the update (a scene entry's anchors its audio); frame() gets the cartridge's t.
        let t = if off == 0 { t } else { t * u as f64 / f as f64 };
        let out = self.update(t, u, f);
        self.shown = Some((u, out.clone()));
        out
    }

    /// The update at program frame `f` (t its time), drawn on video frame `video` (`f` but between
    /// updates): the scenes, the program's frame() and the hooks see `f`; the audio span is `video`'s.
    fn update(&mut self, t: f64, f: u32, video: u32) -> Result<LineTable, LuaError> {
        self.update_f = f;
        // The same update again: a paused repeat, or an edit's replay (`recompiled`). The sprites' key
        // clocks hold on a paused repeat only; the fade shows the step it took on either.
        let f0 = self.scenes.as_ref().map_or(0, |sc| sc.f0);
        let same = self.last_f.map(|l| l + f0) == Some(f);
        let repeat = !self.recompiled && same;
        let again = same;
        // Before a scene's entry: its init() plays in the update it enters on, as on a cartridge.
        self.lua.borrow_mut().enter(|ctx| {
            ctx.set_global("__ppu_update", f as i64).unwrap();
            ctx.set_global("__ppu_sprites_again", repeat).unwrap();
        });
        // A program of scenes: `scenes.go` lands on the next new update, a cold boot there (a
        // paused repeat never switches); a jump to before the scene's entry boots the first scene.
        if let Some(sc) = &self.scenes {
            let (boot, f0, count) = (sc.boot, sc.f0, sc.list.len());
            if f < f0 {
                self.enter_scene(boot, Some((0.0, 0)), true)?;
            } else if !repeat {
                let next = self
                    .lua
                    .borrow_mut()
                    .enter(|ctx| ctx.get_global("__ppu_scene_next").to_int());
                if let Some(n) = next.filter(|n| (1..=count as i64).contains(n)) {
                    self.enter_scene(n as usize - 1, Some((t, f)), false)?;
                }
            }
        }
        // The scene sees its own t and f, counted from its entry; t is the cartridge's, from the run frames.
        let b = self.run_base;
        let (t, f, video) = match &self.scenes {
            Some(sc) => (cart_t(b + f) - cart_t(b + sc.f0), f - sc.f0, video - sc.f0),
            None => (cart_t(b + f) - cart_t(b), f, video),
        };
        // Paused: the first run of an update keeps the state it starts from, and a repeat starts from it again.
        if self.hold {
            match (&self.held, &self.prev_run) {
                (Some((at, snap)), Some(run)) if repeat && *at == f => {
                    crate::carry::apply(snap, run);
                }
                (_, Some(run)) => self.held = Some((f, crate::carry::capture(run))),
                _ => {}
            }
        }
        self.dma.raster.borrow_mut().take();
        // Reset the per-frame hook registry, then run frame(t,f) once.
        {
            let mut l = self.lua.borrow_mut();
            let (pad, pad2) = (self.pad, self.pad2);
            // The previous update is one update's video frames back (2 at 30 Hz), not one frame.
            let step = self.update_frames();
            let (was, was2) = match self.pads_before {
                Some((last, pads, _)) if last + step == f => pads,
                Some((last, _, was)) if last == f => was,
                _ => (pad, pad2),
            };
            self.pads_before = Some((f, (pad, pad2), (was, was2)));
            l.enter(|ctx| {
                ctx.set_global("__ppu_hooks", Table::new(&ctx)).unwrap();
                set_pad_table(ctx, "pad", pad, was);
                set_pad_table(ctx, "pad2", pad2, was2);
            });
            // Begin the controls undo bracket BEFORE the program's own
            // frame() runs, not after — an explicit `__ppu_pokes()` call
            // from inside frame() (or a timer hook) runs under the tracked
            // env too, so its writes must be logged from the start or they
            // escape undoing entirely. One begin() now covers the whole
            // frame — Phase A no longer calls it, and the single restore()
            // at the end of this function undoes everything logged since.
            if self.controls_fn.is_some() {
                call_controls_hook::<()>(&mut l, "__ppu_controls_begin");
            }
            self.dma.loaded.set(false);
            if let Some(frame) = self.frame_fn.clone() {
                let ex = l.enter(|ctx| {
                    let func = ctx.fetch(&frame);
                    ctx.stash(Executor::start(ctx, func, (t, f as i64)))
                });
                self.dma.in_frame.set(true);
                let done = l.execute::<()>(&ex);
                self.dma.in_frame.set(false);
                // A load lands while the update stays blanked: unblanking in the same update is an error.
                let done = done.map_err(|e| located(&mut l, &ex, e)).and_then(|()| {
                    if self.dma.loaded.get() && !l.enter(|ctx| ctx.get_global("force_blank").to_bool()) {
                        Err(LuaError {
                            message: "error[snes.dma]: dma in frame() needs force_blank to stay true until frame() returns: unblank on a later update".into(),
                            line: None,
                            file: self.frame_file.clone(),
                        })
                    } else {
                        Ok(())
                    }
                });
                if let Err(mut err) = done {
                    // Restore here too (`l` is already borrowed, so not via
                    // `restore_controls`) — an explicit `__ppu_pokes()`
                    // call made before this error would otherwise stay
                    // logged into the next frame's stale bracket.
                    if self.controls_fn.is_some() {
                        call_controls_hook::<()>(&mut l, "__ppu_controls_restore");
                    }
                    err.file = err.file.or_else(|| self.frame_file.clone());
                    return Err(err);
                }
            }
            if self.sprites {
                // The video frames this update spans: the keyed animation clocks advance by them.
                let step = self.update_frames() as i64;
                let ex = l.enter(|ctx| {
                    let func = ctx.get_global("__ppu_sprites_flush");
                    let Value::Function(func) = func else {
                        unreachable!("sprites.lua defines the flush")
                    };
                    ctx.stash(Executor::start(ctx, func, step))
                });
                if let Err(e) = l.execute::<()>(&ex) {
                    if self.controls_fn.is_some() {
                        call_controls_hook::<()>(&mut l, "__ppu_controls_restore");
                    }
                    return Err(static_error_to_lua(e));
                }
            }
        }

        // A fade under way (scenes.go(s, {fade = n})) owns brightness: its step follows frame(), as on the cart.
        if self.scenes.is_some() {
            let mut l = self.lua.borrow_mut();
            let ex = l.enter(|ctx| {
                let Value::Function(func) = ctx.get_global("__ppu_scene_fade_step") else {
                    unreachable!("luau.lua defines the fade step")
                };
                // The step in video frames this update spans; a paused repeat takes none.
                ctx.stash(Executor::start(ctx, func, if again { 0 } else { self.update_frames() as i64 }))
            });
            l.execute::<()>(&ex).map_err(static_error_to_lua)?;
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
                    self.dma
                        .placements
                        .borrow()
                        .iter()
                        .chain(self.dma.loads.borrow().iter().map(|l| &l.placement)),
                    &self.source_store.borrow(),
                    &mut self.reports,
                    &mut self.memory,
                );
                animate_worlds(&self.dma, &self.source_store.borrow(), f, &mut self.memory);
                let columns = if self.picture_memory { read_memory(ctx, &mut self.memory) } else { Ok(()) };
                (columns, read_state(ctx, &keys.fetch(ctx), &mut mirror))
            })
        };
        let (columns, defaults) = defaults;
        if let Err(message) = columns {
            self.restore_controls();
            return Err(LuaError { message, line: None, file: self.frame_file.clone() });
        }

        // M12/audio: walk this frame's audio span sample-accurately against
        // the registered `timer(n, div, fn)` hooks — BEFORE the hdma hooks
        // below are even collected, so timers and hdma interleave in a
        // fixed order within one frame (timers, then hdma) even though both
        // may touch the same globals (see `render_frame_audio`). Consequence
        // (Spec-locked, not to be reordered): an hdma hook's `kon`/`koff`/
        // `voice[]`/`dsp` writes are flushed to the DSP only at the NEXT
        // frame's offset-0 flush, one frame later than a `frame()`-body or
        // timer-hook write — see the `kon`/`koff` callback comment below.
        if self.last_f == Some(video) && !self.recompiled {
            // Repeat of the last rendered frame (paused refresh): its span
            // is already rendered, so no audio, no hooks, no DSP writes —
            // and key events issued by this repeat's body belong to the
            // frame already rendered, so they are discarded. The first
            // frame after a recompile is a replay instead (see
            // `recompiled`): it re-renders the span.
            self.audio.clear();
            let mut l = self.lua.borrow_mut();
            l.enter(|ctx| {
                ctx.set_global("__dsp_kon", 0).unwrap();
                ctx.set_global("__dsp_koff", 0).unwrap();
            });
        } else {
            self.render_frame_audio(video)
                .inspect_err(|_| self.restore_controls())?;
        }

        // Controls Phase A: run `__ppu_pokes()` frame-wide, once,
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
                err.file = Some(POKES.to_string());
                return Err(err);
            }
            let mut l = self.lua.borrow_mut();
            if self.picture_memory {
                if let Err(message) = l.enter(|ctx| read_memory(ctx, &mut self.memory)) {
                    call_controls_hook::<()>(&mut l, "__ppu_controls_restore");
                    return Err(LuaError { message, line: None, file: Some(POKES.to_string()) });
                }
            }
            n_program
        } else {
            0
        };
        // Painted tiles, last of all: over the program's and __ppu_pokes()'s
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
        // __ppu_pokes above, if any): it reuses the exact per-hook closure
        // below (write_state(row)/run/read_state), so a property controls
        // doesn't touch keeps whatever the program's hooks resolved for that
        // row, and a controls hdma hook composes on top last-write-wins, same
        // as any other hook. The hook is `__ppu_controls_replay`, which
        // re-applies the register writes Phase A logged (frozen here), not
        // __ppu_pokes itself: re-executing it 224x/frame made every
        // frame-wide poke cost 224 tracked writes, and a tilemap paint's
        // thousands of `vram[]` pokes (which no row reads) dominated the
        // frame. Skipped when __ppu_pokes wrote no register this frame.
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
                // hand-written __ppu_pokes assigning `__ppu_hooks = {}` can
                // shrink it out from under that count — clamp so the splice
                // never goes out of range.
                hooks.insert(
                    n_program.min(hooks.len()),
                    (0, HEIGHT - 1, replay, Some(POKES.to_string())),
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
        // mirror's back (the program's frame(), timer hooks, __ppu_pokes, the
        // controls bracket): forget it, so the first per-row write_state is
        // a full write. Inside the loop only hooks write, and each is
        // followed by the read_state that refreshes the mirror.
        *self.mirror.borrow_mut() = Mirror::default();
        // Resolve the line table: each hook becomes a closure that re-baselines
        // globals to the working row, runs fn(y), and reads the row back.
        let err_sink: Rc<RefCell<Option<LuaError>>> = Rc::new(RefCell::new(None));
        let mut builder = LineTableBuilder::new(defaults.clone());
        if let Some(rows) = self.dma.raster.borrow_mut().take() {
            builder.hdma(0, HEIGHT - 1, move |y, row| *row = rows[y].clone());
        }
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
                        err.file = err.file.or_else(|| file.clone());
                        *sink.borrow_mut() = Some(err);
                    }
                }
            });
        }

        let lt = builder.build(HEIGHT);

        // Undo every write controls made this frame — including an explicit
        // `__ppu_pokes()` call from inside the program's own frame(),
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

            // 3. Flush KOF then KON. KOFF is a level the chip polls, so a voice keyed on this
            // frame drops out of it: `koff(v); kon(v)` retriggers.
            let kof = ctx.get_global("__dsp_koff").to_int().unwrap_or(0) as u8;
            let kon = ctx.get_global("__dsp_kon").to_int().unwrap_or(0) as u8 | self.mix_kon;
            self.dsp.write(0x5c, (kof | self.mix_koff) & !kon);
            self.mix_koff = 0;
            ctx.set_global("__dsp_koff", 0).unwrap();
            self.dsp.write(0x4c, kon);
            self.mix_kon = 0;
            ctx.set_global("__dsp_kon", 0).unwrap();
        });
    }

    /// After a successful recompile: hand each song the OLD program had
    /// sounding to the new player at the same index playing the same song,
    /// when the new program declared that one `at = false` (and its setup
    /// left it so — a `stop()` in `init()` means stopped): it takes over
    /// the old anchor and resumes at t. That is what keeps a `play()`-started
    /// song (game music) going across edits — including the edit that turns
    /// `at = 0` into `at = false`. Any other old song still sounding keys
    /// off through the new VM's `__dsp_koff` (KOF before KON in the next
    /// flush — the replay of a paused frame included — so a kon from
    /// `init()` still wins) — unless a new player plays the same song at the
    /// same anchor, which continues on those voices (a re-anchored or
    /// `Pending` one restarts, so it keys off like `play()`). If the
    /// staff followed a carried song (`prev_score`), it still does. Not
    /// used by `reset()`, which drops every old player first.
    fn carry_songs(&mut self, prev: &DmaRecorder, prev_score: Option<i64>) {
        let old_songs = prev.songs.borrow();
        let mut songs = self.dma.songs.borrow_mut();
        let mut dropped = Vec::new();
        let mut staff = false;
        for (i, old) in old_songs.iter().enumerate() {
            if old.anchor == Anchor::Stopped || !old.playing {
                continue;
            }
            match songs
                .get_mut(i)
                .filter(|p| p.name == old.name && p.at_false)
            {
                Some(p) => {
                    p.anchor = old.anchor;
                    p.shift_h = old.shift_h;
                    p.playing = true;
                    p.at_false = false;
                    staff |= prev_score == Some(i as i64);
                }
                None => dropped.push(old),
            }
        }
        let mut l = self.lua.borrow_mut();
        l.enter(|ctx| {
            // `__scores` is a plain global user code can clobber: no table,
            // no republish.
            if let (true, Some(i), Value::Table(hs)) =
                (staff, prev_score, ctx.get_global("__scores"))
            {
                ctx.set_global("__score", hs.get(ctx, i)).unwrap();
            }
            for old in dropped {
                let continues = songs
                    .iter()
                    .any(|p| p.name == old.name && p.anchor == old.anchor);
                if !continues {
                    song_koff(ctx, old.mask & !prev.owned());
                }
            }
        });
    }

    /// Re-phase every timer onto the timeline at position `pos_h`
    /// (half-samples from t = 0): each Lua timer's `due_h` becomes the first
    /// `k * period_h >= pos_h` with k >= 1. A song timer is phased from its
    /// own [`Anchor`] instead: the first `anchor + k * period_h >= pos_h`,
    /// k >= 1 (a `Pending` one anchors at `pos_h` first and restarts from
    /// the top; a `Stopped` one never comes due). The single re-phase
    /// point: called at the start of every rendered span (a no-op for
    /// continuous play, where the walker already left every due there —
    /// it is what lets a `play()`/`stop()` from the frame body take effect
    /// this span) and after a recompile.
    fn realign(&mut self, pos_h: u64) {
        let mut songs = self.dma.songs.borrow_mut();
        for h in self.timers.iter_mut() {
            let Hook::Song(id) = h.hook else {
                h.due_h = pos_h.div_ceil(h.period_h).max(1) * h.period_h;
                continue;
            };
            let Some(p) = songs.get_mut(id) else {
                h.due_h = u64::MAX;
                continue;
            };
            if p.anchor == Anchor::Pending {
                p.set_anchor(Anchor::At(pos_h as i64));
                p.playing = true;
                p.rewind();
            }
            h.due_h = match p.anchor {
                Anchor::At(a) => {
                    let ahead = pos_h as i64 - a;
                    // k = ceil(ahead / P) for a signed `ahead` (before the
                    // anchor it is negative): -floor(-x / P).
                    let k = (-(-ahead).div_euclid(SONG_PERIOD_H)).max(1);
                    (a + k * SONG_PERIOD_H) as u64
                }
                _ => u64::MAX,
            };
        }
    }

    /// Render frame `f`'s audio span sample-accurately against the
    /// registered `timer(n, div, fn)` hooks. The span is exactly
    /// `[S(f), S(f+1))` samples of the 32 kHz timeline, where
    /// `S(f) = round(32000 * f / 60)` (see `audio_start_s`): 533/533/534
    /// samples, gap-free, no drift. Every span starts with `realign` (which
    /// re-phases the timers after a jump and picks up a `play()`/`stop()`).
    /// If `f` isn't `last_f + 1` after an earlier frame, nor a replay of
    /// `last_f` right after a recompile (see `recompiled`), that is a jump: the
    /// anchored songs' voices key off and re-seek on their next fire (a
    /// finished `loop = false` song plays again until its first fire past
    /// the end), a live reload's anchor shift is dropped (back to the
    /// declared anchor, before `realign`), and
    /// the echo buffer is zeroed (after the offset-0 flush, so the live
    /// EDL decides the region). Flushes `voice[]`/`dsp`/`aram[]`/KON/KOFF
    /// at offset 0, then walks expiry to expiry: render the DSP up to the
    /// next due hook (across all timers, earliest `due_h` first, ties in
    /// registration order), call it as `fn(off, t)` where `off` is the
    /// sample offset within this span and `t` the fire's absolute timeline
    /// time in seconds, flush again so its writes take effect from that
    /// offset on, and repeat until no hook is due within this span, then
    /// render the tail. Runs once per rendered frame, right after
    /// `defaults`/`read_state` and before the hdma hooks are collected —
    /// timers always run before hdma within the same frame. Republishes
    /// `voice[n].envx/.outx/.ended` for the next `frame()`/hook to read only
    /// once the whole span rendered cleanly; a timer hook error aborts the
    /// walk early and leaves this frame's rendered prefix plus a stale tail
    /// in `self.audio` — see [`Self::audio`].
    /// One frame of sfx() key-offs, before the frame's offset-0 flush: a countdown sfx() armed
    /// this frame starts, a kon()/koff() of its voice cancels it, and one that runs out keys its
    /// voice off (a retrigger in the same frame still wins: the flush writes KOF then KON).
    fn tick_sfx_holds(&mut self) {
        let rec = &self.dma;
        self.lua.borrow_mut().enter(|ctx| {
            let kon = ctx.get_global("__dsp_kon").to_int().unwrap_or(0) as u8;
            let koff = ctx.get_global("__dsp_koff").to_int().unwrap_or(0) as u8;
            let armed = rec.sfx_armed.replace(0);
            let mut hold = rec.sfx_hold.get();
            let mut own = rec.sfx_own.get();
            let mut off = 0u8;
            for (v, (left, owned)) in hold.iter_mut().zip(own.iter_mut()).enumerate() {
                let bit = 1u8 << v;
                if armed & bit != 0 {
                    continue;
                }
                if (kon | koff) & bit != 0 {
                    *left = 0;
                    *owned = 0;
                } else {
                    if *left > 0 {
                        *left -= 1;
                        if *left == 0 {
                            off |= bit;
                        }
                    }
                    if *owned > 0 && *owned < u16::MAX {
                        *owned -= 1;
                    }
                }
            }
            rec.sfx_hold.set(hold);
            rec.sfx_own.set(own);
            rec.sfx_auto.set(rec.sfx_auto.get() & rec.owned());
            rec.kon_open
                .set((rec.kon_open.get() & !koff | kon & !armed) & !off);
            if off != 0 {
                ctx.set_global("__dsp_koff", (koff | off) as i64).unwrap();
            }
        });
    }

    /// Frames until sfx() keys `voice` off (0: it will not), as of the last frame.
    pub fn sfx_hold(&self, voice: usize) -> u16 {
        self.dma.sfx_hold.get()[voice]
    }

    /// Frames `voice` stays an effect's, a song's notes there silent (0: none; `u16::MAX`: until
    /// koff()), as of the last frame.
    pub fn sfx_own(&self, voice: usize) -> u16 {
        self.dma.sfx_own.get()[voice]
    }

    fn render_frame_audio(&mut self, f: u32) -> Result<(), LuaError> {
        let continuous = self.last_f.map(|l| l as u64 + 1) == Some(f as u64);
        let replay = self.recompiled && self.last_f == Some(f);
        let jump = self.last_f.is_some() && !continuous && !replay;
        self.last_f = Some(f);
        self.recompiled = false;
        let start_h = 2 * audio_start_s(f as u64);
        let end_h = 2 * audio_start_s(f as u64 + 1);
        if jump {
            // A live reload's anchor shift ends at the next jump: back to
            // the declared anchor BEFORE `realign` phases the song timers.
            // An anchored player is playing again: one that had finished
            // (`loop = false`) re-finishes on its first fire past the end,
            // and starts over if the jump landed inside or before the song.
            for p in self.dma.songs.borrow_mut().iter_mut() {
                if let Anchor::At(a) = p.anchor {
                    p.set_anchor(Anchor::At(a - p.shift_h));
                    p.playing = true;
                }
            }
        }
        self.realign(start_h);
        if jump {
            // Before the flush, so a kon pending from the frame body (also
            // in `__dsp_kon`) still wins: the flush writes KOF then KON.
            let dma = self.dma.clone();
            let mut l = self.lua.borrow_mut();
            l.enter(|ctx| {
                for p in dma.songs.borrow_mut().iter_mut() {
                    if p.anchor != Anchor::Stopped {
                        song_koff(ctx, p.mask & !dma.owned());
                        p.next = -1;
                    }
                }
            });
        }
        self.tick_sfx_holds();
        if self
            .lua
            .borrow_mut()
            .enter(|ctx| match ctx.get_global("dsp") {
                Value::Table(d) => d.get(ctx, "raw").to_bool(),
                _ => false,
            })
        {
            return self.render_raw_frame_audio(f);
        }
        self.flush_dsp_writes();
        if jump {
            let (lo, hi) = echo_region((self.dsp.read(0x7d) & 0x0f) as i64);
            self.aram[lo as usize..hi as usize].fill(0);
        }

        let n = ((end_h - start_h) / 2) as usize;
        self.audio.resize(n * 2, 0);

        let mut cursor = 0usize;
        let mut err: Option<LuaError> = None;

        loop {
            let pick = self
                .timers
                .iter()
                .enumerate()
                .filter(|(_, h)| h.due_h < end_h)
                .min_by_key(|(i, h)| (h.due_h, *i))
                .map(|(i, _)| i);
            let Some(i) = pick else { break };

            let due_h = self.timers[i].due_h;
            let off = ((due_h - start_h) / 2) as usize;
            if off > cursor {
                self.dsp
                    .render(&mut self.aram, &mut self.audio[2 * cursor..2 * off]);
            }

            match self.timers[i].hook.clone() {
                Hook::Lua(func) => {
                    let res = {
                        let mut l = self.lua.borrow_mut();
                        let ex = l.enter(|ctx| {
                            let f = ctx.fetch(&func);
                            ctx.stash(Executor::start(
                                ctx,
                                f,
                                (off as i64, due_h as f64 / 64000.0),
                            ))
                        });
                        l.execute::<()>(&ex)
                    };
                    if let Err(e) = res {
                        let mut e = static_error_to_lua(e);
                        e.file = self.timers[i].file.clone();
                        err = Some(e);
                    }
                }
                // A song hook can't error — see `run_song_tick`.
                Hook::Song(id) => self.run_song_tick(id, due_h),
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
        // so next frame's due_h is exactly where it would be had the hooks
        // run.
        if err.is_some() {
            for h in self.timers.iter_mut() {
                while h.due_h < end_h {
                    h.due_h += h.period_h;
                }
            }
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

    /// Raw S-DSP drivers can submit chip writes at exact sample offsets.
    /// `dsp.raw` selects this path; `dsp.events` is consumed once per frame.
    fn render_raw_frame_audio(&mut self, f: u32) -> Result<(), LuaError> {
        enum Event {
            Write(u8, u8),
            Upload(usize, Vec<u8>),
            Reset,
        }
        let n = (audio_start_s(f as u64 + 1) - audio_start_s(f as u64)) as usize;
        let events =
            self.lua
                .borrow_mut()
                .enter(|ctx| -> Result<Vec<(usize, Event)>, LuaError> {
                    let fail = |message: &str| LuaError {
                        message: message.into(),
                        line: None,
                        file: None,
                    };
                    let Value::Table(d) = ctx.get_global("dsp") else {
                        unreachable!()
                    };
                    let Value::Table(list) = d.get(ctx, "events") else {
                        return Ok(vec![]);
                    };
                    let mut events = Vec::new();
                    let mut last = 0;
                    for i in 1..=list.length() {
                        let Value::Table(e) = list.get(ctx, i) else {
                            return Err(fail("dsp.events entries must be tables"));
                        };
                        let at = e.get(ctx, "at").to_int().unwrap_or(0);
                        if at < last || at < 0 || at > n as i64 {
                            return Err(fail(
                                "dsp.events offsets must increase within the audio frame",
                            ));
                        }
                        last = at;
                        let event = if e.get(ctx, "reset").to_bool() {
                            Event::Reset
                        } else if let Some(addr) = e.get(ctx, "addr").to_int() {
                            if !(0..=65535).contains(&addr) {
                                return Err(fail("dsp.events ARAM address is out of range"));
                            }
                            let bytes = match e.get(ctx, "data") {
                                Value::String(b) => b.as_bytes().to_vec(),
                                Value::Table(b) => {
                                    let mut bytes = Vec::new();
                                    for j in 1..=b.length() {
                                        let v = b.get(ctx, j).to_int().ok_or_else(|| {
                                            fail("dsp.events upload requires bytes")
                                        })?;
                                        if !(0..=255).contains(&v) {
                                            return Err(fail(
                                                "dsp.events upload byte is out of range",
                                            ));
                                        }
                                        bytes.push(v as u8);
                                    }
                                    bytes
                                }
                                _ => {
                                    return Err(fail(
                                        "dsp.events upload requires a string or byte table",
                                    ))
                                }
                            };
                            if bytes.len() > 65536 {
                                return Err(fail("dsp.events upload exceeds ARAM"));
                            }
                            Event::Upload(addr as usize, bytes)
                        } else {
                            let reg = e
                                .get(ctx, "reg")
                                .to_int()
                                .ok_or_else(|| fail("dsp.events write requires reg"))?;
                            let val = e
                                .get(ctx, "value")
                                .to_int()
                                .ok_or_else(|| fail("dsp.events write requires value"))?;
                            if !(0..=127).contains(&reg) || !(0..=255).contains(&val) {
                                return Err(fail("dsp.events register or value is out of range"));
                            }
                            Event::Write(reg as u8, val as u8)
                        };
                        events.push((at as usize, event));
                    }
                    d.set(ctx, "events", Table::new(&ctx)).unwrap();
                    Ok(events)
                })?;
        self.audio.resize(n * 2, 0);
        let mut cursor = 0;
        for (at, event) in events {
            self.dsp
                .render(&mut self.aram, &mut self.audio[cursor * 2..at * 2]);
            match event {
                Event::Write(reg, val) => self.dsp.write(reg, val),
                Event::Reset => {
                    let (clock, log) = (self.dsp.clock, self.dsp.log.take());
                    self.dsp = Dsp::new();
                    (self.dsp.clock, self.dsp.log) = (clock, log);
                }
                Event::Upload(addr, bytes) => {
                    for (i, b) in bytes.into_iter().enumerate() {
                        self.aram[(addr + i) & 65535] = b
                    }
                }
            }
            cursor = at;
        }
        self.live_mix
            .as_ref()
            .unwrap_or(&self.saved_mix)
            .apply(&mut self.dsp);
        self.dsp
            .render(&mut self.aram, &mut self.audio[cursor * 2..]);
        self.unmixed_dsp = Some(decode_dsp_view(&self.dsp));
        self.lua.borrow_mut().enter(|ctx| {
            seed_dsp_tables(ctx, &self.dsp, None);
            if let Value::Table(d) = ctx.get_global("dsp") {
                d.set(ctx, "raw", true).unwrap();
            }
        });
        Ok(())
    }

    /// Plays the fire at `due_h` of the native song player `id` (in
    /// `self.dma.songs`) — exactly what kit.lua's own `score{}`
    /// `timer(0, 32, ...)` hook does for a Lua-sourced score, but reading/
    /// writing `voice[]` and the `__dsp_kon`/`__dsp_koff` globals directly
    /// instead of calling `sfx`/`kon`/`koff`. The fire's tick comes from
    /// time: fire `n = (due_h - anchor) / period - 1` plays tick `n`
    /// (wrapped when looping; past the end a `loop = false` song is
    /// finished and silent). A fire that doesn't follow the previous one
    /// (a jump, a recompile) re-seeks the cursor to that tick, so play
    /// resumes from the next note due. A song's shape was already validated
    /// at `__song_load` time, so this never errors and never touches a voice
    /// outside the song's mask.
    fn run_song_tick(&mut self, id: usize, due_h: u64) {
        let dma = self.dma.clone();
        let mut l = self.lua.borrow_mut();
        l.enter(|ctx| {
            let mut songs = dma.songs.borrow_mut();
            let Some(player) = songs.get_mut(id) else {
                return;
            };
            // `stop()`/`play()` from a hook mid-span: not due until realign.
            let Anchor::At(a) = player.anchor else {
                return;
            };
            if player.looping && player.timing.body() <= 0 {
                return;
            }
            let n = (due_h as i64 - a) / SONG_PERIOD_H - 1;
            let continuous = player.next == n;
            let tick = if player.looping {
                player.timing.wrap(n)
            } else {
                n
            };
            if !player.looping && n >= player.timing.length {
                if continuous && player.playing {
                    song_koff(ctx, player.mask & !dma.owned());
                    player.ends = [None; 8];
                }
                player.playing = false;
                player.tick = 0;
                player.next = n + 1;
                return;
            }
            player.playing = true;
            if continuous
                && player.looping
                && n >= player.timing.length
                && tick == player.timing.loop_tick
            {
                song_koff(ctx, player.mask & !dma.owned());
                player.ends = [None; 8];
                // Not `seek(loop_tick)`: that would put intro notes still
                // sounding back into `ends` right after the key-off.
                let lt = player.timing.loop_tick;
                player.cursor = player.events.partition_point(|e| e.start < lt);
            } else if !continuous {
                player.seek(tick);
            }
            // An effect borrows its voice: the song's notes there are silent until it ends.
            let owned = dma.owned();
            for v in 0..8u8 {
                if player.mask & (1 << v) == 0 {
                    continue;
                }
                if let Some(end) = player.ends[v as usize] {
                    if end <= tick {
                        if owned & 1 << v == 0 {
                            song_key(ctx, "__dsp_koff", v);
                        }
                        player.ends[v as usize] = None;
                    }
                }
            }
            while player.cursor < player.events.len() && player.events[player.cursor].start <= tick
            {
                let ev = player.events[player.cursor].clone();
                player.cursor += 1;
                if owned & 1 << ev.voice != 0 {
                    continue;
                }
                if let Some(inst) = player.insts.get(ev.row as usize) {
                    let inst = ctx.fetch(inst);
                    let pitch = player
                        .pitches
                        .get(ev.row as usize)
                        .copied()
                        .unwrap_or(0x1000);
                    apply_song_event(ctx, inst, pitch, &ev);
                }
                player.ends[ev.voice as usize] = Some(ev.end);
            }
            player.tick = tick + 1;
            player.next = n + 1;
        });
    }
}

/// OR `1 << v` into the hidden KON/KOFF accumulator global — the same thing
/// the `kon`/`koff` Lua callbacks do (see `install_bindings`), but called
/// directly from a native song hook with no Lua function to invoke.
fn song_key(ctx: piccolo::Context<'_>, global: &'static str, v: u8) {
    let cur = ctx.get_global(global).to_int().unwrap_or(0);
    ctx.set_global(global, cur | (1i64 << v)).unwrap();
}

/// The song id of the `__score` handle (the most recently started
/// `score{}`). A song-sourced handle carries a raw `__song` id instead of
/// live `tick`/`length`/`playing` table fields (those only resolve through
/// its `__index` metamethod, which `Table::get` — a raw get — doesn't
/// invoke), so the player is read directly. A table with no `__song` (stale
/// global from some other value) has none.
fn score_id(ctx: piccolo::Context<'_>) -> Option<i64> {
    let Value::Table(h) = ctx.get_global("__score") else {
        return None;
    };
    h.get(ctx, "__song").to_int()
}

/// Key off every voice in a song's `mask` (see [`song_key`]).
fn song_koff(ctx: piccolo::Context<'_>, mask: u8) {
    for v in (0..8u8).filter(|v| mask & (1 << v) != 0) {
        song_key(ctx, "__dsp_koff", v);
    }
}

/// Apply one song event's row instrument to `voice[ev.voice]`, exactly the
/// fields kit.lua's `sfx(inst, v)` applies (sample/adsr/gain/noise/pmod/
/// echo), then the row's precomputed pitch and the event's precomputed l/r
/// (PSNG volume, NOT `sfx`'s own pan/vol formula — the PSNG compiler already
/// folded vel/vol/pan into `ev.l`/`ev.r`), and keys the voice on.
fn apply_song_event<'gc>(
    ctx: piccolo::Context<'gc>,
    inst: Table<'gc>,
    pitch: i64,
    ev: &crate::song::Event,
) {
    let Value::Table(voices) = ctx.get_global("voice") else {
        return;
    };
    let Value::Table(vo) = voices.get(ctx, ev.voice as i64) else {
        return;
    };
    vo.set(ctx, "sample", inst.get(ctx, "sample")).unwrap();
    let adsr = inst.get(ctx, "adsr");
    if let Value::Table(a) = adsr {
        let new_adsr = Table::new(&ctx);
        for k in ["a", "d", "s", "r"] {
            new_adsr.set(ctx, k, a.get(ctx, k)).unwrap();
        }
        vo.set(ctx, "adsr", new_adsr).unwrap();
    }
    let gain = inst.get(ctx, "gain");
    if !matches!(adsr, Value::Nil) || !matches!(gain, Value::Nil) {
        vo.set(ctx, "gain", gain).unwrap();
    }
    for k in ["noise", "pmod", "echo"] {
        let v = inst.get(ctx, k);
        if !matches!(v, Value::Nil) {
            vo.set(ctx, k, v).unwrap();
        }
    }
    vo.set(ctx, "pitch", pitch).unwrap();
    let vol = Table::new(&ctx);
    vol.set(ctx, "l", ev.l as i64).unwrap();
    vol.set(ctx, "r", ev.r as i64).unwrap();
    vo.set(ctx, "vol", vol).unwrap();
    song_key(ctx, "__dsp_kon", ev.voice);
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

/// `@ppu/world`'s hot swap hooks (if the program loaded it): their string result, empty when
/// the hook is absent or raises (they only set flags and report, so a failure leaves the worlds as found).
fn call_world_hook(lua: &mut Lua, name: &'static str, arg: &str) -> String {
    let ex = lua.enter(|ctx| match ctx.get_global(name) {
        Value::Function(f) => Some(ctx.stash(Executor::start(ctx, f, ctx.intern(arg.as_bytes())))),
        _ => None,
    });
    ex.and_then(|ex| lua.execute::<String>(&ex).ok())
        .unwrap_or_default()
}

/// The pokes chunk's top-level `dma(` lines, in order: what an in-place load cannot apply.
fn dma_lines(src: &str) -> Vec<&str> {
    src.lines()
        .map(str::trim_start)
        .filter(|l| l.starts_with("dma("))
        .collect()
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
    let json = crate::save::canonical_json(&lua_to_json(root, "sram", 0)?);
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

/// Publish a controller mask as a global (`pad`, `pad2`): one boolean per button, and the same buttons in
/// `pressed` (down now, up in `was`, the previous frame's mask) and `released` (the reverse).
fn set_pad_table(ctx: piccolo::Context<'_>, global: &'static str, mask: u16, was: u16) {
    let buttons = |m: u16| {
        let t = Table::new(&ctx);
        for (bit, name) in PAD_NAMES.iter().enumerate() {
            t.set(ctx, *name, m & (1 << bit) != 0).unwrap();
        }
        t
    };
    let pad = buttons(mask);
    pad.set(ctx, "pressed", buttons(mask & !was)).unwrap();
    pad.set(ctx, "released", buttons(was & !mask)).unwrap();
    ctx.set_global(global, pad).unwrap();
}

/// `parent[key]` as a table, created when absent and emptied when `empty`: power-on reuses the
/// register tables in place, so a module that kept `local layer = bg[1]` still sees the live one.
fn reg_table<'gc>(
    ctx: piccolo::Context<'gc>,
    parent: Table<'gc>,
    key: impl piccolo::IntoValue<'gc>,
    empty: bool,
) -> Table<'gc> {
    let key = key.into_value(ctx);
    let t = match parent.get(ctx, key) {
        Value::Table(t) => t,
        _ => {
            let t = Table::new(&ctx);
            parent.set(ctx, key, t).unwrap();
            t
        }
    };
    if empty {
        let keys: Vec<Value> = t.iter().map(|(k, _)| k).collect();
        for k in keys {
            t.set(ctx, k, Value::Nil).unwrap();
        }
    }
    t
}

/// `bg[n].map:fill(x, y, w, h, entry)`: the cell table `entry` (copied per cell) over a w x h rectangle of cells,
/// refused when any part of it falls outside the layer's map (32 or 64 cells each way, from screen_size).
fn map_fill<'gc>(ctx: piccolo::Context<'gc>, layer: Table<'gc>) -> Callback<'gc> {
    Callback::from_fn_with(&ctx, layer, |layer, ctx, _, mut stack| {
        let usage = "bg[n].map:fill(x, y, w, h, {tile = .., pal = .., prio = .., flip_x = .., flip_y = ..})";
        let Value::Table(map) = stack.get(0) else {
            return Err(lua_err(ctx, &format!("error[snes.map]: call fill with a colon: {usage}")));
        };
        let mut n = [0i64; 4];
        for (i, name) in ["x", "y", "w", "h"].iter().enumerate() {
            let whole = match stack.get(i + 1) {
                Value::Integer(v) => Some(v),
                Value::Number(v) if v.fract() == 0.0 && v.abs() < 1e15 => Some(v as i64),
                _ => None,
            };
            n[i] = whole.filter(|v| *v >= 0).ok_or_else(|| {
                lua_err(ctx, &format!("error[snes.map]: bg[n].map:fill: {name} must be a whole number 0 or more: {usage}"))
            })?;
        }
        let [x, y, w, h] = n;
        let Value::Table(entry) = stack.get(5) else {
            return Err(lua_err(ctx, &format!("error[snes.map]: bg[n].map:fill: the entry must be a table: {usage}")));
        };
        const FIELDS: [&str; 5] = ["tile", "pal", "prio", "flip_x", "flip_y"];
        for (k, _) in entry {
            let known = matches!(k, Value::String(s) if FIELDS.iter().any(|f| s.as_bytes() == f.as_bytes()));
            if !known {
                return Err(lua_err(ctx, &format!("error[snes.map]: bg[n].map:fill: a map entry has no field {k}: its fields are tile, pal, prio, flip_x, flip_y")));
            }
        }
        let size = crate::quantize::bg_screen_size(layer.get(ctx, "screen_size").to_int().unwrap_or(0) as u8);
        let (cols, rows) = (if size & 1 == 1 { 64 } else { 32 }, if size & 2 == 2 { 64 } else { 32 });
        if x + w > cols || y + h > rows {
            return Err(lua_err(ctx, &format!("error[snes.map]: bg[n].map:fill({x}, {y}, {w}, {h}) reaches past the layer's map of {cols}x{rows} cells (columns 0..{}, rows 0..{})", cols - 1, rows - 1)));
        }
        for col in x..x + w {
            let column = match map.get(ctx, col) {
                Value::Table(t) => t,
                _ => {
                    let t = Table::new(&ctx);
                    map.set(ctx, col, t)?;
                    t
                }
            };
            for row in y..y + h {
                // A copy per cell: changing `entry` later must not change the cells, as on the cart.
                let cell = Table::new(&ctx);
                for f in FIELDS {
                    cell.set(ctx, f, entry.get(ctx, f))?;
                }
                column.set(ctx, row, cell)?;
            }
        }
        stack.clear();
        Ok(CallbackReturn::Return)
    })
}

/// The PPU's power-on state: every register global, OAM, and the VRAM/CGRAM/tilemap poke tables
/// empty. Run once per VM by `install_bindings`, and again by a scene's cold boot.
fn power_on<'gc>(ctx: piccolo::Context<'gc>, k: &Keys<'gc>) {
    // Throwaway: an empty mirror makes every sync_* write unconditional; the engine's own mirror starts empty too.
    let m = &mut Mirror::default();
    let g = ctx.globals();
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
    let bg = reg_table(ctx, g, "bg", false);
    for i in 1..=4i64 {
        let layer = reg_table(ctx, bg, i, false);
        let scroll = reg_table(ctx, layer, "scroll", false);
        scroll.set(ctx, "x", 0.0).unwrap();
        scroll.set(ctx, "y", 0.0).unwrap();
        // bg[n].columns[c] = dy: Mode 2 column offsets, retained; a cold boot clears them.
        reg_table(ctx, layer, "columns", true);
        layer.set(ctx, "visible", true).unwrap();
        // Binding registers (BGMODE tile-size / BGnSC / BGnNBA), quantize-on-write.
        layer.set(ctx, "tile_size", 8).unwrap();
        layer.set(ctx, "map_base", 0).unwrap();
        layer.set(ctx, "screen_size", 0).unwrap();
        layer.set(ctx, "char_base", 0).unwrap();
        layer.set(ctx, "mosaic", false).unwrap();
        // Per-cell tilemap poke surface: map[col][row] = {tile,pal,prio,flip_x,flip_y}.
        let map = reg_table(ctx, layer, "map", true);
        map.set(ctx, "fill", map_fill(ctx, layer)).unwrap();
    }

    // m7
    let m7 = reg_table(ctx, g, "m7", false);
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
    reg_table(ctx, m7, "map", true);

    // Hidden Mode 7 char buffer, keyed `__m7char[tile][fy*8+fx] = index`, filled
    // by `m7pixel` and flushed into the high byte lane at frame time.
    reg_table(ctx, g, "__m7char", true);

    // cgram (scalar table, written by user)
    reg_table(ctx, g, "cgram", true);

    // vram (raw 16-bit word poke table: vram[addr] = word, 0..0x7FFF)
    reg_table(ctx, g, "vram", true);

    // obj[0..127]; OBSEL (`size_sel`, `char_base`, `name_select`) unset reads as zero.
    let obj = reg_table(ctx, g, "obj", false);
    for i in 0..128i64 {
        let o = reg_table(ctx, obj, i, false);
        for (k, v) in [("x", 0.0), ("y", 0.0)] {
            o.set(ctx, k, v).unwrap();
        }
        for k in ["tile", "pal", "prio"] {
            o.set(ctx, k, 0).unwrap();
        }
        for k in ["flip_x", "flip_y", "on", "large"] {
            o.set(ctx, k, false).unwrap();
        }
    }
    for k in ["size_sel", "char_base", "name_select"] {
        obj.set(ctx, k, Value::Nil).unwrap();
    }
    obj.set(ctx, "priority_rotate", false).unwrap();
    obj.set(ctx, "oam_addr", 0).unwrap();
    // Friendly color-math namespace over CGWSEL/CGADSUB/COLDATA.
    // `__color_base` records the packed bytes at each (re-)baseline so
    // read_state can tell WHICH side (friendly field vs raw mnemonic) the
    // user moved — see the fold in read_state.
    let color = reg_table(ctx, g, "color", false);
    reg_table(ctx, color, "on", false);
    reg_table(ctx, g, "__color_base", true);
    sync_color(ctx, k, m, 0, 0, 0); // power-on defaults = decode of zeroed registers

    // Friendly screen-designation namespace over TM/TS ($212C/$212D). Same
    // baseline change-detection pattern as `color` above; `__screen_base`
    // records the bytes at each (re-)baseline.
    let screen = reg_table(ctx, g, "screen", false);
    reg_table(ctx, screen, "main", false);
    reg_table(ctx, screen, "sub", false);
    reg_table(ctx, g, "__screen_base", true);
    sync_screen(ctx, k, m, 0x00, 0x00); // decode of the power-on TM/TS (both empty)

    // Friendly window namespace over WH0-3, W12SEL/W34SEL, WOBJSEL, WBGLOG/
    // WOBJLOG, TMW/TSW. Same baseline change-detection pattern as `color`/
    // `screen` above; `__win_base` records the eleven bytes at each
    // (re-)baseline. Overlap with `color`: win.color.* selects
    // WHICH pixels form the color window (WOBJSEL high nibble + a WOBJLOG
    // slot); WHERE color math is prevented (CGWSEL bits 4-5) stays owned by
    // color.region. `win` never writes CGWSEL, and the color window has no
    // TMW/TSW bit.
    let win = reg_table(ctx, g, "win", false);
    reg_table(ctx, win, "w1", false);
    reg_table(ctx, win, "w2", false);
    for l in &WIN_LAYERS {
        reg_table(ctx, win, k.win_names()[l.name], false);
    }
    reg_table(ctx, g, "__win_base", true);
    sync_win(ctx, k, m, &[0u8; 11]); // power-on: every window register is zero
}

fn install_bindings<'gc>(ctx: piccolo::Context<'gc>, k: &Keys<'gc>) {
    crate::lua_buffer::install(ctx);
    // controller: all released until the host sets a mask; init() may read it.
    set_pad_table(ctx, "pad", 0, 0);
    set_pad_table(ctx, "pad2", 0, 0);
    // battery-backed save data; set_sources overwrites it from the host blob.
    ctx.set_global("sram", Table::new(&ctx)).unwrap();
    power_on(ctx, k);

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

    // hidden hook registry
    ctx.set_global("__ppu_hooks", Table::new(&ctx)).unwrap();
    // The update a frame belongs to (`@ppu/sound` resets its per-update cue priority on a new one).
    ctx.set_global("__ppu_update", 0).unwrap();

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
    // returned fields (so the pokes chunk's writes stay undo-tracked).
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
                _ => Err(lua_err(ctx, &format!("error[snes.mode7]: {name} must be a finite number"))),
            },
            _ => Err(lua_err(ctx, &format!("error[snes.mode7]: {name} must be a finite number"))),
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
            return Err(lua_err(ctx, "error[snes.mode7]: zoom must be at least 1/127"));
        }
        let angle = finite(ctx, stack.get(0), "angle")?;
        let x = finite(ctx, stack.get(2), "x")?;
        let z = finite(ctx, stack.get(3), "z")?;
        let (c, s, cx, cy) = pose(angle, x, z);
        let t = view(
            ctx,
            [c / zoom, -s / zoom, s / zoom, c / zoom],
            cx,
            cy,
            // Screen row 112 is line 113: py = 112 + 1 + scroll_y - cy = 0.
            cy - 113,
            true,
        );
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
            return Err(lua_err(ctx, "error[snes.mode7]: height must be positive"));
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
        let t = view(
            ctx,
            [c * scale, 0.0, s * scale, 0.0],
            cx,
            cy,
            0,
            y > horizon,
        );
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
                    _ => return Err(lua_err(ctx, &format!("error[snes.bank]: {name}: voice numbers must be 0..7"))),
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
            sustains: None,
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
/// The scene module loading (or entering) right now, which owns what `dma()`/`timer()`/`score{}`
/// record: luau.lua's `require` sets it. `None` outside every scene module.
fn scene_owner(ctx: piccolo::Context<'_>) -> Option<String> {
    match ctx.get_global("__ppu_scene_file") {
        Value::String(s) => Some(String::from_utf8_lossy(s.as_bytes()).into_owned()),
        _ => None,
    }
}

/// How long sfx() holds a looping sample given no `len`, in seconds.
const SFX_LEN: f64 = 0.15;

fn lua_err<'gc>(ctx: piccolo::Context<'gc>, msg: &str) -> piccolo::Error<'gc> {
    Value::String(ctx.intern(msg.as_bytes())).into()
}

/// One `world.load` map as the preview keeps it (lib/world.luau holds the camera).
struct World {
    name: String,
    /// The scene module whose load made it; `None` is the whole program's.
    owner: Option<String>,
    map: crate::tiled::Map,
    /// Per tile layer: its cells as BG words, empty cells resolved (`tiled::sheet_words`); a
    /// Mode 7 layer's tile numbers (`tiled::m7_plane`): the whole 128x128 plane, or a streamed
    /// map's own width x height; `None` for a layer with no tiles.
    words: Vec<Option<Vec<u16>>>,
    /// Per tile layer: it draws from an EXTBG `m7` source.
    extbg: Vec<bool>,
    /// Per tile layer: its sheet's palette count (0 without a sheet).
    palettes: Vec<usize>,
    /// The layer bound to each of bg[1..4].
    bound: [Option<WorldBinding>; 4],
    /// The Mode 7 layer bound to the plane (`w:bind(name, m7)`).
    plane: Option<usize>,
    /// The `radius` a map past 128x128 streams through the plane with.
    radius: Option<i64>,
    /// A generated world's period in cells (`world.load`'s `period`): its map is a sheet of
    /// 16x16-cell templates, and its Mode 7 layer's `words` keep the map's own width x height.
    period: Option<i64>,
    /// A generated world's template size in cells, (width, height): 16x16 on the Mode 7 plane.
    chunk: (i64, i64),
    /// A generated world's chunk grid: per slot (chunk y % 16 * 16 + chunk x % 16), the map cell
    /// where the template that chunk shows starts (`__ppu_world_chunk`).
    chunks: Vec<Option<(i64, i64)>>,
    /// Per tile layer: its cells as `w:set` changed them, words (as `words`) and kinds; `None` while the
    /// layer is as authored. `w:bind` (the layer) and `w:enter` (the map entered) put it back.
    live: Vec<Option<(Vec<u16>, Vec<u8>)>>,
}

impl World {
    /// Tile layer `li`'s words (sheet layers on backgrounds; see `words`), as `w:set` left them.
    fn words_of(&self, li: usize) -> Option<&Vec<u16>> {
        match &self.live[li] {
            Some((words, _)) => Some(words),
            None => self.words[li].as_ref(),
        }
    }
    /// Tile layer `li`'s kinds, row-major, as `w:set` left them.
    fn kinds_of(&self, li: usize) -> &[u8] {
        match &self.live[li] {
            Some((_, kinds)) => kinds,
            None => &self.map.layers[li].kinds,
        }
    }
}

#[derive(Clone, Copy)]
struct WorldBinding {
    layer: usize,
    /// The VRAM map: one 32x32 screen, or two side by side when `wide` (a map wider than 32 cells).
    map: u16,
    wide: bool,
    /// The palette of the sheet's first palette, added to every word.
    pal: u16,
    /// A wrapping bind's repeat (`tiled::painted`), columns and rows: cell (x, y) draws the layer's
    /// (x % columns, y % rows).
    wrap: Option<(i64, i64)>,
    /// The priority bit added to every word: 0, or 0x2000 for a `{priority = 1}` bind.
    priority: u16,
}

/// What sheet layer `li` of `world` draws with when bound in scene `owner` (`None`: the whole
/// program): the char base of its sheet's latest placement there (the scene's own or the
/// program's), and the background palette its first palette lands on. Refused when the sheet is
/// not placed or its palettes are not whole background palettes.
fn sheet_layer(
    placements: &[DmaPlacement],
    world: &World,
    li: usize,
    owner: &Option<String>,
) -> Result<(u16, u16), String> {
    let l = &world.map.layers[li];
    let (Some(source), Some(words)) = (&l.source, &world.words[li]) else {
        return Err(format!("layer {} has no sheet tiles", l.name));
    };
    let placement = placements
        .iter()
        .rev()
        .find(|p| p.name == *source && (p.owner.is_none() || p.owner == *owner));
    let Some(p) = placement else {
        return Err(format!("layer {} draws from sheet {source}: place it first, at file level, with dma(\"{source}\", {{char = 0x1000, pal = 0}})", l.name));
    };
    let depth = p
        .fields
        .iter()
        .find(|(k, _)| *k == "bit_depth")
        .map_or(4, |(_, v)| *v);
    // shortcut: palette bands as in modes 1-2 (CGRAM entry / colors per palette); mode 0's per-BG bands are not offset.
    let stride = crate::source::row_size_of(depth as u8) as i64;
    let base = p.cgram_base as i64;
    let top = words.iter().map(|w| (w >> 10) & 7).max().unwrap_or(0) as i64;
    if base % stride != 0 || base / stride + top > 7 {
        return Err(format!("sheet {source} is placed at CGRAM {base}, so its palettes are not whole background palettes 0..7: place it at a multiple of {stride} with room for {} palettes", top + 1));
    }
    Ok((p.char_base, (base / stride) as u16))
}

/// Refused unless bg[`bgi`]'s VRAM map at `map` (0x800 words when `wide`, else 0x400) is aligned,
/// ends by 0x8000 and is clear of the placements scene `owner` sees.
fn map_room(
    rec: &DmaRecorder,
    map: i64,
    wide: bool,
    bgi: usize,
    owner: &Option<String>,
) -> Result<(), String> {
    let end = map + if wide { 0x800 } else { 0x400 };
    if map & 0x3ff != 0 || map < 0 || end > 0x8000 {
        return Err(format!(
            "bg[{bgi}].map_base must be a multiple of 0x400 with room for the {} map before 0x8000",
            if wide { "64x32" } else { "32x32" }
        ));
    }
    let shared =
        |o: &Option<String>| !rec.overlaps.get() && (owner.is_none() || o.is_none() || o == owner);
    // shortcut: checked one way only; a dma() in init() after the bind may land on the map unnoticed. Record the map as a VRAM range if that bites.
    if let Some((other, ..)) = rec
        .vram_ranges
        .borrow()
        .iter()
        .find(|(_, a, b, o)| (map as usize) < *b && *a < end as usize && shared(o))
    {
        return Err(format!("the map at {map:#06x}..{end:#06x} overlaps '{other}' in VRAM: set bg[{bgi}].map_base clear of it"));
    }
    Ok(())
}

/// Refused unless Mode 7 layer `li`'s `m7` source is placed with `dma()` in scene `owner` or the
/// program: binding loads only the plane's map; its tiles and colours are the placement's.
fn m7_layer(
    placements: &[DmaPlacement],
    world: &World,
    li: usize,
    owner: &Option<String>,
) -> Result<(), String> {
    let l = &world.map.layers[li];
    let source = l.source.as_deref().unwrap_or("?");
    if placements
        .iter()
        .any(|p| p.name == source && (p.owner.is_none() || p.owner == *owner))
    {
        Ok(())
    } else {
        Err(format!("layer {} draws from m7 source {source}: place its tiles first, at file level, with dma(\"{source}\")", l.name))
    }
}

/// One `world.load` map as the program loaded it ([`LuaEngine::worlds`]).
#[derive(Clone, Debug)]
pub struct WorldView {
    pub name: String,
    /// The scene module whose load made it; `None` is the whole program's.
    pub owner: Option<String>,
    pub map: crate::tiled::Map,
    /// Per tile layer of `map`: what binding it draws, or why it cannot be bound.
    pub layers: Vec<Result<WorldLayerView, String>>,
}

/// A layer as binding it draws: what `w:bind` and the strips it presents use. A Mode 7 layer's
/// `words` are its row-major tile numbers (the 128x128 plane, or a streamed map's own width x
/// height), `char_base` and `palettes` 0.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldLayerView {
    /// Row-major BG map words, empty cells resolved, the sheet's palette band added.
    pub words: Vec<u16>,
    /// The sheet's char base in VRAM (the background's `char_base`).
    pub char_base: u16,
    /// The sheet's palette count.
    pub palettes: usize,
    /// `words` give each tile its own char's palette: false only for a sheet of several palettes
    /// whose chars' palettes neither its payload (v4) nor `setSheetPalettes` carried (all palette 0).
    pub tile_palettes: bool,
    /// A Mode 7 layer from an EXTBG source: binding it turns on `m7.extbg`.
    pub extbg: bool,
    /// The background palette the sheet's first palette lands on, already in `words` (0 for Mode 7).
    pub pal: u16,
    /// The words of one of the sheet's chars (its bit depth x 4; 0 for a Mode 7 layer): where a tile animation's
    /// chars lie in VRAM from `char_base`.
    pub char_words: usize,
}

/// The palette of each char of a sheet: its payload's own (v4), else what `setSheetPalettes` gave
/// (empty: palette 0 everywhere).
fn char_pals<'a>(sheet: &'a crate::source::SheetSource, given: Option<&'a Vec<u8>>) -> &'a [u8] {
    if sheet.pals.is_empty() {
        given.map_or(&[], |p| p.as_slice())
    } else {
        &sheet.pals
    }
}

/// A Tiled property value as Lua: class properties become tables.
fn json_value<'gc>(ctx: piccolo::Context<'gc>, v: &serde_json::Value) -> Value<'gc> {
    use serde_json::Value as J;
    match v {
        J::Null => Value::Nil,
        J::Bool(b) => Value::Boolean(*b),
        J::Number(n) => match n.as_i64() {
            Some(i) => Value::Integer(i),
            None => Value::Number(n.as_f64().unwrap_or(0.0)),
        },
        J::String(s) => Value::String(ctx.intern(s.as_bytes())),
        J::Array(a) => {
            let t = Table::new(&ctx);
            for (i, x) in a.iter().enumerate() {
                t.set(ctx, i as i64 + 1, json_value(ctx, x)).unwrap();
            }
            Value::Table(t)
        }
        J::Object(o) => {
            let t = Table::new(&ctx);
            for (k, x) in o {
                t.set(ctx, ctx.intern(k.as_bytes()), json_value(ctx, x))
                    .unwrap();
            }
            Value::Table(t)
        }
    }
}

/// A pixel coordinate as Lua: whole numbers stay integers.
fn coordinate<'gc>(v: f64) -> Value<'gc> {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        Value::Integer(v as i64)
    } else {
        Value::Number(v)
    }
}

/// The engine half of `@ppu/world` (lib/world.luau): `__ppu_world_load` reads a project's Tiled
/// map, `__ppu_world_bind` ties one of its layers to a background, `__ppu_world_strip` presents
/// one row or column of a bound layer, and `__ppu_world_kind` answers collision. The library
/// never writes a cell itself: `__ppu_world_strip` is the one VRAM primitive, so a cartridge can
/// stream the same strips its own way without changing the library.
fn install_world(
    ctx: piccolo::Context<'_>,
    store: Rc<RefCell<HashMap<String, crate::source::SourcePayload>>>,
    pals: Rc<RefCell<HashMap<String, Vec<u8>>>>,
    project: Rc<RefCell<HashMap<String, String>>>,
    rec: Rc<DmaRecorder>,
) {
    use crate::source::SourcePayload;
    use crate::tiled::{read_map, sheet_words, Binding};
    let world_of =
        |ctx: piccolo::Context<'_>, rec: &DmaRecorder, v: Value<'_>| -> Result<usize, String> {
            let i = v
                .to_int()
                .filter(|&i| i >= 1 && i as usize <= rec.worlds.borrow().len());
            let i = i.ok_or("not a world: call this on what world.load returned")? as usize - 1;
            // A scene's world shows only in its scene, as its placements do.
            if let Some(owner) = &rec.worlds.borrow()[i].owner {
                if scene_owner(ctx).as_ref() != Some(owner) {
                    return Err(format!(
                        "world {} belongs to scene {owner}: load it in the scene that uses it",
                        rec.worlds.borrow()[i].name
                    ));
                }
            }
            Ok(i)
        };

    let load_rec = rec.clone();
    let load = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let rec = &load_rec;
        if !rec.active.get() || !ctx.get_global("__ppu_scenes_open").to_bool() {
            return Err(lua_err(ctx, "error[snes.world]: world.load runs at file level, while the program loads, like dma(): move it out of init() and frame()"));
        }
        let Value::String(name) = stack.get(0) else {
            return Err(lua_err(
                ctx,
                "error[snes.world]: world.load: the first argument is the map's name, \"level1\" for level1.tmj",
            ));
        };
        let name = String::from_utf8_lossy(name.as_bytes()).into_owned();
        // Only a view world takes speed (1..8, default 8, as on the cartridge), the most px a
        // frame its window keeps up with; every other world's camera moves as far as it likes, so speed there is
        // ignored below (toys from before views wrote one).
        let speed_arg = stack.get(1);
        let speed = match speed_arg {
            Value::Nil => None,
            Value::Integer(s) if (1..=8).contains(&s) => Some(s),
            Value::Number(s) if s.fract() == 0.0 && (1.0..=8.0).contains(&s) => Some(s as i64),
            _ => None,
        };
        let bad_speed = !matches!(speed_arg, Value::Nil) && speed.is_none();
        let radius = match stack.get(2) {
            Value::Nil => None,
            Value::Integer(r) => Some(r),
            Value::Number(r) if r.fract() == 0.0 && r.abs() < 1e9 => Some(r as i64),
            v => {
                return Err(lua_err(
                    ctx,
                    &format!(
                        "error[snes.world]: world.load(\"{name}\"): radius must be a whole number of cells (got {v})"
                    ),
                ))
            }
        };
        // view = {ahead = A, behind = B}: cells of the plane ahead of and behind the follow point.
        let view = match stack.get(3) {
            Value::Nil => None,
            Value::Table(t) => {
                if let Some((k, _)) = t.iter().find(|(k, _)| !matches!(k, Value::String(s) if matches!(s.as_bytes(), b"ahead" | b"behind"))) {
                    return Err(lua_err(ctx, &format!("error[snes.world]: world.load(\"{name}\"): view takes ahead and behind only (got {k})")));
                }
                let cells = |key: &'static str| {
                    match t.get(ctx, key) {
                    Value::Integer(n) if (0..=1_000_000).contains(&n) => Ok(n),
                    Value::Number(n) if n.fract() == 0.0 && (0.0..=1e6).contains(&n) => Ok(n as i64),
                    v => Err(lua_err(ctx, &format!(
                        "error[snes.world]: world.load(\"{name}\"): view.{key} must be a whole number of cells, 0 or more (got {})",
                        if matches!(v, Value::Nil) { "none".to_string() } else { v.to_string() }))),
                }
                };
                Some((cells("ahead")?, cells("behind")?))
            }
            v => {
                return Err(lua_err(
                    ctx,
                    &format!(
                    "error[snes.world]: world.load(\"{name}\"): view is a table, {{ahead = A, behind = B}} (got {v})"
                ),
                ))
            }
        };
        if view.is_some() && radius.is_some() {
            return Err(lua_err(
                ctx,
                &format!("error[snes.world]: world.load(\"{name}\"): view sets the window: drop radius"),
            ));
        }
        // period = P: a generated world, P cells round on each axis (world.luau holds its pick).
        let period = match stack.get(4) {
            Value::Nil => None,
            Value::Integer(p) if (256..=4096).contains(&p) && p & (p - 1) == 0 => Some(p),
            Value::Number(p) if p.fract() == 0.0 && (256.0..=4096.0).contains(&p) && (p as i64) & (p as i64 - 1) == 0 => Some(p as i64),
            v => return Err(lua_err(ctx, &format!(
                "error[snes.world]: world.load(\"{name}\"): period must be 256, 512, 1024, 2048 or 4096 cells (got {v}): the ground repeats after that many cells on each axis"))),
        };
        let generated = period.is_some();
        // chunk = {w, h}: a generated world's template size in cells, 16 or 32 each (16x16 when none).
        let chunk = match stack.get(5) {
            Value::Nil => (16, 16),
            Value::Table(t) => {
                let side = |i: i64| match t.get(ctx, i) {
                    Value::Integer(n) if n == 16 || n == 32 => Some(n),
                    Value::Number(n) if n == 16.0 || n == 32.0 => Some(n as i64),
                    _ => None,
                };
                match (side(1), side(2), t.length()) {
                    (Some(w), Some(h), 2) => (w, h),
                    _ => return Err(lua_err(ctx, &format!("error[snes.world]: world.load(\"{name}\"): chunk is a template's width and height in cells, 16 or 32 each: {{32, 16}}"))),
                }
            }
            v => return Err(lua_err(ctx, &format!("error[snes.world]: world.load(\"{name}\"): chunk is a table of a template's width and height in cells, {{32, 16}} (got {v})"))),
        };
        if !generated && !matches!(stack.get(5), Value::Nil) {
            return Err(lua_err(ctx, &format!("error[snes.world]: world.load(\"{name}\"): chunk sizes a generated world's templates: load it with period and pick too")));
        }
        if let Some(p) = period.filter(|p| p / chunk.0 < 16 || p / chunk.1 < 16) {
            return Err(lua_err(ctx, &format!("error[snes.world]: world.load(\"{name}\"): period {p} is under 16 chunks of {}x{} cells on an axis: use a period of at least {}", chunk.0, chunk.1, 16 * chunk.0.max(chunk.1))));
        }
        let files = project.borrow();
        let fail = |m: String| lua_err(ctx, &format!("error[snes.world]: world.load(\"{name}\"): {m}"));
        let text = match files.get(&format!("{name}.tmj")) {
            Some(t) => t,
            None if files.contains_key(&format!("{name}.tmx")) => {
                return Err(fail(format!(
                    "{name}.tmx is an XML map: save it as JSON ({name}.tmj) in Tiled"
                )))
            }
            None => return Err(fail(format!("the project has no file named {name}.tmj"))),
        };
        let sources = store.borrow();
        let pals = pals.borrow();
        let file = format!("{name}.tmj");
        let fail = |m: String| -> piccolo::Error<'_> {
            ProjectFileFailure {
                file: file.clone(),
                message: format!("error[snes.world]: world.load(\"{name}\"): {m}"),
            }
            .into()
        };
        let map = read_map(
            text,
            |path| {
                let base = path.rsplit(['/', '\\']).next().unwrap_or(path);
                files
                    .get(path)
                    .or_else(|| files.get(base))
                    .cloned()
                    .ok_or(format!("the project has no file named {base}"))
            },
            |source| match sources.get(source) {
                Some(SourcePayload::Sheet(sheet)) => Some(Binding::Sheet {
                    source: sheet,
                    pals: char_pals(sheet, pals.get(source)),
                }),
                Some(SourcePayload::M7(m7)) => Some(Binding::M7(m7)),
                _ => None,
            },
        )
        .map_err(fail)?;
        let (cw, ch) = (chunk.0 as usize, chunk.1 as usize);
        if generated
            && (map.width % cw != 0 || map.height % ch != 0 || map.width > 512 || map.height > 512)
        {
            return Err(fail(format!(
                "a generated world's map is its templates, {cw}x{ch} cells each: its width is a multiple of {cw} cells and its height of {ch}, at most 512 (this map is {}x{})",
                map.width, map.height)));
        }
        // A generated world streams whatever its templates' map measures.
        let big = generated || map.width > 128 || map.height > 128;
        let mode7 = map.layers.iter().any(|l| l.mode7 && l.source.is_some());
        // A view world: (ahead, behind, near, catch_up); its window is the whole plane, radius 64.
        // A speed without a view does nothing (the camera moves as far as you like): ignored, as toys
        // from before views wrote one; the cartridge check warns about it.
        if view.is_some() && bad_speed {
            return Err(fail(format!(
                "a view's speed is a whole number of pixels per frame from 1 to 8 (got {speed_arg}): \
                 the most the camera moves a frame while the window keeps up (a faster move redraws it whole)"
            )));
        }
        let speed = if view.is_some() { speed.unwrap_or(8) } else { 8 };
        let view = match view {
            None => None,
            Some(_) if !mode7 => return Err(fail("view streams a Mode 7 map and this map has no Mode 7 layer: drop view".into())),
            Some(_) if !big => return Err(fail("a map up to 128x128 cells loads whole: drop view".into())),
            Some(_) if !generated && (map.width.min(map.height) < 128 || map.width.max(map.height) > 512) => return Err(fail(format!(
                "a map with a view is at least 128 and at most 512 cells on each side (it streams the whole 128x128 plane): this map is {}x{}",
                map.width, map.height))),
            Some((a, b)) if a + b > 120 => return Err(fail(format!(
                "view ahead + behind is {} cells, past 120: the view must fit the 128-cell plane with margin", a + b))),
            Some((a, b)) => {
                let d = ((a - b).abs() + 1) / 2;
                let catch_up = (16 * d + 7 + (16 - speed) - 1) / (16 - speed);
                if catch_up > 60 {
                    return Err(fail(format!(
                        "the far rows could lag more than a second ({catch_up} updates): shorten ahead, lengthen behind, or lower speed")));
                }
                Some((a, b, 64 - d, catch_up))
            }
        };
        // A generated world streams through the Mode 7 plane (a radius or a view, 16x16 templates) or to backgrounds.
        if generated && mode7 && view.is_none() && radius.is_none() {
            return Err(fail("a generated world on the Mode 7 plane streams through it: load it with a radius or a view".into()));
        }
        if mode7 && view.is_none() {
            match radius {
                None if big => return Err(fail(format!(
                    "the map is {}x{} cells, past the 128x128 a Mode 7 plane holds: stream it with world.load(\"{name}\", {{radius = r}})",
                    map.width, map.height))),
                Some(_) if !big => return Err(fail("a map up to 128x128 cells loads whole: drop radius".into())),
                // The window, 2 * radius cells a side, must fit the 128-cell plane; what streaming costs is the frame budget's.
                Some(r) if !(1..=64).contains(&r) => return Err(fail(format!(
                    "radius must be a whole number from 1 to 64 (got {r}): the window, 2 * radius cells a side, must fit the 128x128 plane"))),
                Some(r) if !generated && (map.width > 512 || map.height > 512 || (map.width.min(map.height) as i64) < 2 * r) => return Err(fail(format!(
                    "a streamed map is at most 512x512 cells and at least 2 * radius = {} cells on each side (the window must fit it): this map is {}x{}",
                    2 * r, map.width, map.height))),
                _ => {}
            }
        } else if radius.is_some() {
            return Err(fail(
                "radius streams a Mode 7 map and this map has no Mode 7 layer: drop radius".into(),
            ));
        }
        if generated && mode7 && chunk != (16, 16) {
            return Err(fail("a generated world on the Mode 7 plane has 16x16-cell templates: drop chunk".into()));
        }
        // A view streams the whole plane.
        let radius = if view.is_some() { Some(64) } else { radius };
        let (mut words, mut palettes, mut extbg) = (Vec::new(), Vec::new(), Vec::new());
        for layer in &map.layers {
            extbg.push(false);
            words.push(match (&layer.source, layer.mode7) {
                (Some(source), true) => {
                    let Some(SourcePayload::M7(m7)) = sources.get(source) else { unreachable!("read_map bound it") };
                    palettes.push(0);
                    *extbg.last_mut().unwrap() = m7.options.extbg;
                    let plane = crate::tiled::m7_plane(layer, map.width, map.height, m7).map_err(fail)?;
                    // Templates that fit the plane keep their own width x height, as a streamed map's cells do.
                    let plane: Vec<u8> = if generated && map.width <= 128 && map.height <= 128 {
                        plane.chunks(128).take(map.height).flat_map(|r| r[..map.width].iter().copied()).collect()
                    } else {
                        plane
                    };
                    Some(plane.into_iter().map(u16::from).collect())
                }
                (Some(source), false) => {
                    let Some(SourcePayload::Sheet(sheet)) = sources.get(source) else { unreachable!("read_map bound it") };
                    palettes.push(sheet.palettes.len());
                    if sheet.palettes.len() > 1 && sheet.pals.is_empty() && !pals.contains_key(source) {
                        return Err(fail(format!(
                            "sheet {source} has {} palettes but the engine was not given which palette each tile uses: \
                             pass its cells' palettes with setSheetPalettes",
                            sheet.palettes.len()
                        )));
                    }
                    Some(sheet_words(layer, sheet).map_err(fail)?)
                }
                _ => {
                    palettes.push(0);
                    None
                }
            });
        }
        drop((sources, pals));

        let ret = Table::new(&ctx);
        mark_setup_handle(ctx, rec, ret);
        let kinds = Table::new(&ctx);
        for (i, k) in map.kinds.iter().enumerate() {
            kinds
                .set(ctx, ctx.intern(k.as_bytes()), i as i64 + 1)
                .unwrap();
        }
        let layers = Table::new(&ctx);
        for (i, l) in map.layers.iter().enumerate() {
            layers
                .set(ctx, ctx.intern(l.name.as_bytes()), i as i64 + 1)
                .unwrap();
        }
        let objects = Table::new(&ctx);
        for (i, o) in map
            .object_layers
            .iter()
            .flat_map(|l| &l.objects)
            .enumerate()
        {
            let t = Table::new(&ctx);
            t.set(ctx, "name", ctx.intern(o.name.as_bytes())).unwrap();
            t.set(ctx, "type", ctx.intern(o.kind.as_bytes())).unwrap();
            for (k, v) in [("x", o.x), ("y", o.y), ("w", o.w), ("h", o.h)] {
                t.set(ctx, k, coordinate(v)).unwrap();
            }
            let props = Table::new(&ctx);
            for (k, v) in &o.props {
                props
                    .set(ctx, ctx.intern(k.as_bytes()), json_value(ctx, v))
                    .unwrap();
            }
            t.set(ctx, "props", props).unwrap();
            objects.set(ctx, i as i64 + 1, t).unwrap();
        }
        let mut worlds = rec.worlds.borrow_mut();
        ret.set(ctx, "id", worlds.len() as i64 + 1).unwrap();
        ret.set(ctx, "name", ctx.intern(name.as_bytes())).unwrap();
        if view.is_some() {
            ret.set(ctx, "speed", speed).unwrap();
        }
        if let Some(r) = radius {
            ret.set(ctx, "radius", r).unwrap();
        }
        if let Some((a, b, near, catch_up)) = view {
            for (k, v) in [
                ("ahead", a),
                ("behind", b),
                ("near", near),
                ("catch_up", catch_up),
            ] {
                ret.set(ctx, k, v).unwrap();
            }
            // The lead per heading (256ths of a turn, ahead = (sin, -cos)), in f64 here: in a makeFixed
            // program world.luau's own math would be fixed point, and a cartridge bakes these in f64.
            let (lead_x, lead_y) = (Table::new(&ctx), Table::new(&ctx));
            let d = (4 * (a - b)) as f64;
            for h in 0..256i64 {
                let turn = h as f64 * std::f64::consts::PI / 128.0;
                lead_x
                    .set(ctx, h + 1, (d * turn.sin() + 0.5).floor() as i64)
                    .unwrap();
                lead_y
                    .set(ctx, h + 1, (-d * turn.cos() + 0.5).floor() as i64)
                    .unwrap();
            }
            ret.set(ctx, "lead_x", lead_x).unwrap();
            ret.set(ctx, "lead_y", lead_y).unwrap();
        }
        // A generated world is `period` cells round; its map is `templates` templates, `columns` to a row.
        let (width, height) = period.map_or((map.width as i64, map.height as i64), |p| (p, p));
        if let Some(p) = period {
            ret.set(ctx, "period", p).unwrap();
            ret.set(ctx, "columns", (map.width / cw) as i64).unwrap();
            ret.set(
                ctx,
                "templates",
                (map.width / cw * (map.height / ch)) as i64,
            )
            .unwrap();
            ret.set(ctx, "cw", chunk.0).unwrap();
            ret.set(ctx, "ch", chunk.1).unwrap();
        }
        ret.set(ctx, "width", width).unwrap();
        ret.set(ctx, "height", height).unwrap();
        ret.set(ctx, "kinds", kinds).unwrap();
        ret.set(ctx, "layers", layers).unwrap();
        ret.set(ctx, "objects", objects).unwrap();
        let layer_count = map.layers.len();
        worlds.push(World {
            name,
            owner: scene_owner(ctx),
            map,
            words,
            extbg,
            palettes,
            bound: [None; 4],
            plane: None,
            radius,
            period,
            chunk,
            chunks: vec![None; 256],
            live: vec![None; layer_count],
        });
        stack.replace(ctx, ret);
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("__ppu_world_load", load).unwrap();

    // (world, layer name, bg number, map address, wrap, priority 1) -> char_base, wide
    let bind_rec = rec.clone();
    let bind = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let rec = &bind_rec;
        if !rec.active.get() || ctx.get_global("__ppu_scenes_open").to_bool() {
            return Err(lua_err(ctx, "error[snes.world]: w:bind runs in init(), where a scene sets up its backgrounds: a cold boot undoes registers set at file level"));
        }
        let w =
            world_of(ctx, rec, stack.get(0)).map_err(|m| lua_err(ctx, &format!("error[snes.world]: w:bind: {m}")))?;
        let mut worlds = rec.worlds.borrow_mut();
        let world = &mut worlds[w];
        let fail = |m: String| lua_err(ctx, &format!("error[snes.world]: w:bind (world {}): {m}", world.name));
        let Value::String(layer) = stack.get(1) else {
            return Err(fail("the first argument is a tile layer's name".into()));
        };
        let layer = String::from_utf8_lossy(layer.as_bytes()).into_owned();
        let names: Vec<&str> = world.map.layers.iter().map(|l| l.name.as_str()).collect();
        let Some(li) = names.iter().position(|n| *n == layer) else {
            return Err(fail(format!(
                "the map has no tile layer named {layer}; it has {}",
                names.join(", ")
            )));
        };
        let l = &world.map.layers[li];
        if l.source.is_none() {
            return Err(fail(format!("layer {layer} has no tiles")));
        }
        // Background 0 is the Mode 7 plane (world.luau passes it for `m7`).
        if stack.get(2).to_int() == Some(0) {
            if !l.mode7 {
                return Err(fail(format!("layer {layer} draws from a sheet: bind it to a background, w:bind(\"{layer}\", bg[1])")));
            }
            m7_layer(&rec.placements.borrow(), world, li, &scene_owner(ctx)).map_err(fail)?;
            world.plane = Some(li);
            stack.replace(ctx, (0i64, world.extbg[li]));
            return Ok(CallbackReturn::Return);
        }
        if l.mode7 {
            return Err(fail(format!("layer {layer} draws from a Mode 7 tileset: bind it to the plane, w:bind(\"{layer}\", m7)")));
        }
        let bgi = stack
            .get(2)
            .to_int()
            .filter(|n| (1..=4).contains(n))
            .ok_or_else(|| {
                fail("the second argument is a background, bg[1] to bg[4], or m7".into())
            })? as usize;
        let map = stack.get(3).to_int().unwrap_or(0);
        // Generated ground has no edge: its window always needs the 64x32 map.
        let wide = world.map.width > 32 || world.period.is_some();
        let owner = scene_owner(ctx);
        map_room(rec, map, wide, bgi, &owner).map_err(fail)?;
        // Two backgrounds of one world on one map would each overwrite the other's cells.
        let size = |wide: bool| if wide { 0x800 } else { 0x400 };
        if let Some((other, b)) = world.bound.iter().enumerate().find_map(|(i, b)| {
            b.as_ref()
                .filter(|b| {
                    i + 1 != bgi
                        && map < i64::from(b.map) + size(b.wide)
                        && i64::from(b.map) < map + size(wide)
                })
                .map(|b| (i + 1, b))
        }) {
            return Err(fail(format!(
                "bg[{bgi}]'s map at {map:#06x} overlaps bg[{other}]'s at {:#06x}: give each background of a world its own map, {{map = 0x0400}}",
                b.map
            )));
        }
        let (char_base, pal) =
            sheet_layer(&rec.placements.borrow(), world, li, &owner).map_err(fail)?;
        // A bind starts the layer over as authored: w:set's changes go.
        world.live[li] = None;
        let wrap = if stack.get(4).to_bool() {
            let (w, h) = crate::tiled::painted(&world.map, li)
                .ok_or_else(|| fail(format!("layer {layer} has no tiles to repeat (wrap)")))?;
            Some((w as i64, h as i64))
        } else {
            None
        };
        world.bound[bgi - 1] = Some(WorldBinding {
            layer: li,
            map: map as u16,
            wide,
            pal,
            wrap,
            priority: if stack.get(5).to_bool() { 0x2000 } else { 0 },
        });
        stack.replace(ctx, (char_base as i64, wide));
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("__ppu_world_bind", bind).unwrap();

    // (world, map name, x, y) -> target world, wide, char_base per bg[1..4] (nil where unbound): w:enter's
    // checks and its one change, all checks first. The slot's bindings (bg, layer name, map_base) move onto
    // the first same-owner world loaded as that map; the library swaps its fields and redraws.
    let enter_rec = rec.clone();
    let enter = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let rec = &enter_rec;
        let w =
            world_of(ctx, rec, stack.get(0)).map_err(|m| lua_err(ctx, &format!("error[snes.world]: w:enter: {m}")))?;
        let mut worlds = rec.worlds.borrow_mut();
        let from = &worlds[w];
        let fail = |m: String| lua_err(ctx, &format!("error[snes.world]: w:enter (world {}): {m}", from.name));
        if !ctx.get_global("force_blank").to_bool() && !rec.hot_swap.get() {
            return Err(fail("w:enter redraws the whole screen, so the screen must be off: set force_blank = true before it, and false on a later update (fade how you like)".into()));
        }
        let Value::String(name) = stack.get(1) else {
            return Err(fail(format!(
                "the first argument is a map's name, \"level2\" for level2.tmj (got {})",
                stack.get(1)
            )));
        };
        let name = String::from_utf8_lossy(name.as_bytes()).into_owned();
        if from.radius.is_some() || from.plane.is_some() {
            return Err(fail("w:enter swaps maps on backgrounds; a world on the Mode 7 plane (or loaded with radius or view) cannot enter another map".into()));
        }
        let whose = match &from.owner {
            Some(s) => format!("scene {s}"),
            None => "the program".to_string(),
        };
        let Some(t) = worlds
            .iter()
            .position(|o| o.name == name && o.owner == from.owner)
        else {
            return Err(fail(format!("{whose} loads no map named {name}: load it at file level there, local {name} = world.load(\"{name}\")")));
        };
        let to = &worlds[t];
        if to.radius.is_some() {
            return Err(fail(format!("map {name} streams through the Mode 7 plane (radius or view): enter maps drawn on backgrounds")));
        }
        if from.period.is_some() || to.period.is_some() {
            return Err(fail("a generated world has no edge to leave by: w:enter swaps maps of fixed size".into()));
        }
        let pixel = |v: Value<'_>| match v {
            Value::Integer(i) => Some(i),
            Value::Number(n) if n.is_finite() => Some(n.floor() as i64),
            _ => None,
        };
        let (Some(x), Some(y), Value::Nil) =
            (pixel(stack.get(2)), pixel(stack.get(3)), stack.get(4))
        else {
            return Err(fail("w:enter(name, x, y) takes the map's name and the camera's map pixel position, two finite numbers".into()));
        };
        let (right, bottom) = (
            (to.map.width as i64 * 8 - 256).max(0),
            (to.map.height as i64 * 8 - 224).max(0),
        );
        if x < 0 || x > right || y < 0 || y > bottom {
            return Err(fail(format!("the camera at {x}, {y} leaves map {name}: keep x in 0..{right} and y in 0..{bottom} (clamp it)")));
        }
        let owner = from.owner.clone();
        let wide = to.map.width > 32;
        let placements = rec.placements.borrow();
        let mut moved = [None; 4];
        let mut chars = [Value::Nil; 4];
        for (i, b) in from.bound.iter().enumerate() {
            let Some(b) = b else { continue };
            let layer = &from.map.layers[b.layer].name;
            let Some(li) = to.map.layers.iter().position(|l| l.name == *layer) else {
                return Err(fail(format!(
                    "map {name} has no tile layer named {layer}, which bg[{}] shows",
                    i + 1
                )));
            };
            if to.map.layers[li].mode7 {
                return Err(fail(format!(
                    "layer {layer} of map {name} draws from a Mode 7 tileset, not a sheet"
                )));
            }
            let (char_base, pal) = sheet_layer(&placements, to, li, &owner).map_err(fail)?;
            let (old_char, _) = sheet_layer(&placements, from, b.layer, &owner).map_err(fail)?;
            if (char_base, pal) != (old_char, b.pal) {
                return Err(fail(format!(
                    "layer {layer} of map {name} draws from a sheet placed at char {char_base:#06x}, palette {pal}, but bg[{}] shows one at char {old_char:#06x}, palette {}: draw both maps' {layer} from the same placement",
                    i + 1, b.pal)));
            }
            map_room(rec, b.map as i64, wide, i + 1, &owner).map_err(fail)?;
            // A wrapping binding repeats the entered map's own painted part.
            let wrap = match b.wrap {
                None => None,
                Some(_) => {
                    let (w, h) = crate::tiled::painted(&to.map, li).ok_or_else(|| {
                        fail(format!("layer {layer} of map {name} has no tiles to repeat (wrap)"))
                    })?;
                    Some((w as i64, h as i64))
                }
            };
            moved[i] = Some(WorldBinding {
                layer: li,
                map: b.map,
                wide,
                pal,
                wrap,
                priority: b.priority,
            });
            chars[i] = Value::Integer(char_base as i64);
        }
        drop(placements);
        // The one change: these backgrounds now show map `name` (clear first, so a same-map jump keeps them),
        // drawn as authored: w:set's changes to it go.
        worlds[t].live.iter_mut().for_each(|l| *l = None);
        for (i, m) in moved.iter().enumerate() {
            if m.is_some() {
                worlds[w].bound[i] = None;
                worlds[t].bound[i] = *m;
            }
        }
        stack.replace(
            ctx,
            (t as i64 + 1, wide, chars[0], chars[1], chars[2], chars[3]),
        );
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("__ppu_world_enter", enter).unwrap();

    // (world, bg number, x, y, cells, column): present a row (or a column, down from x, y) of
    // the layer bound to that background into its VRAM map.
    let strip_rec = rec.clone();
    let strip = Callback::from_fn(&ctx, move |ctx, _, stack| {
        let rec = &strip_rec;
        let w = world_of(ctx, rec, stack.get(0))
            .map_err(|m| lua_err(ctx, &format!("error[snes.world]: world strip: {m}")))?;
        let worlds = rec.worlds.borrow();
        let world = &worlds[w];
        if stack.get(1).to_int() == Some(0) {
            // The Mode 7 plane: a row of tile numbers into m7.map, which changes only each VRAM word's low byte
            // (the high byte is the tiles' pixels).
            let li = world
                .plane
                .ok_or_else(|| lua_err(ctx, "error[snes.world]: world strip: the plane has no bound layer"))?;
            let [x, y, n] = [2, 3, 4].map(|i| stack.get(i).to_int().unwrap_or(-1));
            let column = stack.get(5).to_bool();
            let (width, height) = if world.radius.is_some() {
                (world.map.width as i64, world.map.height as i64)
            } else {
                (128, 128)
            };
            let (ex, ey) = if column {
                (x + 1, y + n)
            } else {
                (x + n, y + 1)
            };
            if world.period.is_some() {
                // A generated world: (x, y) is a cell of the ground, and each cell comes from the template its
                // chunk shows (`__ppu_world_chunk`), at the cell's place in the chunk.
                if x < 0 || y < 0 || n < 1 {
                    return Err(lua_err(
                        ctx,
                        &format!("error[snes.world]: world strip {x},{y} of {n} cells is not on the generated ground"),
                    ));
                }
                let plane = world.words[li]
                    .as_ref()
                    .expect("a bound plane has its cells");
                let Value::Table(m7) = ctx.get_global("m7") else {
                    return Err(lua_err(ctx, "error[snes.world]: m7 is not a table"));
                };
                let Value::Table(map) = m7.get(ctx, "map") else {
                    return Err(lua_err(ctx, "error[snes.world]: m7.map is not a table"));
                };
                for i in 0..n {
                    let (cx, cy) = if column { (x, y + i) } else { (x + i, y) };
                    let Some((sx, sy)) = world.chunks[(cy / 16 % 16 * 16 + cx / 16 % 16) as usize]
                    else {
                        return Err(lua_err(ctx, &format!("error[snes.world]: world strip: no template was picked for the chunk of cell {cx},{cy}")));
                    };
                    let row = match map.get(ctx, cy % 128) {
                        Value::Table(t) => t,
                        _ => {
                            let t = Table::new(&ctx);
                            map.set(ctx, cy % 128, t).unwrap();
                            t
                        }
                    };
                    row.set(
                        ctx,
                        cx % 128,
                        plane[((sy + cy % 16) * width + sx + cx % 16) as usize] as i64,
                    )
                    .unwrap();
                }
                return Ok(CallbackReturn::Return);
            }
            if x < 0 || y < 0 || n < 1 || ex > width || ey > height {
                return Err(lua_err(ctx, &format!("error[snes.world]: world strip {x},{y} of {n} cells is outside the {width}x{height} plane map")));
            }
            let plane = world.words[li]
                .as_ref()
                .expect("a bound plane has its cells");
            let Value::Table(m7) = ctx.get_global("m7") else {
                return Err(lua_err(ctx, "error[snes.world]: m7 is not a table"));
            };
            let Value::Table(map) = m7.get(ctx, "map") else {
                return Err(lua_err(ctx, "error[snes.world]: m7.map is not a table"));
            };
            for i in 0..n {
                let (cx, cy) = if column { (x, y + i) } else { (x + i, y) };
                let row = match map.get(ctx, cy % 128) {
                    Value::Table(t) => t,
                    _ => {
                        let t = Table::new(&ctx);
                        map.set(ctx, cy % 128, t).unwrap();
                        t
                    }
                };
                row.set(ctx, cx % 128, plane[(cy * width + cx) as usize] as i64)
                    .unwrap();
            }
            return Ok(CallbackReturn::Return);
        }
        let b = stack
            .get(1)
            .to_int()
            .filter(|n| (1..=4).contains(n))
            .and_then(|n| world.bound[n as usize - 1]);
        let b = b.ok_or_else(|| lua_err(ctx, "error[snes.world]: world strip: that background has no bound layer"))?;
        // The layer streamed into a VRAM map plays its tile animations there from now on (a cartridge's
        // ppu_world_anim_on does the same), so a map entered over it stops its old layer's.
        {
            let animated = !world.map.layers[b.layer].animations.is_empty();
            let char_base = if animated {
                sheet_layer(&rec.placements.borrow(), world, b.layer, &scene_owner(ctx))
                    .map_or(0, |(c, _)| c)
            } else {
                0
            };
            let mut on = rec.world_animated.borrow_mut();
            on.retain(|&(m, ..)| m != b.map);
            on.push((b.map, w, b.layer, char_base));
        }
        let [x, y, n] = [2, 3, 4].map(|i| stack.get(i).to_int().unwrap_or(-1));
        let column = stack.get(5).to_bool();
        let (width, height) = (world.map.width as i64, world.map.height as i64);
        if let Some(p) = world.period {
            // Generated ground on a background: (x, y) is a cell of the ground (wrapped at the period), each cell
            // from the template its chunk shows (`__ppu_world_chunk`), into the 64x32 map at the cell's place.
            if x < 0 || y < 0 || !(1..=64).contains(&n) {
                return Err(lua_err(ctx, &format!("error[snes.world]: world strip {x},{y} of {n} cells is not on the generated ground")));
            }
            let (cw, ch) = world.chunk;
            let words = world.words[b.layer].as_ref().expect("a bound layer has words");
            let Value::Table(vram) = ctx.get_global("vram") else {
                return Err(lua_err(ctx, "error[snes.world]: vram is not a table"));
            };
            for i in 0..n {
                let (cx, cy) = if column { (x, (y + i) % p) } else { ((x + i) % p, y % p) };
                let cx = cx % p;
                let Some((sx, sy)) = world.chunks[(cy / ch % 16 * 16 + cx / cw % 16) as usize] else {
                    return Err(lua_err(ctx, &format!("error[snes.world]: world strip: no template was picked for the chunk of cell {cx},{cy}")));
                };
                let at = b.map as i64 + if cx % 64 >= 32 { 0x400 } else { 0 } + (cy % 32) * 32 + cx % 32;
                let word = words[((sy + cy % ch) * width + sx + cx % cw) as usize];
                vram.set(ctx, at, (word + (b.pal << 10) + b.priority) as i64).unwrap();
            }
            return Ok(CallbackReturn::Return);
        }
        let (ex, ey) = if column {
            (x + 1, y + n)
        } else {
            (x + n, y + 1)
        };
        if x < 0 || y < 0 || n < 1 || ex > width || ey > height {
            return Err(lua_err(
                ctx,
                &format!("error[snes.world]: world strip {x},{y} of {n} cells is outside the {width}x{height} map"),
            ));
        }
        let words = world.words_of(b.layer).expect("a bound layer has words");
        let Value::Table(vram) = ctx.get_global("vram") else {
            return Err(lua_err(ctx, "error[snes.world]: vram is not a table"));
        };
        for i in 0..n {
            let (cx, cy) = if column { (x, y + i) } else { (x + i, y) };
            let (wx, wy) = b.wrap.map_or((cx, cy), |(w, h)| (cx % w, cy % h));
            let at = b.map as i64
                + if b.wide && cx % 64 >= 32 { 0x400 } else { 0 }
                + (cy % 32) * 32
                + cx % 32;
            vram.set(
                ctx,
                at,
                (words[(wy * width + wx) as usize] + (b.pal << 10) + b.priority) as i64,
            )
            .unwrap();
        }
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("__ppu_world_strip", strip).unwrap();

    // (world, slot, x, y): a generated world's chunk in grid slot `slot` (chunk y % 16 * 16 + chunk x % 16)
    // shows the template that starts at map cell (x, y). The strips after it read the grid.
    let chunk_rec = rec.clone();
    let chunk = Callback::from_fn(&ctx, move |ctx, _, stack| {
        let rec = &chunk_rec;
        let w = world_of(ctx, rec, stack.get(0))
            .map_err(|m| lua_err(ctx, &format!("error[snes.world]: world chunk: {m}")))?;
        let mut worlds = rec.worlds.borrow_mut();
        let world = &mut worlds[w];
        let [slot, x, y] = [1, 2, 3].map(|i| stack.get(i).to_int().unwrap_or(-1));
        let (width, height) = (world.map.width as i64, world.map.height as i64);
        if world.period.is_none()
            || !(0..256).contains(&slot)
            || x < 0
            || y < 0
            || x % world.chunk.0 != 0
            || y % world.chunk.1 != 0
            || x >= width
            || y >= height
        {
            return Err(lua_err(
                ctx,
                &format!(
                    "error[snes.world]: world chunk {slot}: {x},{y} is not a template of the {width}x{height} map"
                ),
            ));
        }
        world.chunks[slot as usize] = Some((x, y));
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("__ppu_world_chunk", chunk).unwrap();

    // (world, layer number, px, py, tile) -> cell x, cell y, cells a side: w:set's one change. The Tiled tile under
    // the pixel (k x k cells, k the map's tile size / 8) becomes tile id `tile` of the layer's tileset, its words
    // and its kinds; world.luau redraws what the camera shows.
    let set_rec = rec.clone();
    let set = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let rec = &set_rec;
        let w = world_of(ctx, rec, stack.get(0)).map_err(|m| lua_err(ctx, &format!("error[snes.world]: w:set: {m}")))?;
        let mut worlds = rec.worlds.borrow_mut();
        let world = &mut worlds[w];
        let fail = |m: String| lua_err(ctx, &format!("error[snes.world]: w:set (world {}): {m}", world.name));
        if world.radius.is_some() || world.period.is_some() {
            return Err(fail("w:set changes maps drawn on backgrounds; a world streamed through the Mode 7 plane (radius, view or period) has no cells to change".into()));
        }
        let li = stack.get(1).to_int().filter(|&i| i >= 1 && i as usize <= world.map.layers.len());
        let li = li.ok_or_else(|| fail("no such layer".into()))? as usize - 1;
        let layer = &world.map.layers[li];
        if layer.mode7 {
            return Err(fail(format!("layer {} draws from a Mode 7 tileset: w:set changes sheet layers on backgrounds", layer.name)));
        }
        if world.bound.iter().flatten().any(|b| b.layer == li && b.wrap.is_some()) {
            return Err(fail(format!("layer {} is bound with wrap, which repeats it as Tiled drew it: w:set changes layers bound without wrap", layer.name)));
        }
        if layer.tile_kinds.is_empty() {
            return Err(fail(format!("layer {} draws no tiles, so it has no tileset to set a tile from: draw at least one tile on it in Tiled", layer.name)));
        }
        let (Some(px), Some(py), Some(tile)) = (stack.get(2).to_int(), stack.get(3).to_int(), stack.get(4).to_int()) else {
            return Err(fail(format!("w:set(layer, x, y, tile) takes a map pixel position and a tile id, whole numbers (got {}, {}, {})", stack.get(2), stack.get(3), stack.get(4))));
        };
        let (width, height) = (world.map.width as i64, world.map.height as i64);
        if !(0..width * 8).contains(&px) || !(0..height * 8).contains(&py) {
            return Err(fail(format!("the pixel {px}, {py} is off the {}x{} px map", width * 8, height * 8)));
        }
        let count = layer.tile_kinds.len() as i64;
        if !(0..count).contains(&tile) {
            return Err(fail(format!("tile {tile} is not in layer {}'s tileset, whose ids are 0 to {} (Tiled's tileset panel shows each tile's ID)", layer.name, count - 1)));
        }
        let k = world.map.tile_size as i64 / 8;
        let per = (k * k) as usize;
        let Some(cells) = layer.tiles[tile as usize * per..][..per].iter().copied().collect::<Option<Vec<u16>>>() else {
            return Err(fail(format!("tile {tile} of layer {}'s tileset is past the chars of sheet {}: the tileset image is larger than the source", layer.name, layer.source.as_deref().unwrap_or("?"))));
        };
        let kind = layer.tile_kinds[tile as usize];
        let (cx, cy) = (px / 8 / k * k, py / 8 / k * k);
        if world.live[li].is_none() {
            let words = world.words[li].clone().expect("a sheet layer with tiles has words");
            world.live[li] = Some((words, world.map.layers[li].kinds.clone()));
        }
        let (words, kinds) = world.live[li].as_mut().expect("just made");
        for dy in 0..k {
            for dx in 0..k {
                let at = ((cy + dy) * width + cx + dx) as usize;
                words[at] = cells[(dy * k + dx) as usize];
                kinds[at] = kind;
            }
        }
        stack.replace(ctx, (cx, cy, k));
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("__ppu_world_set", set).unwrap();

    // (world, layer number, px, py) -> the kind at that pixel; 0 off the map.
    let kind = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let w =
            world_of(ctx, &rec, stack.get(0)).map_err(|m| lua_err(ctx, &format!("error[snes.world]: w:tile: {m}")))?;
        let worlds = rec.worlds.borrow();
        let map = &worlds[w].map;
        let li = stack
            .get(1)
            .to_int()
            .filter(|&i| i >= 1 && i as usize <= map.layers.len());
        let li = li.ok_or_else(|| lua_err(ctx, "error[snes.world]: w:tile: no such layer"))? as usize - 1;
        let kinds = worlds[w].kinds_of(li);
        let (Some(px), Some(py)) = (stack.get(2).to_int(), stack.get(3).to_int()) else {
            return Err(lua_err(
                ctx,
                "error[snes.world]: w:tile needs a pixel position: w:tile(layer, x, y)",
            ));
        };
        let (cx, cy) = (px.div_euclid(8), py.div_euclid(8));
        let k = if (0..map.width as i64).contains(&cx) && (0..map.height as i64).contains(&cy) {
            kinds[cy as usize * map.width + cx as usize] as i64
        } else {
            0
        };
        stack.replace(ctx, k);
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("__ppu_world_kind", kind).unwrap();
}

/// The palette source `pn` as art `art_name` (`art`) takes it: it exists, is a palette, and fits
/// (`source::palette_fits`). The one place dma and sprites.art ask; the error is the refusal text.
fn palette_for(
    store: &HashMap<String, crate::source::SourcePayload>,
    art_name: &str,
    art: &crate::source::SourcePayload,
    pn: &str,
) -> Result<crate::source::PaletteSource, String> {
    match store.get(pn) {
        None => Err(format!("no palette source named '{pn}'")),
        Some(crate::source::SourcePayload::Palette(p)) => crate::source::palette_fits(art, p)
            .map(|_| p.clone())
            .map_err(|e| format!("'{art_name}' with palette '{pn}': {e}")),
        Some(other) => Err(format!("'{pn}' is a {} source, not a palette", other.kind().name())),
    }
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
    crate::lua_raster::install(ctx, rec.raster.clone());
    let timer_rec = rec.clone();
    let hold_rec = rec.clone();
    let song_store = store.clone();
    let song_rec = rec.clone();
    let dma = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        // A call outside setup is a stage load: only from frame(), only blanked.
        let runtime = !rec.active.get();
        let owner = scene_owner(ctx);
        // Two scenes never run together, so their setups never conflict.
        let shared = |other: &Option<String>| {
            !rec.overlaps.get() && (owner.is_none() || other.is_none() || *other == owner)
        };
        if runtime && !rec.in_frame.get() {
            return Err(lua_err(
                ctx,
                "error[snes.dma]: dma runs during setup — call it from top-level code, init() or frame(), not hooks",
            ));
        }
        if runtime && !ctx.get_global("force_blank").to_bool() {
            return Err(lua_err(
                ctx,
                "error[snes.dma]: dma runs during setup, or in frame() while force_blank is true — set force_blank = true before loading",
            ));
        }
        let name = match stack.get(0) {
            Value::String(s) => String::from_utf8_lossy(s.as_bytes()).into_owned(),
            _ => {
                return Err(lua_err(
                    ctx,
                    "error[snes.dma]: dma: first argument must be a source name (string)",
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
            None => return Err(lua_err(ctx, &format!("error[snes.dma]: dma: no source named '{name}'"))),
        };
        let kind = source.kind();
        let opts = match stack.get(1) {
            Value::Table(t) => Some(t),
            Value::Nil => None,
            _ => return Err(lua_err(ctx, "error[snes.dma]: dma: opts must be a table")),
        };
        let opt = |key: &'static str| opts.map_or(Value::Nil, |t| t.get(ctx, key));
        let int_opt = |key: &'static str, default: i64, max: i64| match opt(key) {
            Value::Nil => Ok(default),
            v => match v.to_int() {
                Some(n) if (0..=max).contains(&n) => Ok(n),
                _ => Err(lua_err(
                    ctx,
                    &format!("error[snes.dma]: dma: opts.{key} must be an integer in 0..{max:#x}"),
                )),
            },
        };
        let (char_base, map_base, cgram_base) = if kind == SourceKind::Palette {
            for k in ["char", "map", "palette"] {
                if !matches!(opt(k), Value::Nil) {
                    return Err(lua_err(
                        ctx,
                        &format!(
                            "error[snes.dma]: dma: '{name}' is a palette source — it takes only 'pal', not '{k}'"
                        ),
                    ));
                }
            }
            (0, 0, int_opt("pal", 0, 0xff)?)
        } else if kind == SourceKind::M7 || kind == SourceKind::Sample {
            for k in ["char", "map", "pal"]
                .into_iter()
                .chain((kind == SourceKind::Sample).then_some("palette"))
            {
                if !matches!(opt(k), Value::Nil) {
                    let msg = if kind == SourceKind::M7 {
                        format!(
                            "'{name}' is an m7 source — chars+map always live \
                             interleaved at 0x0000 (palette at CGRAM 1), so it takes no '{k}' opt"
                        )
                    } else {
                        format!(
                            "'{name}' is a sample source — it takes only 'addr', not '{k}'"
                        )
                    };
                    return Err(lua_err(ctx, &format!("error[snes.dma]: dma: {msg}")));
                }
            }
            (0, 0, 0)
        } else {
            (
                int_opt("char", 0x1000, 0x7fff)?,
                int_opt("map", 0x0000, 0x7fff)?,
                int_opt("pal", 0, if kind == SourceKind::Obj { 7 } else { 0xff })?,
            )
        };

        // `palette = "name"`: the named palette's rows replace the art's own colours.
        let palette_name = match opt("palette") {
            Value::Nil => None,
            Value::String(s) => Some(String::from_utf8_lossy(s.as_bytes()).into_owned()),
            _ => {
                return Err(lua_err(
                    ctx,
                    "error[snes.dma]: dma: opts.palette must be a palette source name (string)",
                ))
            }
        };
        let palette = match (&palette_name, crate::source::art_row_size(&source)) {
            (Some(pn), Some(_)) => Some(
                palette_for(&store.borrow(), &name, &source, pn)
                    .map_err(|e| lua_err(ctx, &format!("error[snes.dma]: dma: {e}")))?,
            ),
            _ => None,
        };
        // A Song takes no palette either; its arm below refuses it.
        let palette_name = palette_name.filter(|_| palette.is_some());
        let palette_rows = palette.as_ref().map(|p| p.rows.len());

        let align_up = |n: usize, align: usize| (n + align - 1) & !(align - 1);
        let mut vram = Vec::<(usize, usize)>::new();
        let cgram;
        let cgram_end;
        let ret = Table::new(&ctx);
        mark_setup_handle(ctx, &rec, ret);
        match &source {
            crate::source::SourcePayload::Bg(src) => {
                if char_base & 0x0fff != 0 || map_base & 0x03ff != 0 {
                    return Err(lua_err(
                        ctx,
                        "error[snes.dma]: dma: bg char must align to 0x1000 and map to 0x400",
                    ));
                }
                let char_end = char_base as usize + src.char_words.len();
                let map_end = map_base as usize + src.tilemap_words.len();
                let stride = crate::source::row_size_of(src.bit_depth) as usize;
                let pal_end =
                    cgram_base as usize + stride * palette_rows.unwrap_or(src.palettes.len());
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
                    return Err(lua_err(ctx, "error[snes.dma]: dma: sheet char must align to 0x1000"));
                }
                let char_end = char_base as usize + src.char_words.len();
                let stride = crate::source::row_size_of(src.bit_depth) as usize;
                let pal_end =
                    cgram_base as usize + stride * palette_rows.unwrap_or(src.palettes.len());
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
                let known = {
                    let bases = rec.obj_base.borrow();
                    let find = |o: &Option<String>| bases.iter().find(|(b, _)| b == o).map(|(_, base)| *base);
                    find(&owner).or_else(|| find(&None))
                };
                let base = match known {
                    Some(base) => base,
                    None => {
                        if char_base & 0x1fff != 0 {
                            return Err(lua_err(ctx, "error[snes.dma]: dma: first obj char must align to 0x2000"));
                        }
                        // A load is measured from the setup's base; with none, from its own char.
                        if !runtime {
                            rec.obj_base.borrow_mut().push((owner.clone(), char_base as u16));
                        }
                        char_base as u16
                    }
                };
                let char_end = char_base as usize + src.char_words.len();
                if !rec.overlaps.get()
                    && (char_base < base as i64
                        || (char_base - base as i64) % 16 != 0
                        || char_end > base as usize + 0x2000)
                {
                    return Err(lua_err(
                        ctx,
                        "error[snes.dma]: dma: obj chars must fit the shared 512-tile region: OBSEL (obj.char_base) points at one 8K-word region, \
                         so place every sprite sheet in the first one's (from its char, chaining by next_char)",
                    ));
                }
                let pal_end = cgram_base as usize + palette_rows.unwrap_or(src.palettes.len());
                vram.push((char_base as usize, char_end));
                cgram_end = 128 + pal_end * 16;
                cgram = (128 + cgram_base as usize * 16, cgram_end);
                ret.set(ctx, "char", char_base).unwrap();
                ret.set(ctx, "pal", cgram_base).unwrap();
                ret.set(ctx, "tile", (char_base - base as i64) / 16)
                    .unwrap();
                ret.set(ctx, "name", ctx.intern(name.as_bytes())).unwrap();
                ret.set(ctx, "next_char", align_up(char_end, 16) as i64)
                    .unwrap();
                ret.set(ctx, "next_pal", pal_end as i64).unwrap();
                ret.set(ctx, "cell_size", src.cell_size as i64).unwrap();
                ret.set(ctx, "cell_w", src.cell_size as i64).unwrap();
                ret.set(ctx, "cell_h", src.cell_height as i64).unwrap();
                let blocks = (src.cell_size, src.cell_height) != (8, 8);
                let opaque = |t: usize| src.char_words.get(t * 16..t * 16 + 16).is_some_and(|w| w.iter().any(|&w| w != 0));
                let cells = Table::new(&ctx);
                for (i, cell) in src.cells.iter().enumerate() {
                    let t = Table::new(&ctx);
                    t.set(ctx, "tile", cell.tile as i64).unwrap();
                    t.set(ctx, "pal", cell.pal as i64).unwrap();
                    t.set(ctx, "flip_x", cell.flip_x).unwrap();
                    t.set(ctx, "flip_y", cell.flip_y).unwrap();
                    // A block cell's drawn 8x8 tiles, per tile row a bit per column: the sprites library skips
                    // the pieces of a composite cell that show nothing.
                    if blocks {
                        let rows = Table::new(&ctx);
                        for sy in 0..src.cell_height as usize / 8 {
                            let bits = (0..src.cell_size as usize / 8)
                                .filter(|sx| opaque(cell.tile as usize + sy * 16 + sx))
                                .fold(0i64, |m, sx| m | 1 << sx);
                            rows.set(ctx, sy as i64 + 1, bits).unwrap();
                        }
                        t.set(ctx, "opaque", rows).unwrap();
                    }
                    cells.set(ctx, i as i64 + 1, t).unwrap();
                }
                ret.set(ctx, "cells", cells).unwrap();
            }
            crate::source::SourcePayload::M7(src) => {
                vram.push((0, 0x4000));
                // The plane is 8bpp, but it only WRITES entries 1..=colors (its own, or
                // the named palette's real count); OBJ palettes (128+) and offset BG
                // bases past that end can coexist with it, exactly as on hardware.
                cgram_end = match &palette {
                    Some(p) => p.rows[0].len(),
                    None => 1 + src.palette.len(),
                };
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
            crate::source::SourcePayload::Sample(_) if runtime => {
                return Err(lua_err(
                    ctx,
                    &format!("error[snes.dma]: dma runs during setup for samples: '{name}' goes to sound RAM in setup only"),
                ))
            }
            crate::source::SourcePayload::Sample(src) => {
                let recorded = rec.samples.borrow().len();
                if recorded >= 256 {
                    return Err(lua_err(
                        ctx,
                        &format!("error[snes.dma]: dma: '{name}' exceeds the 256-entry sample directory"),
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
                                "error[snes.dma]: dma: opts.addr must be an integer in 0..0xffff",
                            ))
                        }
                    },
                };
                let bytes = src.brr.len() as u32;
                let end = addr + bytes;
                if end > 0x10000 {
                    return Err(lua_err(
                        ctx,
                        &format!("error[snes.dma]: dma: '{name}' placement exceeds sound RAM"),
                    ));
                }
                // Reserved sample directory page: [SAMPLE_DIR, SAMPLE_DIR_END).
                if addr < SAMPLE_DIR_END && SAMPLE_DIR < end {
                    return Err(lua_err(
                        ctx,
                        &format!(
                            "error[snes.dma]: dma: '{name}' overlaps the sample directory page (0x{SAMPLE_DIR:04x}-0x{:04x})",
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
                            "error[snes.dma]: dma: '{name}' overlaps the echo region (0x{echo_lo:04x}-0x{hi:04x}, \
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
                            "error[snes.dma]: dma: '{name}' overlaps sample '{}' in sound RAM",
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
                    loops: src.loop_block.is_some(),
                });
                ret.set(ctx, "id", id as i64).unwrap();
                ret.set(ctx, "addr", addr as i64).unwrap();
                ret.set(ctx, "next_addr", end as i64).unwrap();
                stack.clear();
                stack.replace(ctx, ret);
                return Ok(CallbackReturn::Return);
            }
            crate::source::SourcePayload::Palette(src) => {
                // A 256 block claims only its real colours, as it does lent to art.
                let pal_end = cgram_base as usize
                    + match src.row_size {
                        256 => src.rows[0].len(),
                        n => n as usize * src.rows.len(),
                    };
                cgram_end = pal_end;
                cgram = (cgram_base as usize, pal_end);
                ret.set(ctx, "pal", cgram_base).unwrap();
                ret.set(ctx, "next_pal", pal_end as i64).unwrap();
            }
            // A song is authored data played natively by `score{ song = }`
            // (see `install_dma`'s `__song_load`), never `dma()`'s — same
            // refusal shape as the sample-specific opts check above.
            crate::source::SourcePayload::Song(_) => {
                return Err(lua_err(
                    ctx,
                    &format!(
                        "error[snes.dma]: dma: '{name}' is a song source — score{{ song = \"{name}\" }} plays it, not dma()"
                    ),
                ))
            }
        }
        if vram.iter().any(|&(s, e)| e > 0x8000 || s >= e) || cgram_end > 256 {
            return Err(lua_err(
                ctx,
                &format!("error[snes.dma]: dma: '{name}' placement exceeds PPU memory"),
            ));
        }
        let fields = [
            "next_char",
            "next_map",
            "next_pal",
            "bit_depth",
            "screen_size",
            "tile",
            "cell_size",
        ]
        .into_iter()
        .filter_map(|k| match ret.get(ctx, k) {
            Value::Integer(v) => Some((k, v)),
            _ => None,
        })
        .collect();
        let placement = DmaPlacement {
            name: name.clone(),
            owner: owner.clone(),
            char_base: char_base as u16,
            map_base: map_base as u16,
            cgram_base: cgram_base as u8,
            palette: palette_name,
            fields,
        };
        if runtime {
            // Like hardware DMA, a load overwrites what its spans held, earlier
            // vram[]/cgram[] writes included (later writes win again). The m7
            // plane writes CGRAM from entry 1.
            let cgram = if matches!(source, crate::source::SourcePayload::M7(_)) {
                (1, cgram.1)
            } else {
                cgram
            };
            for (global, spans) in [("vram", vram.clone()), ("cgram", vec![cgram])] {
                if let Value::Table(t) = ctx.get_global(global) {
                    let keys: Vec<Value> = t
                        .iter()
                        .filter(|(k, _)| {
                            k.to_int().is_some_and(|k| {
                                spans
                                    .iter()
                                    .any(|&(s, e)| (s as i64..e as i64).contains(&k))
                            })
                        })
                        .map(|(k, _)| k)
                        .collect();
                    for k in keys {
                        t.set(ctx, k, Value::Nil).unwrap();
                    }
                }
            }
            let inside = |(s, e): (usize, usize), spans: &[(usize, usize)]| {
                s >= e || spans.iter().any(|&(a, b)| a <= s && e <= b)
            };
            let mut loads = rec.loads.borrow_mut();
            loads.retain(|l| {
                !(l.vram.iter().all(|&r| inside(r, &vram)) && inside(l.cgram, &[cgram]))
            });
            loads.push(DmaLoad {
                placement,
                vram,
                cgram,
            });
            rec.loaded.set(true);
            stack.clear();
            stack.replace(ctx, ret);
            return Ok(CallbackReturn::Return);
        }
        for (i, &(s, e)) in vram.iter().enumerate() {
            if vram[..i].iter().any(|&(a, b)| s < b && a < e)
                || rec
                    .vram_ranges
                    .borrow()
                    .iter()
                    .any(|(_, a, b, o)| s < *b && *a < e && shared(o))
            {
                return Err(lua_err(
                    ctx,
                    &format!("error[snes.dma]: dma: '{name}' overlaps VRAM already in use"),
                ));
            }
        }
        if rec.cgram_ranges.borrow().iter().any(|(_, start, end, o)| {
            cgram.0 != *start && cgram.0 < *end && *start < cgram.1 && shared(o)
        }) {
            return Err(lua_err(
                ctx,
                &format!("error[snes.dma]: dma: '{name}' conflicts with CGRAM already in use"),
            ));
        }
        rec.vram_ranges.borrow_mut().extend(
            vram.into_iter()
                .map(|(s, e)| (name.clone(), s, e, owner.clone())),
        );
        rec.cgram_ranges
            .borrow_mut()
            .push((name.clone(), cgram.0, cgram.1, owner.clone()));
        rec.placements.borrow_mut().push(placement);
        stack.clear();
        stack.replace(ctx, ret);
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("dma", dma).unwrap();

    // __sfx_hold(v, sample, len, pitch) -> v: kit.lua's sfx() arms voice v's key-off before
    // keying it on. `len` is seconds, false (no key-off: the note sustains until koff()) or nil:
    // SFX_LEN on a looping sample, none on a one-shot, which ends on its own. Counted in frames by
    // `tick_sfx_holds`. The effect borrows the voice from any song until it ends: its hold, the
    // one-shot's own length at `pitch`, or (len = false) a koff(). With `v` nil the voice is the
    // one `ppu_rules::voices::choose` picks; nil back when every voice is held by explicit effects.
    // __sfx_hold(v, false) releases voice v (no key-off armed, no borrow): a compiled cold boot's
    // power-on, where the cartridge's driver holds nothing.
    let sfx_hold = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        if let (Some(v), Value::Boolean(false)) = (stack.get(0).to_int().filter(|v| (0..=7).contains(v)), stack.get(1)) {
            let v = v as usize;
            let mut hold = hold_rec.sfx_hold.get();
            hold[v] = 0;
            hold_rec.sfx_hold.set(hold);
            let mut owns = hold_rec.sfx_own.get();
            owns[v] = 0;
            hold_rec.sfx_own.set(owns);
            hold_rec.sfx_auto.set(hold_rec.sfx_auto.get() & !(1 << v));
            hold_rec.sfx_armed.set(hold_rec.sfx_armed.get() & !(1 << v));
            stack.clear();
            return Ok(CallbackReturn::Return);
        }
        let v = match stack.get(0) {
            Value::Nil => None,
            v => match v.to_int().filter(|v| (0..=7).contains(v)) {
                Some(v) => Some(v as usize),
                None => {
                    return Err(lua_err(
                        ctx,
                        "error[snes.bank]: sfx: voice numbers must be 0..7",
                    ))
                }
            },
        };
        let sample = stack.get(1).to_int();
        let placed = {
            let samples = hold_rec.samples.borrow();
            samples
                .iter()
                .rev()
                .find(|s| Some(s.id as i64) == sample)
                .map(|s| (s.loops, (s.end.saturating_sub(s.addr as u32) / 9) as f64))
        };
        let sustain = matches!(stack.get(2), Value::Boolean(false));
        let seconds = match stack.get(2) {
            Value::Nil => placed.is_some_and(|(loops, _)| loops).then_some(SFX_LEN),
            Value::Boolean(false) => None,
            len => match len.to_number() {
                Some(s) if s > 0.0 && s.is_finite() => Some(s),
                _ => {
                    return Err(lua_err(
                        ctx,
                        "error[snes.bank]: sfx: len must be seconds above 0, or false to sustain until koff()",
                    ))
                }
            },
        };
        let frames = |s: f64| (s * 60.0).round().clamp(1.0, u16::MAX as f64 - 1.0) as u16;
        let own = match (seconds, placed) {
            _ if sustain => u16::MAX,
            (Some(s), _) => frames(s),
            // A one-shot plays its 16-sample blocks once, at 32 kHz scaled by its pitch.
            (None, Some((_, blocks))) => {
                let pitch = stack.get(3).to_number().unwrap_or(4096.0).max(1.0);
                frames(blocks * 16.0 * 4096.0 / (32000.0 * pitch))
            }
            (None, None) => 1,
        };
        let v = match v {
            Some(v) => v,
            None => {
                let ticks = ppu_rules::voices::ticks(own).min(u16::MAX as u32) as u16;
                match ppu_rules::voices::choose(&hold_rec.voice_view(), hold_rec.keep_mask(), ticks) {
                    Some(v) => v as usize,
                    None => {
                        stack.clear();
                        return Ok(CallbackReturn::Return);
                    }
                }
            }
        };
        let auto = stack.get(0).is_nil();
        let mut hold = hold_rec.sfx_hold.get();
        hold[v] = seconds.map_or(0, frames);
        hold_rec.sfx_hold.set(hold);
        let mut owns = hold_rec.sfx_own.get();
        owns[v] = own;
        hold_rec.sfx_own.set(owns);
        let mut since = hold_rec.sfx_since.get();
        since[v] = hold_rec.sfx_seq.get();
        hold_rec.sfx_since.set(since);
        hold_rec.sfx_seq.set(since[v].wrapping_add(1));
        hold_rec
            .sfx_auto
            .set(hold_rec.sfx_auto.get() & !(1 << v) | (auto as u8) << v);
        hold_rec.sfx_armed.set(hold_rec.sfx_armed.get() | 1 << v);
        hold_rec.kon_open.set(hold_rec.kon_open.get() & !(1 << v));
        stack.replace(ctx, v as i64);
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("__sfx_hold", sfx_hold).unwrap();

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
                "error[snes.timer]: timer runs during setup — call it from top-level code, not frame() or hooks",
            ));
        }
        let n = match stack.get(0).to_int() {
            Some(n) if (0..=2).contains(&n) => n as u8,
            _ => return Err(lua_err(ctx, "error[snes.timer]: timer: n must be an integer in 0..2")),
        };
        let div = match stack.get(1).to_int() {
            Some(d) if (1..=255).contains(&d) => d as u8,
            _ => return Err(lua_err(ctx, "error[snes.timer]: timer: div must be an integer in 1..255")),
        };
        let f = match stack.get(2) {
            Value::Function(f) => f,
            _ => return Err(lua_err(ctx, "error[snes.timer]: timer: third argument must be a function")),
        };
        let file = function_chunk_name(&f);
        stack.clear();
        timer_rec.timers.borrow_mut().push((
            n,
            div,
            Hook::Lua(ctx.stash(f)),
            file,
            scene_owner(ctx),
        ));
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("timer", timer).unwrap();

    // `__ppu_palette_fits(art, palette)`: dma's own palette rule for engine Lua, before anything is
    // placed. Returns the palette's row count, or raises the refusal dma would.
    let fit_store = song_store.clone();
    let palette_fits = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (Value::String(art), Value::String(pal)) = (stack.get(0), stack.get(1)) else {
            return Err(lua_err(ctx, "error[snes.dma]: __ppu_palette_fits: arguments must be source names (strings)"));
        };
        let (art, pal) = (
            String::from_utf8_lossy(art.as_bytes()).into_owned(),
            String::from_utf8_lossy(pal.as_bytes()).into_owned(),
        );
        stack.clear();
        let store = fit_store.borrow();
        let Some(source) = store.get(&art) else {
            return Err(lua_err(ctx, &format!("error[snes.dma]: no source named '{art}'")));
        };
        let p = palette_for(&store, &art, source, &pal).map_err(|e| lua_err(ctx, &format!("error[snes.dma]: {e}")))?;
        stack.push_back(Value::Integer(p.rows.len() as i64));
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("__ppu_palette_fits", palette_fits).unwrap();

    install_song_natives(ctx, song_store, song_rec);
}

/// A song's rows as `__song_load` hands them to kit.lua: `{ sound, note? }`.
fn song_rows<'gc>(ctx: piccolo::Context<'gc>, song: &crate::song::Song) -> Table<'gc> {
    let rows = Table::new(&ctx);
    for (i, row) in song.rows.iter().enumerate() {
        let t = Table::new(&ctx);
        t.set(ctx, "sound", ctx.intern(row.sound.as_bytes()))
            .unwrap();
        t.set(ctx, "note", row.note.map(|n| n as i64)).unwrap();
        rows.set(ctx, i as i64 + 1, t).unwrap();
    }
    rows
}

/// Install `__song_load`/`__song_start`/`__song_get`/`__song_play`/
/// `__song_stop` — the natives kit.lua's `score{ song = }` path drives (see
/// `kit.lua`'s own doc comment). `load`/`start` share `dma`/`timer`'s
/// init-window gate; `get`/`play`/`stop` are callable any time (frame(),
/// hooks), matching the `score{}` handle's `play()`/`stop()`.
fn install_song_natives(
    ctx: piccolo::Context<'_>,
    store: Rc<RefCell<HashMap<String, crate::source::SourcePayload>>>,
    rec: Rc<DmaRecorder>,
) {
    let load_rec = rec.clone();
    let song_load = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        if !load_rec.active.get() {
            return Err(lua_err(
                ctx,
                "error[snes.song]: __song_load runs during setup — call it from top-level code, not frame() or hooks",
            ));
        }
        let name = match stack.get(0) {
            Value::String(s) => String::from_utf8_lossy(s.as_bytes()).into_owned(),
            _ => {
                return Err(lua_err(
                    ctx,
                    "error[snes.song]: __song_load: first argument must be a source name (string)",
                ))
            }
        };
        let bytes = match store.borrow().get(&name) {
            Some(crate::source::SourcePayload::Song(bytes)) => bytes.clone(),
            _ => {
                stack.clear();
                stack.replace(ctx, Value::Nil);
                return Ok(CallbackReturn::Return);
            }
        };
        let song = crate::song::decode(&bytes)
            .map_err(|e| lua_err(ctx, &format!("error[snes.score]: score: song '{name}': {e}")))?;
        let compiled = crate::song::compile(&song)
            .map_err(|e| lua_err(ctx, &format!("error[snes.score]: score: song '{name}': {e}")))?;
        let id = load_rec.songs.borrow().len();
        let rows = song_rows(ctx, &song);
        load_rec.songs.borrow_mut().push(SongPlayer {
            name: name.clone(),
            events: compiled.events,
            length: compiled.timing.length,
            mask: song.voice_mask,
            looping: false,
            playing: false,
            anchor: Anchor::Stopped,
            declared: Anchor::Stopped,
            shift_h: 0,
            at_false: false,
            tick: 0,
            next: 0,
            cursor: 0,
            ends: [None; 8],
            insts: Vec::new(),
            pitches: Vec::new(),
            song,
            timing: compiled.timing,
        });
        stack.clear();
        stack.replace(ctx, (id as i64, rows));
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("__song_load", song_load).unwrap();

    let start_rec = rec.clone();
    let song_start = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        if !start_rec.active.get() {
            return Err(lua_err(
                ctx,
                "error[snes.song]: __song_start runs during setup — call it from top-level code, not frame() or hooks",
            ));
        }
        let id = match stack.get(0).to_int() {
            Some(n) if n >= 0 => n as usize,
            _ => {
                return Err(lua_err(
                    ctx,
                    "error[snes.song]: __song_start: id must be a non-negative integer",
                ))
            }
        };
        let insts_t = match stack.get(1) {
            Value::Table(t) => t,
            _ => return Err(lua_err(ctx, "error[snes.song]: __song_start: insts must be a table")),
        };
        let pitches_t = match stack.get(2) {
            Value::Table(t) => t,
            _ => return Err(lua_err(ctx, "error[snes.song]: __song_start: pitches must be a table")),
        };
        let looping = stack.get(3).to_bool();
        // Seconds from t = 0, or false/nil: set up, not playing.
        let anchor = match stack.get(4).to_number() {
            Some(at) => Anchor::At((at * 64000.0).round() as i64),
            None => Anchor::Stopped,
        };
        let n = insts_t.length();
        let mut insts = Vec::with_capacity(n.max(0) as usize);
        let mut pitches = Vec::with_capacity(n.max(0) as usize);
        for i in 1..=n {
            let inst = match insts_t.get(ctx, i) {
                Value::Table(t) => t,
                _ => return Err(lua_err(ctx, "error[snes.song]: __song_start: insts must hold tables")),
            };
            insts.push(ctx.stash(inst));
            pitches.push(pitches_t.get(ctx, i).to_int().unwrap_or(0x1000));
        }
        if let Some(p) = start_rec.songs.borrow_mut().get_mut(id) {
            p.insts = insts;
            p.pitches = pitches;
            p.looping = looping;
            p.playing = anchor != Anchor::Stopped;
            p.set_anchor(anchor);
            p.declared = anchor;
            p.at_false = anchor == Anchor::Stopped;
            p.rewind();
        }
        start_rec
            .timers
            .borrow_mut()
            .push((0u8, 32u8, Hook::Song(id), None, scene_owner(ctx)));
        stack.clear();
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("__song_start", song_start).unwrap();

    let get_rec = rec.clone();
    let song_get = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let id = stack.get(0).to_int().unwrap_or(-1);
        let key = match stack.get(1) {
            Value::String(s) => String::from_utf8_lossy(s.as_bytes()).into_owned(),
            _ => String::new(),
        };
        let songs = get_rec.songs.borrow();
        let val = match id.try_into().ok().and_then(|id: usize| songs.get(id)) {
            Some(p) => match key.as_str() {
                "tick" => Value::Integer(p.tick),
                "length" => Value::Integer(p.length),
                "playing" => Value::Boolean(p.playing),
                "loop" => Value::Boolean(p.looping),
                _ => Value::Nil,
            },
            None => Value::Nil,
        };
        stack.clear();
        stack.replace(ctx, val);
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("__song_get", song_get).unwrap();

    let play_rec = rec.clone();
    // Restart from the top, anchored at the start of the span being
    // rendered next (see `LuaEngine::realign`). A restart keys off what
    // the song had sounding, as `stop()` does. One score plays at a time:
    // playing one stops any other playing, as the cartridge's sound driver
    // does (the cartridge sound driver's `score_play`).
    let song_play = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        if let Some(id) = stack.get(0).to_int().and_then(|n| usize::try_from(n).ok()) {
            let mut songs = play_rec.songs.borrow_mut();
            for (_, p) in songs.iter_mut().enumerate().filter(|(i, p)| *i != id && p.playing) {
                p.set_anchor(Anchor::Stopped);
                p.playing = false;
                song_koff(ctx, p.mask & !play_rec.owned());
                p.ends = [None; 8];
            }
            if let Some(p) = songs.get_mut(id) {
                if p.anchor != Anchor::Stopped {
                    song_koff(ctx, p.mask & !play_rec.owned());
                }
                p.set_anchor(Anchor::Pending);
                p.playing = true;
                p.at_false = false;
            }
        }
        stack.clear();
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("__song_play", song_play).unwrap();

    let song_stop = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        if let Some(id) = stack.get(0).to_int().and_then(|n| usize::try_from(n).ok()) {
            if let Some(p) = rec.songs.borrow_mut().get_mut(id) {
                p.set_anchor(Anchor::Stopped);
                p.playing = false;
                p.at_false = false;
                song_koff(ctx, p.mask & !rec.owned());
                p.ends = [None; 8];
            }
        }
        stack.clear();
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("__song_stop", song_stop).unwrap();
}

/// Re-run the init-recorded `dma()` placements in call order into the frame's
/// zeroed VRAM/CGRAM. The payload's committed kind decides the layout — NOT
/// the frame-wide mode (the whole point of M11: an m7 payload and a tile bg
/// coexist in one frame). Names resolve against the LIVE store: an
/// `add_source` under a placed name flows in next frame; a removed one
/// degrades to a Mismatch report (mirrors the old binding-path UX) and places
/// nothing.
/// The tile animations playing at frame counter `f` (the scene's): over the placed sheets, each animated tile's chars
/// hold its frame's chars, for the layer last streamed into each VRAM map (`DmaRecorder::world_animated`). Before the
/// program's own `vram[]` writes, which win, as on a cartridge (its present copies the frames before the stage).
fn animate_worlds(
    rec: &DmaRecorder,
    store: &HashMap<String, crate::source::SourcePayload>,
    f: u32,
    mem: &mut Memory,
) {
    use crate::source::SourcePayload;
    let worlds = rec.worlds.borrow();
    for &(_, w, li, char_base) in rec.world_animated.borrow().iter() {
        let layer = &worlds[w].map.layers[li];
        let Some(SourcePayload::Sheet(sheet)) = layer.source.as_ref().and_then(|s| store.get(s)) else {
            continue;
        };
        let per = sheet.bit_depth as usize * 4;
        for a in &layer.animations {
            let (from, _) = &a.frames[a.frame_at(u64::from(f))];
            for (&to, &from) in a.chars.iter().zip(from) {
                let src = &sheet.char_words[from as usize * per..][..per];
                for (i, &word) in src.iter().enumerate() {
                    mem.vram[(char_base as usize + to as usize * per + i) & 0x7fff] = word;
                }
            }
        }
    }
}

fn replay_dma<'a>(
    placements: impl Iterator<Item = &'a DmaPlacement>,
    store: &HashMap<String, crate::source::SourcePayload>,
    reports: &mut Vec<ImportBudget>,
    mem: &mut Memory,
) {
    use crate::source::{palette_fits, SourcePayload};
    for p in placements {
        let Some(art) = store.get(&p.name) else {
            reports.push(ImportBudget::Mismatch {
                layer: None,
                slot: p.name.clone(),
                expected: "dma placement".into(),
                found: MISSING_SOURCE.into(),
            });
            continue;
        };
        // The named palette, live like the art; one that is gone, changed kind or row size
        // reports and leaves the art its own colours.
        let palette = p.palette.as_ref().and_then(|pn| {
            let fit = match store.get(pn) {
                Some(SourcePayload::Palette(pal)) => palette_fits(art, pal).map(|_| pal),
                Some(other) => Err(format!("a {} source, not a palette", other.kind().name())),
                None => Err(MISSING_SOURCE.into()),
            };
            match fit {
                Ok(pal) => Some(pal),
                Err(found) => {
                    reports.push(ImportBudget::Mismatch {
                        layer: None,
                        slot: p.name.clone(),
                        expected: format!("palette '{pn}'"),
                        found,
                    });
                    None
                }
            }
        });
        let cgram = p.cgram_base as usize;
        match art {
            SourcePayload::Bg(src) => {
                crate::source::place_bg_in(src, mem, p.map_base, p.char_base, cgram, palette)
            }
            SourcePayload::Sheet(src) => {
                crate::source::place_sheet_in(src, mem, p.char_base, cgram, palette)
            }
            SourcePayload::M7(src) => crate::source::place_m7_in(src, mem, palette),
            SourcePayload::Obj(src) => {
                crate::source::place_obj_in(src, mem, p.char_base, cgram, palette)
            }
            SourcePayload::Palette(src) => crate::source::place_palette(src, mem, cgram),
            // Samples are written once by `LuaEngine::set_sources` after
            // `init()`, never replayed per frame — a `dma()` placement
            // naming one is a no-op here.
            SourcePayload::Sample(_) => {}
            // Songs are never `dma()`'d (the `dma` callback refuses the
            // kind), so a placement can't name one — kept only so this
            // match stays exhaustive over `SourcePayload`.
            SourcePayload::Song(_) => {}
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

/// A refusal that belongs to a project file (a Tiled map), not to the Luau line that loaded it:
/// the error names the map's file, so the Studio shows it on the map.
#[derive(Debug)]
struct ProjectFileFailure {
    file: String,
    message: String,
}
impl std::fmt::Display for ProjectFileFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ProjectFileFailure {}

/// `e`, the error `ex` ended with, placed at the innermost line of the author's own files it passed
/// (not an engine library's) when it names no place of its own: a refusal from `dma` or
/// `sprites.art` points at the line that called it, not at the file that holds `frame`.
fn located(lua: &mut Lua, ex: &piccolo::StashedExecutor, e: StaticError) -> LuaError {
    let mut err = static_error_to_lua(e);
    if err.line.is_some() || err.file.is_some() {
        return err;
    }
    let author = |f: &str| !f.starts_with('@') && (f.ends_with(".luau") || f.ends_with(".lua"));
    let at = lua.enter(|ctx| {
        ctx.fetch(ex)
            .error_trace()
            .into_iter()
            .find_map(|(chunk, line)| {
                let file = String::from_utf8_lossy(chunk.as_bytes()).into_owned();
                author(&file).then_some((file, line.0 as u32 + 1))
            })
    });
    if let Some((file, line)) = at {
        err.file = Some(file);
        err.line = Some(line);
    }
    err
}

fn static_error_to_lua(e: StaticError) -> LuaError {
    if let StaticError::Runtime(rt) = &e {
        if let Some(failure) = rt.downcast::<ProjectFileFailure>() {
            return LuaError {
                file: Some(failure.file.clone()),
                line: None,
                message: failure.message.clone(),
            };
        }
        if let Some(failure) = rt.downcast::<crate::precision::Failure>() {
            return LuaError {
                file: Some(failure.file.clone()),
                line: Some(failure.line),
                message: failure.message.clone(),
            };
        }
    }
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

/// Flush the DSL memory-poke surfaces into `Memory`. Runs AFTER the dma replay
/// has laid down the init placements, so manual pokes compose on top under
/// last-write-wins. Order: structured tilemap/char pokes, then the raw `vram[]`
/// table as the FINAL authority (a raw word write always wins). `cgram[]` is
/// applied as an override on top of the placed palette (only set entries).
/// VRAM is NOT zeroed here — `frame()` zeroes it before the replay.
fn read_memory(ctx: piccolo::Context<'_>, mem: &mut Memory) -> Result<(), String> {
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
                    let addr =
                        crate::bg::map_entry_addr(map_base, screen_size, col as u32, row_i as u32)
                            as usize;
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

    // bg[n].columns -> BG3's offset table: after the structured pokes, before raw vram[].
    write_columns(ctx, mem)?;

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
    Ok(())
}

/// `bg[n].columns[c] = dy`: derive Mode 2's offset table (BG3's map at `bg[3].map_base`) from
/// the set columns and this frame's scroll. No column set: the table is left alone, so raw
/// `vram[]` tables keep working. Anything the rule can't serve is refused with its fix.
fn write_columns<'gc>(ctx: piccolo::Context<'gc>, mem: &mut Memory) -> Result<(), String> {
    use crate::quantize::{bg_map_base, bg_screen_size, bg_tile_size, scroll_reg};
    let Value::Table(bg) = ctx.get_global("bg") else { return Ok(()) };
    let layer = |n: i64| match bg.get(ctx, n) {
        Value::Table(t) => Some(t),
        _ => None,
    };
    let num = |t: Option<Table<'gc>>, k: &'static str| t.map_or(0.0, |t| t.get(ctx, k).to_number().unwrap_or(0.0));
    let scroll = |t: Option<Table<'gc>>| match t.map(|t| t.get(ctx, "scroll")) {
        Some(Value::Table(s)) => (num(Some(s), "x"), num(Some(s), "y")),
        _ => (0.0, 0.0),
    };
    let mut used = vec![];
    for n in 1..=4i64 {
        if let Some(Value::Table(c)) = layer(n).map(|t| t.get(ctx, "columns")) {
            if c.iter().next().is_some() {
                used.push((n, c));
            }
        }
    }
    let Some(&(n, cols)) = used.first() else { return Ok(()) };
    if n > 2 {
        return Err(format!("bg[{n}].columns: only bg[1] and bg[2] can move columns (BG3 holds the table in Mode 2)"));
    }
    if used.len() > 1 {
        return Err("bg[1].columns and bg[2].columns are both set: Mode 2 moves the columns of one layer at a time; clear one".into());
    }
    let mode = ctx.get_global("mode").to_int().unwrap_or(0);
    if mode != 2 {
        return Err(format!("bg[{n}].columns needs mode = 2 (offset-per-tile), but mode is {mode}"));
    }
    let lt = layer(n);
    if bg_tile_size(num(lt, "tile_size") as u8) != 8 {
        return Err(format!("bg[{n}].columns needs bg[{n}].tile_size = 8"));
    }
    let t3 = layer(3);
    if bg_tile_size(num(t3, "tile_size") as u8) != 8 || scroll(t3) != (0.0, 0.0) {
        return Err(format!("bg[{n}].columns reads the table at bg[3]: leave bg[3].tile_size 8 and bg[3].scroll at 0, 0"));
    }
    // The table is two map rows (64 words) at bg[3].map_base. Sitting on BG1's or BG2's map is
    // allowed here, as on hardware: the Studio warns and offers a free block, the cart compiler refuses.
    let table = bg_map_base(num(t3, "map_base") as u32) as u32;
    let width = if bg_screen_size(num(lt, "screen_size") as u8) & 1 != 0 { 64 } else { 32 };
    let mut dy = [0i16; 64];
    for (k, v) in cols {
        let (Some(c), Some(d)) = (k.to_int(), v.to_number()) else {
            return Err(format!("bg[{n}].columns[{k:?}] = {v:?}: use a map column number and a whole number of pixels"));
        };
        if !(0..width as i64).contains(&c) {
            return Err(format!("bg[{n}].columns[{c}]: the map is {width} columns wide (0 to {})", width - 1));
        }
        dy[c as usize] = scroll_reg(d as f32);
    }
    let (sx, sy) = scroll(lt);
    let words = crate::bg::columns_vertical_words((n - 1) as u8, width, scroll_reg(sx as f32), scroll_reg(sy as f32), |c| dy[c as usize]);
    for (k, w) in words.into_iter().enumerate() {
        mem.vram[(table as usize + k) & 0x7fff] = 0; // row 0: horizontal entries, unarmed
        mem.vram[(table as usize + 32 + k) & 0x7fff] = w;
    }
    Ok(())
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
