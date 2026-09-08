//! `timer(n, div, fn)` — SPC700 timer hooks + the sample-accurate segment
//! walker (PPU-133). Driven entirely through `LuaEngine`'s public API
//! (`set_source`/`set_sources`/`frame`/`audio`/`memory().vram`,
//! `LineTable.rows[y].wh0/.wh1`) — never reaching into `Dsp` internals.
//! BRR/directory poke helpers copied from `dsp_dsl.rs` (see that file's doc
//! comment for the layout).
use ppu_core::LuaEngine;

fn brr_pokes(base: u16, blocks: usize, last_header: u8) -> String {
    let mut s = String::new();
    for b in 0..blocks {
        let addr = base as usize + b * 9;
        let header = if b == blocks - 1 { last_header } else { 0xc0 };
        s += &format!("aram[{addr}] = {header}\n");
        for (i, byte) in [0x77u8, 0x77, 0x77, 0x77, 0x99, 0x99, 0x99, 0x99]
            .iter()
            .enumerate()
        {
            s += &format!("aram[{}] = {byte}\n", addr + 1 + i);
        }
    }
    s
}

fn dir_entry_pokes(index: u8, start: u16, loop_addr: u16) -> String {
    let addr = 0x0100 + index as usize * 4;
    format!(
        "aram[{a0}] = {b0}\naram[{a1}] = {b1}\naram[{a2}] = {b2}\naram[{a3}] = {b3}\n",
        a0 = addr,
        b0 = start & 0xff,
        a1 = addr + 1,
        b1 = (start >> 8) & 0xff,
        a2 = addr + 2,
        b2 = loop_addr & 0xff,
        a3 = addr + 3,
        b3 = (loop_addr >> 8) & 0xff,
    )
}

/// The `init()` block that pokes a 6-block looping BRR sample (directory
/// entry 0, ARAM 0x0200) — same shape as `dsp_dsl.rs::voice0_program`.
fn voice0_init() -> String {
    format!(
        "function init()\n{sample}{dir}end\n",
        sample = brr_pokes(0x0200, 6, 0xc3),
        dir = dir_entry_pokes(0, 0x0200, 0x0200),
    )
}

/// The per-frame voice-0 register setup shared by every `frame()` in this
/// file (matches `dsp_dsl.rs::voice0_program`'s known-good fields): sample 0,
/// pitch 1:1, full volume, instant-attack/no-decay/full-sustain/no-release
/// ADSR, full master volume.
const VOICE0_SETUP: &str = "\
  voice[0].sample = 0\n\
  voice[0].pitch = 0x1000\n\
  voice[0].vol.l = 127\n\
  voice[0].vol.r = 127\n\
  voice[0].adsr = {a = 15, d = 0, s = 7, r = 0}\n\
  dsp.mvol.l = 127\n\
  dsp.mvol.r = 127\n";

/// The sample offsets (design's h-unit formula, not the implementation) a
/// fresh `timer(n, div, ..)` registration fires at within one span:
/// `period_h = div * (8 for n=0/1 | 1 for n=2)`, `due_h` starts at
/// `period_h`, and it fires while `due_h < span_h`, `off = due_h / 2`.
fn expected_offsets(period_h: u64, span_h: u64) -> Vec<u64> {
    let mut out = Vec::new();
    let mut due = period_h;
    while due < span_h {
        out.push(due / 2);
        due += period_h;
    }
    out
}

fn first_nonzero_sample(audio: &[i16]) -> Option<usize> {
    (0..audio.len() / 2).find(|&i| audio[2 * i] != 0 || audio[2 * i + 1] != 0)
}

fn is_silent(audio: &[i16]) -> bool {
    audio.iter().all(|&s| s == 0)
}

/// 1. `timer(0, 96, fn)`: period_h = 96*8 = 768 -> first fire at sample
/// 768/2 = 384, and since every `due_h` after that increases by the same
/// period_h, the sample gap between consecutive fires is always exactly
/// period_h/2 = 384.
#[test]
fn timer0_fires_every_384_samples_across_frames() {
    let mut e = LuaEngine::new();
    e.set_source(
        "ticks = {}\n\
         timer(0, 96, function(off) ticks[#ticks + 1] = off end)\n\
         function frame(t, f)\n\
           vram[0] = #ticks\n\
           for i = 1, #ticks do vram[i] = ticks[i] end\n\
           ticks = {}\n\
         end\n",
    )
    .unwrap();

    let mut frame_start = vec![0usize];
    let mut positions = Vec::new();
    for f in 0..601u32 {
        e.frame(0.0, f).unwrap();
        if f > 0 {
            let count = e.memory().vram[0] as usize;
            for i in 1..=count {
                let off = e.memory().vram[i] as usize;
                positions.push(frame_start[(f - 1) as usize] + off);
            }
        }
        let n = e.audio().len() / 2;
        frame_start.push(frame_start[f as usize] + n);
    }

    assert_eq!(
        positions.first().copied(),
        Some(384),
        "period_h=768 -> first fire at sample 384, got {positions:?}"
    );
    for w in positions.windows(2) {
        assert_eq!(
            w[1] - w[0],
            384,
            "consecutive gap should be exactly 384 (period_h/2), got {} -> {}",
            w[0],
            w[1]
        );
    }
    // 600 frames = 319_473 samples = 638_946 h; floor(638_946 / 768) == 831.
    assert_eq!(
        positions.len(),
        831,
        "expected 831 ticks over the first 600 frames (638_946 h / 768 h period), got {}",
        positions.len()
    );
}

/// 2. A `kon(0)` issued by a timer hook at sample offset 256 should render
/// the SAME waveform as a `kon(0)` issued at offset 0, shifted 256 samples
/// later — not the same audio patched in place.
#[test]
fn timer_hook_kon_matches_frame_zero_kon_shifted_by_its_offset() {
    let mut a = LuaEngine::new();
    a.set_source(&format!(
        "{init}\
         function frame(t, f)\n\
         {setup}\
           if f == 0 then kon(0) end\n\
         end\n",
        init = voice0_init(),
        setup = VOICE0_SETUP,
    ))
    .unwrap();
    a.frame(0.0, 0).unwrap();
    let lag = first_nonzero_sample(a.audio()).expect("kon(0) at frame 0 should render audio");

    let mut b = LuaEngine::new();
    b.set_source(&format!(
        "{init}\
         ticks_b = 0\n\
         timer(0, 8, function(off)\n\
           ticks_b = ticks_b + 1\n\
           if ticks_b % 8 == 0 then kon(0) end\n\
         end)\n\
         function frame(t, f)\n\
         {setup}\
         end\n",
        init = voice0_init(),
        setup = VOICE0_SETUP,
    ))
    .unwrap();
    b.frame(0.0, 0).unwrap();
    let lag_b = first_nonzero_sample(b.audio()).expect("the 8th tick's kon(0) should render audio");

    // timer(0, 8, ..): period_h = 8*8 = 64 -> 8th tick due_h = 512 -> off = 256.
    assert_eq!(
        lag_b,
        256 + lag,
        "timer-triggered kon at off 256 should start sounding at 256 + LAG \
         (LAG={lag}, got first-nonzero at {lag_b}, expected {})",
        256 + lag
    );

    let span = 64usize;
    assert_eq!(
        &b.audio()[2 * 256..2 * (256 + span)],
        &a.audio()[0..2 * span],
        "the timer-shifted span should be sample-identical to frame 0's span, \
         just shifted 256 samples later, not patched"
    );
}

/// 3. Shared scope + ordering: a timer hook and an `hdma` hook both read a
/// global the timer hook writes; timers must run before hdma within one
/// frame, and a frame's own `frame()` body runs before ITS OWN timer walk
/// (so it always observes the PREVIOUS frame's count).
#[test]
fn timers_share_globals_and_run_before_hdma_in_the_same_frame() {
    // timer(2, 64, ..): period_h = 64*1 = 64, due_h starts at 64.
    // Frame 0 is deterministic: n=532 samples -> span_h=1064.
    let period_h = 64u64;
    let span_h0 = 1064u64;
    let expected_ticks_frame0 = expected_offsets(period_h, span_h0).len() as u64;

    let mut e = LuaEngine::new();
    e.set_source(
        "timer(2, 64, function(off) beat = (beat or 0) + 1 end)\n\
         function frame(t, f)\n\
           WH0 = beat or 0\n\
         end\n",
    )
    .unwrap();

    let lt0 = e.frame(0.0, 0).unwrap();
    assert_eq!(
        lt0.rows[0].wh0, 0,
        "frame 0's frame() runs before frame 0's own timer walk, so beat is still nil"
    );
    assert_eq!(e.audio().len() / 2, 532, "frame 0's span is deterministic");

    let lt1 = e.frame(0.0, 1).unwrap();
    assert_eq!(
        lt1.rows[0].wh0 as u64, expected_ticks_frame0,
        "frame 1's frame() should read frame 0's tick count ({expected_ticks_frame0}, from \
         period_h={period_h} over span_h={span_h0})"
    );

    // Same program, but frame() also registers an hdma hook covering row 0
    // that reads `beat` into WH1. Timers must have already run by the time
    // it fires (same frame), while WH0 above was set before the walk.
    let mut e2 = LuaEngine::new();
    e2.set_source(
        "timer(2, 64, function(off) beat = (beat or 0) + 1 end)\n\
         function frame(t, f)\n\
           WH0 = beat or 0\n\
           hdma(0, 0, function(y) WH1 = beat or 0 end)\n\
         end\n",
    )
    .unwrap();
    let lt = e2.frame(0.0, 0).unwrap();
    assert_eq!(
        lt.rows[0].wh0, 0,
        "WH0 is set by frame() before this frame's timer walk runs"
    );
    assert_eq!(
        lt.rows[0].wh1 as u64, expected_ticks_frame0,
        "the hdma hook runs AFTER the timer walk in the same frame, so it should see \
         frame 0's full tick count ({expected_ticks_frame0})"
    );
}

/// 4. Two timers (one per rate class) interleave in time order; ties (same
/// sample offset) fire in registration order.
#[test]
fn two_timers_interleave_sorted_by_offset_ties_in_registration_order() {
    let mut e = LuaEngine::new();
    e.set_source(
        "events = {}\n\
         timer(0, 2, function(off) events[#events + 1] = 1000 + off end)\n\
         timer(2, 8, function(off) events[#events + 1] = 2000 + off end)\n\
         function frame(t, f)\n\
           vram[0] = #events\n\
           for i = 1, #events do vram[i] = events[i] end\n\
           events = {}\n\
         end\n",
    )
    .unwrap();

    e.frame(0.0, 0).unwrap();
    let n0 = e.audio().len() / 2;
    e.frame(0.0, 1).unwrap();

    let span_h = 2 * n0 as u64;
    // timer(0, 2, a): period_h = 2*8 = 16. timer(2, 8, b): period_h = 8*1 = 8.
    let mut expected: Vec<(u64, i64)> = expected_offsets(16, span_h)
        .into_iter()
        .map(|off| (off, 1000 + off as i64))
        .collect();
    expected.extend(
        expected_offsets(8, span_h)
            .into_iter()
            .map(|off| (off, 2000 + off as i64)),
    );
    // Sort by (off, code): equal off -> a's code (1000+off) sorts before
    // b's (2000+off), matching "ties fire in registration order" (a first).
    expected.sort();
    let expected_codes: Vec<i64> = expected.into_iter().map(|(_, code)| code).collect();

    let count = e.memory().vram[0] as usize;
    let actual: Vec<i64> = (1..=count).map(|i| e.memory().vram[i] as i64).collect();

    assert_eq!(
        actual, expected_codes,
        "interleaved event order should match the two timers' offsets, ties (a) before (b)"
    );
}

/// 5a. An error thrown inside a timer hook surfaces through the same
/// `LuaError` path as an hdma hook error, attributed to the hook's
/// DEFINING file (fx.lua), not the file that called `timer()`.
#[test]
fn error_inside_timer_hook_is_attributed_to_its_defining_file() {
    let mut e = LuaEngine::new();
    e.set_sources(&[
        ("fx.lua", "function tick(off) error('kaboom') end"),
        ("main.lua", "timer(0, 1, tick)\nfunction frame(t, f) end"),
    ])
    .unwrap();

    let err = e.frame(0.0, 0).unwrap_err();
    assert_eq!(err.file.as_deref(), Some("fx.lua"), "got: {err:?}");
    assert!(err.message.contains("kaboom"), "got: {}", err.message);
}

/// 5b. `timer()` may only be called from top-level code / `init()` — the
/// same init-only gate `dma()` uses, and the same error shape.
#[test]
fn timer_inside_frame_is_a_runtime_error() {
    let mut e = LuaEngine::new();
    e.set_sources(&[(
        "main.lua",
        "function frame(t, f) timer(0, 1, function(off) end) end",
    )])
    .unwrap();
    let err = e.frame(0.0, 0).unwrap_err();
    assert!(
        err.message.contains("timer runs during setup"),
        "got: {}",
        err.message
    );
}

#[test]
fn timer_inside_an_hdma_hook_is_a_runtime_error() {
    let mut e = LuaEngine::new();
    e.set_sources(&[(
        "main.lua",
        "function frame(t, f) hdma(0, 10, function(y) timer(0, 1, function(off) end) end) end",
    )])
    .unwrap();
    let err = e.frame(0.0, 0).unwrap_err();
    assert!(
        err.message.contains("timer runs during setup"),
        "got: {}",
        err.message
    );
}

#[test]
fn timer_inside_a_timer_hook_is_a_runtime_error() {
    let mut e = LuaEngine::new();
    e.set_sources(&[(
        "main.lua",
        "timer(0, 1, function(off) timer(0, 1, function(off2) end) end)\n\
         function frame(t, f) end\n",
    )])
    .unwrap();
    let err = e.frame(0.0, 0).unwrap_err();
    assert!(
        err.message.contains("timer runs during setup"),
        "got: {}",
        err.message
    );
}

/// 5c. Argument validation at registration time — each a `set_source` error,
/// with the exact `timer:` wording (mirrors `dma_setup.rs`'s error-message
/// assertions).
#[test]
fn timer_validates_arguments_at_registration() {
    let frame = "function frame(t, f) end";

    let err = LuaEngine::new()
        .set_source(&format!("timer(3, 1, function(off) end)\n{frame}"))
        .unwrap_err();
    assert!(
        err.message.contains("timer:") && err.message.contains("n must be an integer in 0..2"),
        "n=3 is out of range (0..=2), got: {}",
        err.message
    );

    let err = LuaEngine::new()
        .set_source(&format!("timer(0, 0, function(off) end)\n{frame}"))
        .unwrap_err();
    assert!(
        err.message.contains("timer:") && err.message.contains("div must be an integer in 1..255"),
        "div=0 is below the 1..=255 range, got: {}",
        err.message
    );

    let err = LuaEngine::new()
        .set_source(&format!("timer(0, 256, function(off) end)\n{frame}"))
        .unwrap_err();
    assert!(
        err.message.contains("timer:") && err.message.contains("div must be an integer in 1..255"),
        "div=256 is above the 1..=255 range, got: {}",
        err.message
    );

    let err = LuaEngine::new()
        .set_source(&format!("timer(0, 1, 42)\n{frame}"))
        .unwrap_err();
    assert!(
        err.message.contains("timer:") && err.message.contains("third argument must be a function"),
        "third argument must be a function, got: {}",
        err.message
    );
}

/// 6a. Phase resets on recompile: program A's `timer(0, 96, ..)` runs for 3
/// frames (1597 samples = 3194 h — see `timer0_fires_every_384_samples_across_frames`),
/// leaving its `due_h` partway through a 768h period (a carried phase would
/// make the next fire land well short of a fresh period). Recompiling to
/// program B, which registers the SAME `timer(0, 96, ..)`, must start that
/// registration over: `due_h = period_h` (768), so it fires once at exactly
/// `period_h / 2` = 384 in the first post-recompile span, not at the
/// leftover carried offset.
#[test]
fn recompile_resets_timer_phase() {
    let program = || {
        "ticks = {}\n\
         timer(0, 96, function(off) ticks[#ticks + 1] = off end)\n\
         function frame(t, f)\n\
           vram[0] = #ticks\n\
           for i = 1, #ticks do vram[i] = ticks[i] end\n\
           ticks = {}\n\
         end\n"
    };

    let mut e = LuaEngine::new();
    e.set_source(program()).unwrap();
    for f in 0..3u32 {
        e.frame(0.0, f).unwrap();
    }

    // Recompile with the identical registration + probe. The `frame()` body
    // reads the PREVIOUS frame's ticks (see test 3's doc comment), so the
    // first post-recompile span's fire only shows up on the SECOND
    // post-recompile frame's probe read.
    e.set_source(program()).unwrap();
    e.frame(0.0, 0).unwrap();
    e.frame(0.0, 1).unwrap();

    let count = e.memory().vram[0] as usize;
    let offsets: Vec<u16> = (1..=count).map(|i| e.memory().vram[i]).collect();
    assert_eq!(
        offsets,
        vec![384],
        "a fresh registration's due_h starts at period_h (768), so the only fire in the \
         first post-recompile span lands at exactly period_h/2 = 384 — a carried due_h from \
         the pre-recompile program would land somewhere else entirely, got {offsets:?}"
    );
}

/// 6b. Recompile drops the OLD program's timer hooks outright, not just their
/// phase: program A's `timer(0, 1, ..)` hook keys voice 0 on every tick
/// (period_h = 1*8 = 8, so it fires early and often); program B registers NO
/// timers at all and keys the voice off in its first frame. If A's hook
/// survived the recompile it would keep re-triggering `kon(0)` underneath
/// B's `koff(0)`, and the voice would never fall silent.
#[test]
fn recompile_drops_old_timer_hooks() {
    let mut e = LuaEngine::new();
    e.set_source(&format!(
        "{init}\
         timer(0, 1, function(off) kon(0) end)\n\
         function frame(t, f)\n\
         {setup}\
         end\n",
        init = voice0_init(),
        setup = VOICE0_SETUP,
    ))
    .unwrap();
    e.frame(0.0, 0).unwrap();
    assert!(
        !is_silent(e.audio()),
        "program A's timer hook should have keyed voice 0 on during its first frame"
    );

    e.set_source(&format!(
        "{init}\
         function frame(t, f)\n\
         {setup}\
           if f == 0 then koff(0) end\n\
         end\n",
        init = voice0_init(),
        setup = VOICE0_SETUP,
    ))
    .unwrap();

    e.frame(0.0, 0).unwrap();
    assert!(
        !is_silent(e.audio()),
        "koff(0)'s release ramp should still be sounding on B's first frame"
    );
    for f in 1..4u32 {
        e.frame(0.0, f).unwrap();
    }
    assert!(
        is_silent(e.audio()),
        "the release ramp should have finished silent by B's 4th frame — if A's kon(0) \
         timer hook survived the recompile it would still be keying the voice on every tick"
    );
}
