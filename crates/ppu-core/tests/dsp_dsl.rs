//! `voice[]`/`dsp`/`aram[]` DSL + per-frame audio render — driven entirely
//! through `LuaEngine`'s public API (never reaching into `Dsp` internals).
//! BRR bytes/directory layout mirror `golden_dsp.rs::build_sample`/
//! `write_dir_entry`, but poked from Lua via `aram[addr] = byte`.
use ppu_core::LuaEngine;

/// Lua statements poking one BRR sample (`blocks` 9-byte blocks, filter 0,
/// shift 12, alternating 0x7/0x9 nibbles — same shape as the golden fixture)
/// starting at `base`. The last block uses `last_header` (0xc3 = loop+end,
/// 0xc1 = end without loop).
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

/// Lua statements poking sample directory entry `index` at the fixed
/// directory page `0x0100 + index*4` (the DSL never exposes DIR; the engine
/// pins it at 0x01): little-endian start, then little-endian loop address.
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

/// A program that pokes a 6-block BRR sample (directory entry 0, `0x0200`)
/// in `init()`, configures voice 0 every frame (ADSR fields match
/// `golden_dsp.rs::setup_voice0`'s known-good 0x8f/0xe0), and calls
/// `kon(0)` (possibly more than once) on frame number `kon_frame`. Also
/// publishes `voice[0].envx`/`.ended` into WH0/WH1 so the test can observe
/// them through the returned `LineTable` without reaching into `Dsp`.
fn voice0_program(last_header: u8, kon_frame: i64, kon_calls: u32) -> String {
    let kons = "kon(0)\n".repeat(kon_calls as usize);
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
             {kons}\
           end\n\
           WH0 = voice[0].envx\n\
           WH1 = (voice[0].ended and 1) or 0\n\
         end\n",
        sample = brr_pokes(0x0200, 6, last_header),
        dir = dir_entry_pokes(0, 0x0200, 0x0200),
    )
}

fn is_silent(audio: &[i16]) -> bool {
    audio.iter().all(|&s| s == 0)
}

#[test]
fn silent_before_kon_nonsilent_on_kon_frame() {
    let mut e = LuaEngine::new();
    e.set_source(&voice0_program(0xc3, 1, 1)).unwrap();

    e.frame(0.0, 0).unwrap();
    assert!(is_silent(e.audio()), "no kon yet: frame should be silent");

    e.frame(0.0, 1).unwrap();
    assert!(
        !is_silent(e.audio()),
        "kon(0) frame should render non-silent audio"
    );
}

#[test]
fn kon_twice_same_frame_matches_kon_once() {
    let mut once = LuaEngine::new();
    once.set_source(&voice0_program(0xc3, 0, 1)).unwrap();
    once.frame(0.0, 0).unwrap();

    let mut twice = LuaEngine::new();
    twice.set_source(&voice0_program(0xc3, 0, 2)).unwrap();
    twice.frame(0.0, 0).unwrap();

    assert!(!is_silent(once.audio()));
    assert_eq!(
        once.audio(),
        twice.audio(),
        "kon(0) twice in one frame must equal kon(0) once"
    );
}

#[test]
fn envx_nonzero_while_sustaining_in_next_frame() {
    let mut e = LuaEngine::new();
    // Looping sample (0xc3): the note never ends, so it's still sustaining
    // whenever we check.
    e.set_source(&voice0_program(0xc3, 0, 1)).unwrap();

    e.frame(0.0, 0).unwrap(); // kon(0) here
    let lt = e.frame(0.0, 1).unwrap(); // reads voice[0].envx from frame 0
    assert_ne!(
        lt.rows[0].wh0, 0,
        "envx read in the NEXT frame() should be non-zero while sustaining"
    );
}

#[test]
fn ended_true_after_nonlooping_sample_finishes() {
    let mut e = LuaEngine::new();
    // Non-looping sample (0xc1): 6 blocks * 16 samples/block = 96 samples,
    // well under one ~532-sample frame at pitch 0x1000 (1:1).
    e.set_source(&voice0_program(0xc1, 0, 1)).unwrap();

    e.frame(0.0, 0).unwrap(); // kon(0) here; sample plays out within this frame
    let lt = e.frame(0.0, 1).unwrap(); // reads voice[0].ended from frame 0
    assert_eq!(
        lt.rows[0].wh1, 1,
        "ended should be true once the non-looping sample finishes"
    );
}

#[test]
fn recompile_keeps_sounding_voice_sounding() {
    let mut e = LuaEngine::new();
    e.set_source(&voice0_program(0xc3, 0, 1)).unwrap();
    e.frame(0.0, 0).unwrap(); // kon(0)
    assert!(!is_silent(e.audio()));

    // Recompile with a program that does NOT touch voice[] at all.
    e.set_source("function frame(t, f)\nend\n").unwrap();
    e.frame(0.0, 1).unwrap();
    assert!(
        !is_silent(e.audio()),
        "a recompile that never touches voice[] should keep a sounding voice sounding"
    );
}

#[test]
fn fresh_engine_first_frame_is_silent_with_expected_span_length() {
    let mut e = LuaEngine::new();
    assert!(e.audio().is_empty(), "no audio before the first frame()");

    e.set_source("function frame(t, f)\nend\n").unwrap();
    e.frame(0.0, 0).unwrap();
    assert!(
        is_silent(e.audio()),
        "a no-op frame() should render silence"
    );
    let len = e.audio().len();
    assert!(
        len == 532 * 2 || len == 533 * 2,
        "frame span should be 532 or 533 stereo samples, got {}",
        len / 2
    );
}

#[test]
fn sample_count_exact_over_600_frames() {
    let mut e = LuaEngine::new();
    e.set_source("function frame(t, f)\nend\n").unwrap();

    let mut total = 0usize;
    for f in 0..600 {
        e.frame(0.0, f).unwrap();
        let n = e.audio().len() / 2;
        if f == 0 {
            assert_eq!(n, 532, "first frame span is deterministic");
        } else {
            assert!(n == 532 || n == 533, "frame {f} span was {n}");
        }
        total += n;
    }
    // 600 * 32000/60.0988 == 319473.9...; the accumulator floors each frame,
    // and the spec's "round +/- 1" admits 319473.
    assert_eq!(total, 319_473);
}

#[test]
fn dsp_view_reflects_lua_set_fields() {
    let mut e = LuaEngine::new();
    e.set_source(
        "function frame(t, f)\n\
           voice[0].sample = 5\n\
           voice[0].pitch = 0x2345\n\
           voice[0].vol.l = -10\n\
           voice[0].vol.r = 20\n\
           voice[0].adsr = {a = 1, d = 2, s = 3, r = 4}\n\
           voice[0].noise = true\n\
           voice[0].pmod = false\n\
           voice[0].echo = true\n\
           voice[1].gain = 33\n\
           dsp.mvol.l = 100\n\
           dsp.mvol.r = -50\n\
           dsp.evol.l = 10\n\
           dsp.evol.r = -10\n\
           dsp.echo.feedback = -20\n\
           dsp.echo.fir = {1, -2, 3, -4, 5, -6, 7, -8}\n\
           dsp.echo.delay = 5\n\
           dsp.noise_clock = 12\n\
           dsp.mute = true\n\
         end\n",
    )
    .unwrap();
    e.frame(0.0, 0).unwrap();

    let view = e.dsp_view();
    let v0 = &view.voices[0];
    assert_eq!(v0.sample, 5);
    assert_eq!(v0.pitch, 0x2345);
    assert_eq!(v0.vol.l, -10);
    assert_eq!(v0.vol.r, 20);
    assert_eq!(v0.adsr.a, 1);
    assert_eq!(v0.adsr.d, 2);
    assert_eq!(v0.adsr.s, 3);
    assert_eq!(v0.adsr.r, 4);
    assert_eq!(v0.gain, None, "adsr set without gain should stay ADSR mode");
    assert!(v0.noise);
    assert!(!v0.pmod);
    assert!(v0.echo);

    assert_eq!(
        view.voices[1].gain,
        Some(33),
        "gain non-nil selects GAIN mode"
    );

    assert_eq!(view.mvol.l, 100);
    assert_eq!(view.mvol.r, -50);
    assert_eq!(view.evol.l, 10);
    assert_eq!(view.evol.r, -10);
    assert_eq!(view.echo.feedback, -20);
    assert_eq!(view.echo.fir, [1, -2, 3, -4, 5, -6, 7, -8]);
    assert_eq!(view.echo.delay, 5);
    assert_eq!(view.noise_clock, 12);
    assert!(view.mute);
}

#[test]
fn dsp_echo_esa_and_flg_from_delay() {
    // ESA (echo RAM start) sits at the TOP of ARAM: 0x10000 - delay*0x800.
    let mut e = LuaEngine::new();
    e.set_source("function frame(t, f)\ndsp.echo.delay = 2\nend\n")
        .unwrap();
    e.frame(0.0, 0).unwrap();
    assert_eq!(e.dsp().read(0x6d), 0xf0, "ESA for delay=2");
    assert_eq!(e.dsp().read(0x6c) & 0x20, 0, "echo writes on for delay!=0");

    let mut e0 = LuaEngine::new();
    e0.set_source("function frame(t, f)\ndsp.echo.delay = 0\nend\n")
        .unwrap();
    e0.frame(0.0, 0).unwrap();
    assert_eq!(e0.dsp().read(0x6d), 0xff, "ESA sentinel for delay=0");
    assert_ne!(e0.dsp().read(0x6c) & 0x20, 0, "echo writes off for delay=0");
}
