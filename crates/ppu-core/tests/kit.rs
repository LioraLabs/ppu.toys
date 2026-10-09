//! The sequencer sugar prelude (`note`/`instrument`/`sfx`/`score`), embedded
//! as the `"kit"` Lua chunk and run before every user chunk (see
//! `LuaEngine::set_sources`) — driven entirely through `LuaEngine`'s public
//! API (`set_source`/`set_sources`/`frame`/`memory().vram`/`dsp_view`/
//! `audio`), never reaching into `Dsp` internals.
mod common;

use ppu_core::song::{Note, Pattern, Row, Song};
use ppu_core::LuaEngine;
use serde_json::Value;

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

// ---- song{} is gone ----------------------------------------------------

#[test]
fn song_call_errors_pointing_at_score() {
    let mut e = LuaEngine::new();
    let err = e
        .set_source("song{ tempo = 120, tracks = {} }\n")
        .unwrap_err();
    assert_eq!(err.file.as_deref(), Some("source"), "got: {err:?}");
    assert!(
        err.message.contains("score{ song = \"<name>\" }"),
        "got: {}",
        err.message
    );
}

// ---- score{} ----------------------------------------------------------

/// Mixed-tempo (125/150 BPM), odd-length (44-unit) B pattern, 50% swing,
/// arrangement A,B,A: exercises every knob the song timing's `tick_in_slot`
/// formula (song.rs) has, for `score_view_reports_a_song_sources_slot_and_step`.
fn slot_step_song() -> Song {
    let note = |at| Note {
        at,
        row: 0,
        len: 12,
        vel: 100,
        voice: None,
        nudge: 0,
        end_nudge: 0,
    };
    Song {
        tempo: 12000,
        swing: 50,
        key: 0,
        voice_mask: 0xff,
        rows: vec![Row {
            sound: "kick".into(),
            note: None,
            vol: 127,
            pan: 0,
        }],
        patterns: vec![
            Pattern {
                name: "A".into(),
                length: 48,
                tempo: Some(12500), // 125 BPM
                notes: vec![note(0)],
            },
            Pattern {
                name: "B".into(),
                length: 44, // odd: not a multiple of 12 (ceil(44/12) = 4 steps, last short)
                tempo: Some(15000), // 150 BPM
                notes: vec![note(0)],
            },
        ],
        arrangement: vec![0, 1, 0],
        loop_start: 0,
    }
}

/// Each entry is (arrangement slot, tick the global 16th step starts at),
/// hand-derived from the song timing's `tick_in_slot` formula (song.rs)
/// (`3750 / bpm` ticks per 16th; a pair's odd 16th warps by `+swing`, its
/// even one is untouched, only when the WHOLE pair fits inside the pattern):
///
/// Slot 0 ("A", 125 BPM -> 30 ticks/16th, run starts at tick 0): every one
/// of its two pairs (units 0..24, 24..48) fits inside the 48-unit pattern,
/// so both odd steps (k=1,3) warp by +0.5 steps: k0=0,
/// k1=floor(1.5*30+0.5)=45, k2=floor(2*30+0.5)=60,
/// k3=floor(3.5*30+0.5)=105, end(k4)=floor(4*30+0.5)=120.
/// Slot 1 ("B", 150 BPM -> 25 ticks/16th, run starts at tick 120, the
/// previous slot's end): only its FIRST pair (units 0..24) fits inside the
/// 44-unit pattern, so only k1 warps: k0=120,
/// k1=120+floor(1.5*25+0.5)=158, k2=120+floor(2*25+0.5)=170,
/// k3=120+floor(3*25+0.5)=195 (k=3's pair, units 24..48, no longer fits, so
/// it is NOT warped even though it's odd), end(k4)=120+floor(44.0/12*25+0.5)=212.
/// Slot 2 ("A" again, 125 BPM, run starts at tick 212, same shape as slot 0):
/// k0=212, k1=212+45=257, k2=212+60=272, k3=212+105=317, end(k4)=212+120=332.
///
/// The slot column's run-lengths (4, 4, 4) are `ceil(length/12)` steps per
/// slot, so the global step's prefix (`slot_steps` in `song_analyze`) is
/// (0, 4, 8) — exactly where this table's slot column changes.
const STEP_STARTS: [(u32, i64); 12] = [
    (0, 0),
    (0, 45),
    (0, 60),
    (0, 105),
    (1, 120),
    (1, 158),
    (1, 170),
    (1, 195),
    (2, 212),
    (2, 257),
    (2, 272),
    (2, 317),
];

/// The song's length in ticks: `end(k4)` of the last slot above.
const SLOT_STEP_SONG_LENGTH: i64 = 332;

/// The (arrangement slot, GLOBAL 16th step) a `tick` in `0..SLOT_STEP_SONG_LENGTH`
/// falls in: the latest [`STEP_STARTS`] entry starting at or before it.
fn expected_slot_step(tick: i64) -> (u32, u32) {
    let global = STEP_STARTS
        .iter()
        .rposition(|&(_, start)| start <= tick)
        .expect("tick 0 always matches the first entry");
    (STEP_STARTS[global].0, global as u32)
}

/// The engine playhead reports a native song source's arrangement slot and
/// GLOBAL 16th step — `slot_steps[slot] + step-in-slot`, the same index
/// `analyzeSong`'s `used`/`wanted` use — computed from `Timing::position`.
#[test]
fn score_view_reports_a_song_sources_slot_and_step() {
    let song = slot_step_song();
    assert_eq!(
        ppu_core::song::compile(&song).unwrap().timing.length,
        SLOT_STEP_SONG_LENGTH,
        "the hand-derived song length must match the compiled one"
    );
    assert_eq!(
        ppu_core::song_analyze::analyze(&song).unwrap().slot_steps,
        vec![0, 4, 8],
        "slot_steps must tie the readout's global step to the voice strip's own indexing"
    );

    // Hand-derived literal (see STEP_STARTS's doc comment): the swung 2nd
    // 16th of the odd-length B pattern lands at tick 158, global step 5
    // (slot 1's own step 1, after slot 0's 4 steps).
    assert_eq!(expected_slot_step(158), (1, 5));

    let mut e = LuaEngine::new();
    common::add_song_source(&mut e, "beat", &song);
    e.set_source("h = score{ song = \"beat\", loop = false }\nfunction frame() end")
        .unwrap();

    let mut seen = 0;
    for f in 0..90u32 {
        e.frame(f as f64 / 60.0, f).unwrap();
        let Some(v) = e.score_view().filter(|v| !v.finished) else {
            break;
        };
        assert_eq!(v.song.as_deref(), Some("beat"));
        let (want_slot, want_step) = expected_slot_step(v.tick);
        assert_eq!(v.slot, Some(want_slot), "frame {f} tick {}: slot", v.tick);
        assert_eq!(v.step, Some(want_step), "frame {f} tick {}: step", v.tick);
        seen += 1;
    }
    assert!(
        seen > 60,
        "expected most of these 90 frames to still be playing, got {seen}"
    );
}

/// A minimal one-note song naming `sound` in its only row.
fn one_row_song(sound: &str) -> Song {
    Song {
        tempo: 12000,
        swing: 0,
        key: 0,
        voice_mask: 0xff,
        rows: vec![Row {
            sound: sound.into(),
            note: None,
            vol: 127,
            pan: 0,
        }],
        patterns: vec![Pattern {
            name: "A".into(),
            length: 12,
            tempo: None,
            notes: vec![Note {
                at: 0,
                row: 0,
                len: 12,
                vel: 100,
                voice: None,
                nudge: 0,
                end_nudge: 0,
            }],
        }],
        arrangement: vec![0],
        loop_start: 0,
    }
}

/// Two song sources naming the same sound place it once: a toy playing
/// several songs would otherwise fill sound RAM with copies of the same
/// sample.
#[test]
fn scores_share_each_placed_sound() {
    let mut e = LuaEngine::new();
    common::add_sample(&mut e, "mine");
    common::add_song_source(&mut e, "a", &one_row_song("mine"));
    common::add_song_source(&mut e, "b", &one_row_song("mine"));
    e.set_source(
        "local real, n = dma, 0\n\
         dma = function(...) n = n + 1 return real(...) end\n\
         score{ song = \"a\" }\n\
         score{ song = \"b\" }\n\
         dma = real\n\
         function frame() sram.placed = n end",
    )
    .unwrap();
    e.frame(0.0, 0).unwrap();
    let got: Value = serde_json::from_str(&e.take_sram().unwrap()).unwrap();
    assert_eq!(got["placed"], 1);
}

/// A row's sound is a built-in (`bank(name)`), not an upload, but a sound-RAM
/// failure on its `dma()` must still be caught and named the same way the
/// upload path already is: pin a placement one sample-length short of the
/// top of sound RAM, so the built-in's own auto-chained `dma()` inside
/// `bank()` overflows and `score{}` must wrap that error too.
#[test]
fn score_built_in_sound_ram_failure_names_the_row() {
    let mut e = LuaEngine::new();
    common::add_song_source(&mut e, "beat", &one_row_song("bass"));
    let err = e
        .set_sources(&[(
            "main.lua",
            "local b = dma('bass')\n\
             local sz = b.next_addr - b.addr\n\
             dma('bass', { addr = 0x10000 - sz })\n\
             score{ song = \"beat\" }",
        )])
        .unwrap_err();
    assert_eq!(err.file.as_deref(), Some("main.lua"), "{err:?}");
    assert!(
        err.message.contains("row 1 sound 'bass'"),
        "got: {}",
        err.message
    );
}
