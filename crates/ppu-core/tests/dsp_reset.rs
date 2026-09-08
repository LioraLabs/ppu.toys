//! `LuaEngine::reset()` — Run (t=0): a full power cycle of the sound chip
//! (fresh DSP, zeroed ARAM incl. echo RAM, placements replayed, timer phase
//! zero), as opposed to `set_sources` (recompile: hot reload, chip keeps
//! running). Driven entirely through `LuaEngine`'s public API
//! (`add_source`/`set_source`/`set_sources`/`frame`/`audio`/`dsp_view`/
//! `reset`) — never `Dsp` internals or ARAM directly. BRR/directory poke
//! helpers copied from `dsp_timers.rs`; sine/dma placement helpers copied
//! from `dsp_placement.rs`; the `WH0 = voice[0].envx` register trick copied
//! from `dsp_dsl.rs`.
use ppu_core::{convert_sample, ConvertSampleOptions, DspSampleView, LuaEngine};

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

/// A looping 6-block BRR sample (directory entry 0, ARAM 0x0200), voice 0
/// configured every frame, `kon(0)` on frame `kon_frame`, envx published to
/// WH0 (same shape as `dsp_dsl.rs::voice0_program`).
fn voice0_program(kon_frame: i64) -> String {
    format!(
        "function init()\n{sample}{dir}end\n\n\
         function frame(t, f)\n\
           voice[0].sample = 0\n\
           voice[0].pitch = 0x1000\n\
           voice[0].vol.l = 127\n\
           voice[0].vol.r = 127\n\
           voice[0].adsr = {{a = 15, d = 0, s = 7, r = 0}}\n\
           dsp.mvol.l = 127\n\
           dsp.mvol.r = 127\n\
           if f == {kon_frame} then\n\
             kon(0)\n\
           end\n\
           WH0 = voice[0].envx\n\
         end\n",
        sample = brr_pokes(0x0200, 6, 0xc3),
        dir = dir_entry_pokes(0, 0x0200, 0x0200),
    )
}

fn is_silent(audio: &[i16]) -> bool {
    audio.iter().all(|&s| s == 0)
}

/// 1 kHz sine at 32 kHz, 640 samples (see `dsp_placement.rs::sine_640`).
fn sine_640() -> Vec<i16> {
    (0..640)
        .map(|i| {
            let phase = (i as f64) / 32.0 * std::f64::consts::TAU;
            (phase.sin() * 20000.0).round() as i16
        })
        .collect()
}

const SAMPLE_BYTES: i64 = 360;

fn add_sine(e: &mut LuaEngine, name: &str) {
    let pcm = sine_640();
    let (payload, _meta) = convert_sample(
        &pcm,
        &ConvertSampleOptions {
            loop_start: Some(0),
        },
    )
    .unwrap();
    e.add_source(name, &payload.encode()).unwrap();
}

/// Voice-0 frame body driving sample `sample_id`, keying on at `kon_frame`
/// (see `dsp_placement.rs::voice0_frame`).
fn voice0_frame(sample_id: &str, kon_frame: i64) -> String {
    format!(
        "function frame(t, f)\n\
           voice[0].sample = {sample_id}\n\
           voice[0].pitch = 0x1000\n\
           voice[0].vol.l = 127\n\
           voice[0].vol.r = 127\n\
           voice[0].adsr = {{a = 15, d = 0, s = 7, r = 0}}\n\
           dsp.mvol.l = 127\n\
           dsp.mvol.r = 127\n\
           if f == {kon_frame} then\n\
             kon(0)\n\
           end\n\
         end\n"
    )
}

/// 1. `reset()` power-cycles the DSP: a sounding voice goes silent on the
/// very next frame, and its envx (read the FOLLOWING frame, since envx
/// published each frame reflects the PREVIOUS frame's render) is zero.
#[test]
fn reset_silences_voice_and_zeroes_envx() {
    let mut e = LuaEngine::new();
    e.set_source(&voice0_program(0)).unwrap();
    e.frame(0.0, 0).unwrap(); // kon(0)
    assert!(!is_silent(e.audio()), "voice should be sounding pre-reset");

    e.reset().unwrap();

    e.frame(0.0, 1).unwrap();
    assert!(
        is_silent(e.audio()),
        "the frame right after reset() should render silence"
    );
    let lt = e.frame(0.0, 2).unwrap();
    assert_eq!(
        lt.rows[0].wh0, 0,
        "envx read after reset should be zero (fresh DSP, voice never keyed on again)"
    );
}

/// 2. `reset()` replays the `dma()`-placed sample: after reset, keying the
/// voice on again produces sound, and `dsp_view().samples` still lists the
/// same placement (id/name/start/end unchanged).
#[test]
fn reset_replays_dma_sample_placement() {
    let mut e = LuaEngine::new();
    add_sine(&mut e, "kick");
    let program = format!(
        "function init()\n\
           dma(\"kick\", {{ addr = 0x0500 }})\n\
         end\n\
         {frame}",
        frame = voice0_frame("0", 0),
    );
    e.set_source(&program).unwrap();
    e.frame(0.0, 0).unwrap();
    assert!(!is_silent(e.audio()), "voice should sound pre-reset");

    let expected = vec![DspSampleView {
        id: 0,
        name: "kick".to_string(),
        start: 0x0500,
        end: 0x0500 + SAMPLE_BYTES as u32,
    }];
    assert_eq!(e.dsp_view().samples, expected, "placement view pre-reset");

    e.reset().unwrap();
    assert_eq!(
        e.dsp_view().samples,
        expected,
        "reset() replays the placement, so dsp_view().samples is unchanged"
    );

    // Re-key: kon_frame=0 relative to the frame counter passed in, so key on
    // again on the first post-reset frame.
    e.frame(0.0, 0).unwrap();
    assert!(
        !is_silent(e.audio()),
        "re-keying voice 0 after reset should sound again, proving the BRR bytes \
         (and directory entry) were rewritten into the freshly-zeroed ARAM"
    );
}

/// 3. `reset()` restarts timer phase: an engine that ran for a while (so its
/// `timer(0, 96, ..)` hook's `due_h` carries mid-period phase), then
/// `reset()`, must observe the SAME per-frame fire pattern over N frames as
/// a brand-new engine loaded with the same program — and must NOT match
/// what its own pre-reset continuation would have produced at the same
/// frame indices (proving the reset actually zeroed phase, rather than the
/// assertion passing by coincidence).
#[test]
fn reset_restarts_timer_phase() {
    // timer(0, 96, ..): period_h = 96*8 = 768 h = 384 samples. Each frame is
    // ~532.5 samples, so consecutive frames drift against the 384-sample
    // period — phase carry is visible within a handful of frames.
    let program = || {
        "ticks = {}\n\
         timer(0, 96, function(off) ticks[#ticks + 1] = off end)\n\
         function frame(t, f)\n\
           vram[0] = #ticks\n\
           for i = 1, #ticks do vram[i] = ticks[i] end\n\
           ticks = {}\n\
         end\n"
    };

    // Walk `e` for `frames` frames (from a fresh frame counter of 0),
    // collecting the per-frame tick-offset lists (as read by the FOLLOWING
    // frame's probe, per dsp_timers.rs test 3's doc comment).
    fn collect(e: &mut LuaEngine, frames: u32) -> Vec<Vec<u16>> {
        let mut out = Vec::new();
        for f in 0..=frames {
            e.frame(0.0, f).unwrap();
            if f > 0 {
                let count = e.memory().vram[0] as usize;
                out.push((1..=count).map(|i| e.memory().vram[i]).collect());
            }
        }
        out
    }

    const WARMUP: u32 = 5;
    const N: u32 = 6;

    let mut a = LuaEngine::new();
    a.set_source(program()).unwrap();
    // Warm A up first so its timer phase carries mid-period into reset.
    for f in 0..WARMUP {
        a.frame(0.0, f).unwrap();
    }

    // What A's un-reset continuation would have observed, from a SEPARATE
    // clone-by-replay engine so we don't disturb `a`'s own state: replay the
    // same warmup on `a_continued`, then collect N frames without resetting.
    let mut a_continued = LuaEngine::new();
    a_continued.set_source(program()).unwrap();
    for f in 0..WARMUP {
        a_continued.frame(0.0, f).unwrap();
    }
    let continued = collect(&mut a_continued, N);

    a.reset().unwrap();
    let after_reset = collect(&mut a, N);

    let mut b = LuaEngine::new();
    b.set_source(program()).unwrap();
    let fresh = collect(&mut b, N);

    assert_eq!(
        after_reset, fresh,
        "reset() must restart timer phase to zero, matching a brand-new engine"
    );
    assert_ne!(
        after_reset, continued,
        "the pre-reset continuation (carried phase) must differ from the post-reset \
         run at the same frame indices, or this test can't tell reset from a no-op"
    );
}

/// 4. Recompile (`set_sources`) is NOT a reset: it keeps a sounding voice
/// sounding (existing invariant, guarded here against regression from the
/// `power_on_dsp` refactor).
#[test]
fn recompile_keeps_voice_sounding() {
    let mut e = LuaEngine::new();
    e.set_source(&voice0_program(0)).unwrap();
    e.frame(0.0, 0).unwrap(); // kon(0)
    assert!(!is_silent(e.audio()));

    e.set_source(&voice0_program(0)).unwrap(); // same program, no kon fires again
    e.frame(0.0, 1).unwrap();
    assert!(
        !is_silent(e.audio()),
        "recompile must not reset the DSP — a sounding voice should keep sounding"
    );
}

/// 5a. A failed `init()` recompile keeps `dsp_view().samples` reporting the
/// PREVIOUS program's placements — `set_sources` swaps the recorder before
/// running `init()`, so the failure path must restore the old samples
/// rather than leaving the new (empty-so-far) recorder's list.
#[test]
fn failed_init_recompile_keeps_previous_samples_view() {
    let mut e = LuaEngine::new();
    add_sine(&mut e, "kick");
    let program_a = "function init()\n\
                       dma(\"kick\", {})\n\
                     end\n\
                     function frame(t, f)\nend\n";
    e.set_source(program_a).unwrap();
    assert_eq!(e.dsp_view().samples.len(), 1, "program A placed one sample");
    let before = e.dsp_view().samples;

    let program_b = "function init()\n\
                       dma(\"kick\", {})\n\
                       error(\"boom\")\n\
                     end\n\
                     function frame(t, f)\nend\n";
    assert!(e.set_source(program_b).is_err());
    assert_eq!(
        e.dsp_view().samples,
        before,
        "a failed recompile keeps reporting the PREVIOUS program's placements, since \
         nothing new was actually written to ARAM"
    );
}

/// 5b. Same contract, but the failure comes from the post-init echo-overlap
/// re-check rather than a Lua `error()`: `init()` places a sample, then
/// pushes `dsp.echo.delay` up so the placement falls inside the (now
/// larger) echo region, which `set_sources` catches AFTER `init()` returns.
#[test]
fn failed_echo_overlap_recompile_keeps_previous_samples_view() {
    let mut e = LuaEngine::new();
    add_sine(&mut e, "kick");
    let program_a = "function init()\n\
                       dma(\"kick\", {})\n\
                     end\n\
                     function frame(t, f)\nend\n";
    e.set_source(program_a).unwrap();
    assert_eq!(e.dsp_view().samples.len(), 1, "program A placed one sample");
    let before = e.dsp_view().samples;

    // 0xf000 sits inside the echo region once delay=15 reserves the top
    // 15*0x800 = 0xa800 bytes of ARAM: [0x5800, 0x10000).
    let program_b = "function init()\n\
                       dma(\"kick\", { addr = 0xf000 })\n\
                       dsp.echo.delay = 15\n\
                     end\n\
                     function frame(t, f)\nend\n";
    let err = e.set_source(program_b).unwrap_err();
    assert!(
        err.message.contains("echo region"),
        "expected the echo-overlap re-check to fire, got: {}",
        err.message
    );
    assert_eq!(
        e.dsp_view().samples,
        before,
        "a failed recompile (echo-overlap path) keeps reporting the PREVIOUS program's \
         placements"
    );
}
