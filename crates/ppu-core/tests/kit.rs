//! The sequencer sugar prelude (`note`/`instrument`/`sfx`/`song`), embedded
//! as the `"kit"` Lua chunk and run before every user chunk (see
//! `LuaEngine::set_sources`) — driven entirely through `LuaEngine`'s public
//! API (`set_source`/`set_sources`/`frame`/`memory().vram`/`dsp_view`/
//! `audio`), never reaching into `Dsp` internals.
use ppu_core::LuaEngine;

fn run_frame0(e: &mut LuaEngine, src: &str) {
    e.set_source(src).unwrap();
    e.frame(0.0, 0).unwrap();
}

// ---- (a) note()/pitch ------------------------------------------------

/// note("A4") with the default base C4 (MIDI 60): A4 is MIDI 69, so
/// pitch = floor(4096 * 2^((69-60)/12) + 0.5). 2^0.75 = 1.681792830507429,
/// 4096 * that = 6888.653... -> +0.5 -> floor = 6889. This literal is
/// derived straight from the ratio formula (440 / 261.63 * 4096 = 6888.6).
#[test]
fn note_a4_default_base_is_440_over_261_63_ratio() {
    let mut e = LuaEngine::new();
    run_frame0(&mut e, "function frame() vram[0] = note('A4') end");
    let got = e.memory().vram[0] as i64;
    assert!(
        (got - 6889).abs() <= 1,
        "note('A4') = {got}, want 6889 +/-1"
    );
}

#[test]
fn note_69_midi_matches_a4_name() {
    let mut e = LuaEngine::new();
    run_frame0(&mut e, "function frame() vram[0] = note(69) end");
    let got = e.memory().vram[0] as i64;
    assert!((got - 6889).abs() <= 1, "note(69) = {got}, want 6889 +/-1");
}

#[test]
fn note_c4_default_base_is_unity_pitch() {
    let mut e = LuaEngine::new();
    run_frame0(&mut e, "function frame() vram[0] = note('C4') end");
    assert_eq!(e.memory().vram[0], 4096);
}

#[test]
fn note_c3_default_base_c4_is_half_pitch() {
    let mut e = LuaEngine::new();
    run_frame0(&mut e, "function frame() vram[0] = note('C3') end");
    assert_eq!(e.memory().vram[0], 2048);
}

/// note("C5", "C3") is two octaves up: ratio 2^2 = 4, 4096*4 = 16384,
/// clamped to the 14-bit max 0x3fff.
#[test]
fn note_c5_base_c3_clamps_to_14_bit_max() {
    let mut e = LuaEngine::new();
    run_frame0(&mut e, "function frame() vram[0] = note('C5', 'C3') end");
    assert_eq!(e.memory().vram[0], 0x3fff);
}

/// note("C#4") and note("Db4") are both one semitone above C4:
/// 4096 * 2^(1/12) = 4096 * 1.0594630943592953 = 4339.594... -> +0.5 ->
/// floor = 4340 (computed from the ratio formula, not the kit's own code).
#[test]
fn note_sharp_and_flat_spellings_agree() {
    let mut sharp = LuaEngine::new();
    run_frame0(&mut sharp, "function frame() vram[0] = note('C#4') end");
    let mut flat = LuaEngine::new();
    run_frame0(&mut flat, "function frame() vram[0] = note('Db4') end");

    let want = 4340i64;
    let got_sharp = sharp.memory().vram[0] as i64;
    let got_flat = flat.memory().vram[0] as i64;
    assert!(
        (got_sharp - want).abs() <= 1,
        "note('C#4') = {got_sharp}, want {want} +/-1"
    );
    assert!(
        (got_flat - want).abs() <= 1,
        "note('Db4') = {got_flat}, want {want} +/-1"
    );
    assert_eq!(got_sharp, got_flat, "C#4 and Db4 must agree");
}

#[test]
fn note_name_is_case_insensitive() {
    let mut e = LuaEngine::new();
    run_frame0(&mut e, "function frame() vram[0] = note('a4') end");
    let got = e.memory().vram[0] as i64;
    assert!(
        (got - 6889).abs() <= 1,
        "note('a4') = {got}, want 6889 +/-1"
    );
}

#[test]
fn note_bad_letter_errors_attributed_to_caller_file() {
    let mut e = LuaEngine::new();
    e.set_sources(&[("main.lua", "function frame() vram[0] = note('H9') end")])
        .unwrap();
    let err = e.frame(0.0, 0).unwrap_err();
    assert_eq!(err.file.as_deref(), Some("main.lua"), "got: {err:?}");
    assert!(
        err.message.contains("bad note 'H9'"),
        "got: {}",
        err.message
    );
}

// ---- (b) sfx() writes the voice ---------------------------------------

#[test]
fn sfx_writes_sample_vol_pitch_adsr_hard_pan_left() {
    let mut e = LuaEngine::new();
    run_frame0(
        &mut e,
        "function frame()\n\
           sfx(instrument{ sample = 3, vol = 100, pan = -1, adsr = {a = 15, d = 0, s = 7, r = 0} }, 2)\n\
         end\n",
    );
    let v = &e.dsp_view().voices[2];
    assert_eq!(v.sample, 3);
    assert_eq!(v.vol.l, 100);
    assert_eq!(v.vol.r, 0);
    assert_eq!(v.pitch, 0x1000);
    assert_eq!(v.adsr.a, 15);
    assert_eq!(v.adsr.s, 7);
}

#[test]
fn sfx_center_ish_pan_splits_volume() {
    let mut e = LuaEngine::new();
    run_frame0(
        &mut e,
        "function frame()\n\
           sfx(instrument{ sample = 3, vol = 100, pan = 0.5 }, 2)\n\
         end\n",
    );
    let v = &e.dsp_view().voices[2];
    assert_eq!(v.vol.l, 50);
    assert_eq!(v.vol.r, 100);
}

#[test]
fn sfx_with_note_name_sets_pitch() {
    let mut e = LuaEngine::new();
    run_frame0(
        &mut e,
        "function frame()\n\
           sfx(instrument{ sample = 3 }, 1, 'A4')\n\
         end\n",
    );
    let got = e.dsp_view().voices[1].pitch as i64;
    assert!(
        (got - 6889).abs() <= 1,
        "voice 1 pitch = {got}, want 6889 +/-1"
    );
}

/// The envelope attacks on KON regardless of what's in ARAM: an all-zero
/// sample (no BRR data poked at all — `sample = 0`'s directory entry is
/// whatever ARAM's zeroed default holds) decodes as silence, but an instant
/// attack (`a = 15`) still drives `envx` up from 0 once `sfx()`'s `kon(0)`
/// takes effect. `envx` is read-only and republished from the live DSP only
/// once a frame's audio has fully rendered (see `LuaEngine::render_frame_
/// audio`), so `sfx()` runs at f == 0 and the result is read back at f == 1.
#[test]
fn sfx_kons_the_voice_without_any_brr_sample_data() {
    let mut e = LuaEngine::new();
    e.set_source(
        "function frame(t, f)\n\
           if f == 0 then\n\
             sfx(instrument{ sample = 0, adsr = {a = 15, d = 0, s = 7, r = 0} }, 0)\n\
           end\n\
         end\n",
    )
    .unwrap();
    e.frame(0.0, 0).unwrap();
    e.frame(0.0, 1).unwrap();
    let envx = e.dsp_view().voices[0].envx;
    assert!(
        envx > 0,
        "kon(0) should have started voice 0's envelope attacking, got envx={envx}"
    );
}

// ---- (c) instrument() validation --------------------------------------

#[test]
fn instrument_without_sample_errors() {
    let mut e = LuaEngine::new();
    e.set_source("function frame() instrument{} end").unwrap();
    let err = e.frame(0.0, 0).unwrap_err();
    assert!(
        err.message.contains("instrument: sample is required"),
        "got: {}",
        err.message
    );
}

// ---- (d)/(e) prelude presence + shadowing -----------------------------

#[test]
fn user_global_note_shadows_the_kit() {
    let mut e = LuaEngine::new();
    e.set_sources(&[(
        "main.lua",
        "function note(x) return 42 end\nfunction frame() vram[0] = note('C4') end",
    )])
    .unwrap();
    e.frame(0.0, 0).unwrap();
    assert_eq!(e.memory().vram[0], 42);
}

/// The shadow must reach even the kit's OWN internal call sites: `sfx`
/// resolves `note` as a global at call time (never a captured local), so a
/// user override changes what `sfx(..., "C4")` computes for `voice[].pitch`
/// too. This also can't pass without the kit existing at all (unlike the
/// simple shadow test above): `instrument`/`sfx`/`voice` are kit surface.
#[test]
fn user_global_note_shadows_the_kit_even_inside_sfx() {
    let mut e = LuaEngine::new();
    e.set_sources(&[(
        "main.lua",
        "function note(x, base) return 42 end\n\
         function frame()\n\
           sfx(instrument{ sample = 0 }, 0, 'C4')\n\
           vram[0] = voice[0].pitch\n\
         end\n",
    )])
    .unwrap();
    e.frame(0.0, 0).unwrap();
    assert_eq!(
        e.memory().vram[0],
        42,
        "sfx's internal note(n, inst.base) call must resolve the user's override"
    );
}

#[test]
fn kit_globals_are_present_before_any_user_chunk_runs() {
    let mut e = LuaEngine::new();
    e.set_source(
        "function frame()\n\
           vram[0] = type(note) == 'function' and 1 or 0\n\
           vram[1] = type(instrument) == 'function' and 1 or 0\n\
           vram[2] = type(sfx) == 'function' and 1 or 0\n\
         end\n",
    )
    .unwrap();
    e.frame(0.0, 0).unwrap();
    assert_eq!(e.memory().vram[0], 1);
    assert_eq!(e.memory().vram[1], 1);
    assert_eq!(e.memory().vram[2], 1);
}

#[test]
fn kit_prelude_loads_with_no_user_chunk_at_all() {
    assert!(LuaEngine::new().set_sources(&[]).is_ok());
}

// ---- (f) sfx() copies the preset's adsr table, never aliases it -------

#[test]
fn sfx_copies_adsr_fields_never_aliases_the_preset() {
    let mut e = LuaEngine::new();
    e.set_source(
        "local i = instrument{sample = 0, adsr = {a = 1, d = 2, s = 3, r = 4}}\n\
         function frame()\n\
           sfx(i, 0)\n\
           voice[0].adsr.a = 9\n\
           vram[0] = i.adsr.a\n\
         end\n",
    )
    .unwrap();
    e.frame(0.0, 0).unwrap();
    assert_eq!(
        e.memory().vram[0],
        1,
        "mutating voice[0].adsr must not affect the preset's own adsr table"
    );
}

// ---- (fix 1) sfx() clears a stale GAIN-mode value when a later preset is
// ADSR-mode with no gain of its own -----------------------------------

#[test]
fn sfx_adsr_preset_clears_a_stale_gain_value() {
    let mut e = LuaEngine::new();
    e.set_source(
        "function frame()\n\
           sfx(instrument{ sample = 0, gain = 100 }, 0)\n\
           sfx(instrument{ sample = 0, adsr = {a = 15, d = 0, s = 7, r = 0} }, 0)\n\
           vram[0] = voice[0].gain == nil and 1 or 0\n\
         end\n",
    )
    .unwrap();
    e.frame(0.0, 0).unwrap();
    assert_eq!(
        e.memory().vram[0],
        1,
        "an adsr preset with no gain of its own must clear the previous GAIN-mode value"
    );
}

// ---- song{} -----------------------------------------------------------
//
// Every test here drives the timer/kon/koff seam described in the ticket:
// a user chunk shadows the three globals `song{}`'s hook calls at runtime
// (`timer` to record the current hook offset + count registrations,
// `kon`/`koff` to log `{off, voice}`), then forwards to the real ones.
// `frame()` copies whatever's in the log tables into `vram[]`; since
// `LuaEngine::frame` runs the Lua `frame()` function BEFORE walking this
// frame's timer hooks (see `dsp_timers.rs::timer0_fires_every_384_samples_
// across_frames`, the established precedent for this lag), a log entry
// added by a hook that fired during call `f` only shows up in `vram[]`
// when read after call `f + 1`; absolute sample position is therefore
// `frame_start[f] + off`, accumulated the same way as that file.

/// `adsr={a=15,d=0,s=7,r=0}` (instant attack, no decay, full sustain, no
/// release) — irrelevant to `song{}` itself, just an inst preset shared
/// across these tests. Keying on with no real BRR sample never errors
/// regardless of ADSR shape (see `sfx_kons_the_voice_without_any_brr_
/// sample_data`); this preset is only here to keep envelope levels
/// predictable where a test happens to look at them.
const FAST_ADSR: &str = "adsr = {a = 15, d = 0, s = 7, r = 0}";

/// AC1 (grid + pitch) and (g) (polymeter): a two-track song, 480 frames
/// (>= 4 bars of 16 sixteenths at 120 BPM: 4 bars * 16 steps = 64 steps;
/// 64 steps / 8 steps-per-second = 8s; 8s * 60.0988 fps ~= 481 frames, so
/// 480 frames covers the 4 bars with a hair of margin).
#[test]
fn song_ac1_grid_pitch_and_polymeter() {
    let mut e = LuaEngine::new();
    e.set_source(&format!(
        "local real_timer = timer\n\
         timer_count = 0\n\
         function timer(n, div, fn)\n\
           timer_count = timer_count + 1\n\
           real_timer(n, div, function(off)\n\
             __cur_off = off\n\
             fn(off)\n\
           end)\n\
         end\n\
         local real_kon = kon\n\
         kon_log = {{}}\n\
         function kon(v)\n\
           kon_log[#kon_log + 1] = {{ off = __cur_off, voice = v }}\n\
           real_kon(v)\n\
         end\n\
         local real_koff = koff\n\
         koff_log = {{}}\n\
         function koff(v)\n\
           koff_log[#koff_log + 1] = {{ off = __cur_off, voice = v }}\n\
           real_koff(v)\n\
         end\n\
         bass = instrument{{ sample = 0, {adsr} }}\n\
         lead = instrument{{ sample = 0, {adsr} }}\n\
         h = song{{ tempo = 120, steps = 16, tracks = {{\n\
           {{ voice = 0, inst = bass, pattern = 'C2 . . G2 . . C3 .' }},\n\
           {{ voice = 1, inst = lead, pattern = 'C4 ^ C4 ^' }},\n\
         }} }}\n\
         function frame(t, f)\n\
           vram[0] = #kon_log\n\
           for i = 1, #kon_log do\n\
             vram[1 + (i - 1) * 2] = kon_log[i].off\n\
             vram[2 + (i - 1) * 2] = kon_log[i].voice\n\
           end\n\
           vram[200] = #koff_log\n\
           for i = 1, #koff_log do\n\
             vram[201 + (i - 1) * 2] = koff_log[i].off\n\
             vram[202 + (i - 1) * 2] = koff_log[i].voice\n\
           end\n\
           vram[500] = timer_count\n\
           vram[501] = voice[0].pitch\n\
           kon_log = {{}}\n\
           koff_log = {{}}\n\
         end\n",
        adsr = FAST_ADSR,
    ))
    .unwrap();

    let frames = 480u32;
    let mut frame_start = vec![0usize];
    let mut kon0: Vec<usize> = Vec::new();
    let mut kon1: Vec<usize> = Vec::new();
    let mut koff1: Vec<usize> = Vec::new();
    let mut timer_count = 0i64;
    let mut pitch_at_first_kon0: Option<i64> = None;

    for f in 0..=frames {
        e.frame(0.0, f).unwrap();
        if f > 0 {
            let base = frame_start[(f - 1) as usize];
            let kon_count = e.memory().vram[0] as usize;
            for i in 0..kon_count {
                let off = e.memory().vram[1 + i * 2] as usize;
                let v = e.memory().vram[2 + i * 2];
                let pos = base + off;
                match v {
                    0 => {
                        if pitch_at_first_kon0.is_none() {
                            pitch_at_first_kon0 = Some(e.memory().vram[501] as i64);
                        }
                        kon0.push(pos);
                    }
                    1 => kon1.push(pos),
                    _ => panic!("unexpected kon voice {v}"),
                }
            }
            let koff_count = e.memory().vram[200] as usize;
            for i in 0..koff_count {
                let off = e.memory().vram[201 + i * 2] as usize;
                let v = e.memory().vram[202 + i * 2];
                if v == 1 {
                    koff1.push(base + off);
                }
            }
            timer_count = e.memory().vram[500] as i64;
        }
        let n = e.audio().len() / 2;
        frame_start.push(frame_start[f as usize] + n);
    }

    assert_eq!(timer_count, 1, "song{{}} must register exactly one timer");

    // 4000 = 32000 Hz / 8 sixteenths-per-second (120 BPM, 4/4): the sample
    // gap between consecutive 16th-note steps on the achieved grid.
    const STEP_SAMPLES: i64 = 4000;

    assert!(!kon0.is_empty(), "expected voice-0 key-ons");
    let p0 = kon0[0] as i64;
    // First fire is one timer period after compile: div=250 (tempo 120's
    // step_ticks=1000, k=4, div=floor(1000/4+0.5)=250), so due_h = period_h
    // = div * 8 = 2000 half-samples -> sample 1000.
    assert!((p0 - 1000).abs() <= 4, "p_0 = {p0}, want 1000 +/-4");

    // voice-0's pattern "C2 . . G2 . . C3 ." key-ons at local steps
    // {0, 3, 6} of every 8-token cycle.
    const V0_LOCAL_STEPS: [i64; 3] = [0, 3, 6];
    for (j, &p) in kon0.iter().enumerate() {
        let cycle = (j / 3) as i64;
        let i_step = cycle * 8 + V0_LOCAL_STEPS[j % 3];
        let want = p0 + i_step * STEP_SAMPLES;
        assert!(
            (p as i64 - want).abs() <= 4,
            "kon0[{j}] = {p}, want {want} +/-4 (i_step={i_step})"
        );
    }
    // Exact step count observable over this run: the loop makes 481
    // e.frame() calls (f=0..=480), but the one-frame read lag documented
    // above means the LAST call's own hook fires are never read back, so
    // only the first 480 calls' worth of steps are observable. The engine's
    // exact 32000/60.0988 accumulator (see `render_frame_audio`) makes the
    // cumulative sample count after 480 calls a deterministic 255,579
    // (computed once from that same accumulator, not re-derived here).
    // Steps land every STEP_SAMPLES=4000 samples starting at p0=1000, so
    // the last observable step index j satisfies 4000*j + 1000 < 255,579,
    // i.e. j <= 63 -> 64 steps observed (all 4 bars, matching the 64 steps
    // documented on this test). Voice-0 keys on 3 of every 8 steps ("C2 . .
    // G2 . . C3 ."), and 64 steps is exactly 8 full 8-step cycles, so
    // exactly 8 * 3 = 24 key-ons.
    assert_eq!(kon0.len(), 24, "expected exactly 24 voice-0 key-ons");

    // voice-1's "C4 ^ C4 ^" keys on at every even step, off at every odd
    // step (polymeter: its own 4-token cycle against the 16-step song).
    for (j, &p) in kon1.iter().enumerate() {
        let i_step = 2 * j as i64;
        let want = p0 + i_step * STEP_SAMPLES;
        assert!(
            (p as i64 - want).abs() <= 4,
            "kon1[{j}] = {p}, want {want} +/-4 (i_step={i_step})"
        );
    }
    for (j, &p) in koff1.iter().enumerate() {
        let i_step = 2 * j as i64 + 1;
        let want = p0 + i_step * STEP_SAMPLES;
        assert!(
            (p as i64 - want).abs() <= 4,
            "koff1[{j}] = {p}, want {want} +/-4 (i_step={i_step})"
        );
    }
    // Voice-1's "C4 ^ C4 ^" alternates kon/koff every step (a 2-step
    // period once "." rests are irrelevant here — there are none). Over the
    // same 64 observable steps that's 64 / 2 = 32 of each, exactly.
    assert_eq!(kon1.len(), 32, "expected exactly 32 voice-1 key-ons");
    assert_eq!(koff1.len(), 32, "expected exactly 32 voice-1 key-offs");

    // note("C2") with default base C4: C2 is 24 semitones below C4, ratio
    // 2^-2 = 0.25, 4096 * 0.25 = 1024.
    assert_eq!(
        pitch_at_first_kon0,
        Some(1024),
        "voice-0's first note (C2) should have set pitch = 1024"
    );
}

/// AC1b: `h.beat` advances 8 times/sec at 120 BPM in 4/4.
#[test]
fn song_beat_advances_8_per_second_at_120_bpm() {
    let mut e = LuaEngine::new();
    e.set_source(&format!(
        "h = song{{ tempo = 120, tracks = {{\n\
           {{ voice = 0, inst = instrument{{ sample = 0, {adsr} }}, pattern = 'C2' }},\n\
         }} }}\n\
         function frame(t, f)\n\
           vram[0] = h.beat\n\
         end\n",
        adsr = FAST_ADSR,
    ))
    .unwrap();

    let mut beat0 = 0i64;
    let mut beat600 = 0i64;
    for f in 0..=600u32 {
        e.frame(0.0, f).unwrap();
        let beat = e.memory().vram[0] as i64;
        if f == 0 {
            beat0 = beat;
        }
        if f == 600 {
            beat600 = beat;
        }
    }
    let delta = beat600 - beat0;
    // 8 * 600 / 60.0988 = 79.87 -> 79 or 80 (the exact frame the read
    // lands on shifts by at most one hook processing step either way).
    assert!(
        delta == 79 || delta == 80,
        "beat delta over 600 frames = {delta}, want 79 or 80"
    );
}

/// `h.stop()`/`h.play()` called from `frame()`.
#[test]
fn song_stop_and_play_from_frame() {
    let mut e = LuaEngine::new();
    e.set_source(&format!(
        "local real_timer = timer\n\
         function timer(n, div, fn)\n\
           real_timer(n, div, function(off)\n\
             __cur_off = off\n\
             fn(off)\n\
           end)\n\
         end\n\
         local real_kon = kon\n\
         kon_log = {{}}\n\
         function kon(v)\n\
           kon_log[#kon_log + 1] = v\n\
           real_kon(v)\n\
         end\n\
         local real_koff = koff\n\
         koff_log = {{}}\n\
         function koff(v)\n\
           koff_log[#koff_log + 1] = v\n\
           real_koff(v)\n\
         end\n\
         h = song{{ tempo = 120, tracks = {{\n\
           {{ voice = 0, inst = instrument{{ sample = 0, {adsr} }}, pattern = 'C2' }},\n\
           {{ voice = 1, inst = instrument{{ sample = 0, {adsr} }}, pattern = 'C4' }},\n\
         }} }}\n\
         function frame(t, f)\n\
           if f == 30 then h.stop() end\n\
           if f == 60 then h.play() end\n\
           vram[0] = #kon_log\n\
           for i = 1, #kon_log do vram[i] = kon_log[i] end\n\
           vram[100] = #koff_log\n\
           for i = 1, #koff_log do vram[100 + i] = koff_log[i] end\n\
           kon_log = {{}}\n\
           koff_log = {{}}\n\
         end\n",
        adsr = FAST_ADSR,
    ))
    .unwrap();

    let mut koff_at_30: Vec<u16> = Vec::new();
    let mut kon_after_stop = 0usize;
    let mut kon_after_resume = 0usize;
    for f in 0..=90u32 {
        e.frame(0.0, f).unwrap();
        let kon_count = e.memory().vram[0] as usize;
        if f == 30 {
            let koff_count = e.memory().vram[100] as usize;
            koff_at_30 = (0..koff_count)
                .map(|i| e.memory().vram[100 + 1 + i])
                .collect();
        }
        // Hook fires from calls >= 30 (playing=false, set before that
        // call's own hook runs) are read one frame later, at f in
        // 31..=60: no kon should land there.
        if (31..=60).contains(&f) {
            kon_after_stop += kon_count;
        }
        // Hook fires from calls >= 60 (playing=true again) are read at
        // f in 61..=90: kon should resume.
        if (61..=90).contains(&f) {
            kon_after_resume += kon_count;
        }
    }

    assert!(
        koff_at_30.contains(&0) && koff_at_30.contains(&1),
        "h.stop() should koff both voices, got {koff_at_30:?}"
    );
    assert_eq!(kon_after_stop, 0, "no kon should be logged while stopped");
    assert!(
        kon_after_resume > 0,
        "kon should resume once h.play() is called"
    );
}

/// Timer-0 divider choice: tempo 120 divides its 16th-note grid exactly
/// (step_ticks = 8000*60/(120*4) = 1000, k = ceil(1000/255) = 4, div =
/// floor(1000/4 + 0.5) = 250, rate = 8000/(250*4) = 8 exactly). Tempo 128
/// does not (step_ticks = 937.5, k = 4, div = floor(234.375 + 0.5) = 234,
/// rate = 8000/936 = 8.547..., within 0.2% of 128*4/60 = 8.5333).
#[test]
fn song_div_and_rate_for_two_tempos() {
    let src = |tempo: i64| {
        format!(
            "h = song{{ tempo = {tempo}, tracks = {{\n\
               {{ voice = 0, inst = instrument{{ sample = 0 }}, pattern = 'C2' }},\n\
             }} }}\n\
             function frame(t, f)\n\
               vram[0] = h.div\n\
               vram[1] = math.floor(h.rate * 1000)\n\
             end\n"
        )
    };

    let mut e120 = LuaEngine::new();
    e120.set_source(&src(120)).unwrap();
    e120.frame(0.0, 0).unwrap();
    assert_eq!(e120.memory().vram[0], 250, "div at tempo 120");
    assert_eq!(e120.memory().vram[1], 8000, "rate*1000 at tempo 120");

    let mut e128 = LuaEngine::new();
    e128.set_source(&src(128)).unwrap();
    e128.frame(0.0, 0).unwrap();
    assert_eq!(e128.memory().vram[0], 234, "div at tempo 128");
    let rate = e128.memory().vram[1] as f64 / 1000.0;
    let target = 8.5333;
    assert!(
        (rate - target).abs() / target < 0.002,
        "rate = {rate}, want within 0.2% of {target}"
    );
}

/// A runtime error inside a shadowed `kon()` — called from `song{}`'s own
/// timer hook — is attributed to the hook's DEFINING chunk, `"kit"` (the
/// hook closure is created inside `song()`, itself defined in kit.lua),
/// even though the error actually originates in the user's `kon`
/// override. Matches the established precedent in
/// `dsp_timers.rs::error_inside_timer_hook_is_attributed_to_its_defining_file`.
#[test]
fn song_hook_error_reports_kit_as_the_defining_file() {
    let mut e = LuaEngine::new();
    e.set_sources(&[(
        "main.lua",
        &format!(
            "function kon() error('boom') end\n\
             song{{ tempo = 120, tracks = {{\n\
               {{ voice = 0, inst = instrument{{ sample = 0, {adsr} }}, pattern = 'C2' }},\n\
             }} }}\n\
             function frame(t, f) end\n",
            adsr = FAST_ADSR,
        ),
    )])
    .unwrap();

    let mut err = None;
    for f in 0..200u32 {
        if let Err(e2) = e.frame(0.0, f) {
            err = Some(e2);
            break;
        }
    }
    let err = err.expect("expected a runtime error once the first step fires");
    assert_eq!(err.file.as_deref(), Some("kit"), "got: {err:?}");
    assert!(err.message.contains("boom"), "got: {}", err.message);
}

// ---- song{} setup-time validation --------------------------------------

#[test]
fn song_bad_token_errors_with_its_track_index() {
    let mut e = LuaEngine::new();
    let err = e
        .set_source(
            "song{ tempo = 120, tracks = {\n\
               { voice = 0, inst = instrument{ sample = 0 }, pattern = 'C4 Q9' },\n\
             } }\n",
        )
        .unwrap_err();
    assert!(
        err.message.contains("song: bad token 'Q9' in track 1"),
        "got: {}",
        err.message
    );
}

#[test]
fn song_missing_tempo_errors() {
    let mut e = LuaEngine::new();
    let err = e.set_source("song{ tracks = {} }\n").unwrap_err();
    assert!(
        err.message.contains("song: tempo must be > 0"),
        "got: {}",
        err.message
    );
}

#[test]
fn song_track_missing_inst_errors() {
    let mut e = LuaEngine::new();
    let err = e
        .set_source(
            "song{ tempo = 120, tracks = {\n\
               { voice = 0, pattern = 'C4' },\n\
             } }\n",
        )
        .unwrap_err();
    assert!(
        err.message.contains("song: track 1 needs voice and inst"),
        "got: {}",
        err.message
    );
}

// A user fault in song{} must fail in the user's chunk, at song{} time,
// with a `song:` prefix — never later inside the kit's timer hook.

#[test]
fn song_voice_too_high_errors_in_user_file() {
    let mut e = LuaEngine::new();
    let err = e
        .set_source(
            "song{ tempo = 120, tracks = {\n\
               { voice = 8, inst = instrument{ sample = 0 }, pattern = 'C4' },\n\
             } }\n",
        )
        .unwrap_err();
    assert_eq!(err.file.as_deref(), Some("source"), "got: {err:?}");
    assert!(
        err.message.contains("song: track 1 voice must be 0..7"),
        "got: {}",
        err.message
    );
}

#[test]
fn song_voice_negative_errors_in_user_file() {
    let mut e = LuaEngine::new();
    let err = e
        .set_source(
            "song{ tempo = 120, tracks = {\n\
               { voice = -1, inst = instrument{ sample = 0 }, pattern = 'C4' },\n\
             } }\n",
        )
        .unwrap_err();
    assert_eq!(err.file.as_deref(), Some("source"), "got: {err:?}");
    assert!(
        err.message.contains("song: track 1 voice must be 0..7"),
        "got: {}",
        err.message
    );
}

#[test]
fn song_empty_pattern_errors_in_user_file() {
    let mut e = LuaEngine::new();
    let err = e
        .set_source(
            "song{ tempo = 120, tracks = {\n\
               { voice = 0, inst = instrument{ sample = 0 }, pattern = '' },\n\
             } }\n",
        )
        .unwrap_err();
    assert_eq!(err.file.as_deref(), Some("source"), "got: {err:?}");
    assert!(
        err.message
            .contains("song: track 1 pattern must have at least one token"),
        "got: {}",
        err.message
    );
}

#[test]
fn song_whitespace_only_pattern_errors_in_user_file() {
    let mut e = LuaEngine::new();
    let err = e
        .set_source(
            "song{ tempo = 120, tracks = {\n\
               { voice = 0, inst = instrument{ sample = 0 }, pattern = '   ' },\n\
             } }\n",
        )
        .unwrap_err();
    assert_eq!(err.file.as_deref(), Some("source"), "got: {err:?}");
    assert!(
        err.message
            .contains("song: track 1 pattern must have at least one token"),
        "got: {}",
        err.message
    );
}

#[test]
fn song_nil_tracks_errors_in_user_file() {
    let mut e = LuaEngine::new();
    let err = e.set_source("song{ tempo = 120 }\n").unwrap_err();
    assert_eq!(err.file.as_deref(), Some("source"), "got: {err:?}");
    assert!(
        err.message.contains("song: tracks must be a table"),
        "got: {}",
        err.message
    );
}

#[test]
fn song_nil_pattern_errors_in_user_file() {
    let mut e = LuaEngine::new();
    let err = e
        .set_source(
            "song{ tempo = 120, tracks = {\n\
               { voice = 0, inst = instrument{ sample = 0 } },\n\
             } }\n",
        )
        .unwrap_err();
    assert_eq!(err.file.as_deref(), Some("source"), "got: {err:?}");
    assert!(
        err.message
            .contains("song: track 1 pattern must have at least one token"),
        "got: {}",
        err.message
    );
}

#[test]
fn song_empty_inst_errors_in_user_file() {
    let mut e = LuaEngine::new();
    let err = e
        .set_source(
            "song{ tempo = 120, tracks = {\n\
               { voice = 0, inst = {}, pattern = 'C4' },\n\
             } }\n",
        )
        .unwrap_err();
    assert_eq!(err.file.as_deref(), Some("source"), "got: {err:?}");
    assert!(
        err.message.contains("song: track 1 needs voice and inst"),
        "got: {}",
        err.message
    );
}

/// (h) `h.step` never leaves 0..15 and wraps back to 0 after 15.
#[test]
fn song_step_wraps_after_15() {
    let mut e = LuaEngine::new();
    e.set_source(&format!(
        "h = song{{ tempo = 120, tracks = {{\n\
           {{ voice = 0, inst = instrument{{ sample = 0, {adsr} }}, pattern = 'C2' }},\n\
         }} }}\n\
         function frame(t, f)\n\
           vram[0] = h.step\n\
         end\n",
        adsr = FAST_ADSR,
    ))
    .unwrap();

    let mut saw_wrap = false;
    let mut prev = -1i64;
    for f in 0..400u32 {
        e.frame(0.0, f).unwrap();
        let step = e.memory().vram[0] as i64;
        assert!(
            (0..=15).contains(&step),
            "h.step = {step} out of range at f={f}"
        );
        if prev == 15 && step == 0 {
            saw_wrap = true;
        }
        prev = step;
    }
    assert!(
        saw_wrap,
        "h.step never wrapped from 15 back to 0 over 400 frames"
    );
}

/// Run (`reset()`) re-runs the kit prelude and re-registers `song{}`'s
/// timer at phase zero: the first step's key-on lands at absolute sample
/// 1000 (one timer-0 period at div 250) on a warmed-then-reset engine,
/// exactly as on a fresh one.
#[test]
fn song_first_step_offset_repeats_after_reset() {
    const PROGRAM: &str = "local real_timer = timer\n\
         function timer(n, div, fn)\n\
           real_timer(n, div, function(off)\n\
             __cur_off = off\n\
             fn(off)\n\
           end)\n\
         end\n\
         local real_kon = kon\n\
         first_kon = nil\n\
         function kon(v)\n\
           first_kon = first_kon or __cur_off\n\
           real_kon(v)\n\
         end\n\
         song{ tempo = 120, tracks = {\n\
           { voice = 0, inst = instrument{ sample = 0 }, pattern = 'C2' },\n\
         } }\n\
         function frame(t, f)\n\
           vram[0] = first_kon and 1 or 0\n\
           vram[1] = first_kon or 0\n\
         end\n";

    // Absolute sample of the first key-on, read back through the one-frame
    // lag (a hook fire during call f shows up in vram after call f + 1).
    fn first_step(e: &mut LuaEngine) -> usize {
        let mut frame_start = vec![0usize];
        for f in 0..4usize {
            e.frame(0.0, f as u32).unwrap();
            if e.memory().vram[0] == 1 {
                return frame_start[f - 1] + e.memory().vram[1] as usize;
            }
            frame_start.push(frame_start[f] + e.audio().len() / 2);
        }
        panic!("song never keyed on within 4 frames");
    }

    let mut fresh = LuaEngine::new();
    fresh.set_source(PROGRAM).unwrap();
    let want = first_step(&mut fresh);
    assert_eq!(want, 1000, "fresh engine: first step at sample 1000");

    let mut e = LuaEngine::new();
    e.set_source(PROGRAM).unwrap();
    for f in 0..7u32 {
        e.frame(0.0, f).unwrap();
    }
    e.reset().unwrap();
    assert_eq!(
        first_step(&mut e),
        want,
        "after reset() the first song step must land at the same sample as on a fresh engine"
    );
}
