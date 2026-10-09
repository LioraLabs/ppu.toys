//! Tutorial toy :: first-note — a sample, a song source on score{}, an sfx on A, and a light
//! that follows the envelope (see `tutorial_first_light.rs`, the register
//! tutorial this one mirrors in shape; this one is the sound-chip
//! counterpart, M12).
mod common;

use ppu_core::render_frame;
use ppu_core::song::{Note, Pattern, Row, Song};
use ppu_core::{DspSampleView, LuaEngine};
use std::path::Path;

/// A small two-row song (a `kick` hit, then a `mybass` note) — the
/// `every_lua_snippet_in_the_chapter_runs` stand-in for a `beat1` song
/// source, registered through `common::add_song_source`.
fn beat1_song() -> Song {
    Song {
        tempo: 12000, // 120 BPM
        swing: 0,
        key: 0,
        voice_mask: 0xff,
        rows: vec![
            Row {
                sound: "kick".into(),
                note: None,
                vol: 127,
                pan: 0,
            },
            Row {
                sound: "mybass".into(),
                note: Some(36), // C2
                vol: 100,
                pan: 0,
            },
        ],
        patterns: vec![Pattern {
            name: "A".into(),
            length: 48,
            tempo: None,
            notes: vec![
                Note {
                    at: 0,
                    row: 0,
                    len: 12,
                    vel: 100,
                    voice: None,
                    nudge: 0,
                    end_nudge: 0,
                },
                Note {
                    at: 24,
                    row: 1,
                    len: 24,
                    vel: 90,
                    voice: None,
                    nudge: 0,
                    end_nudge: 0,
                },
            ],
        }],
        arrangement: vec![0],
        loop_start: 0,
    }
}

/// The bass line the toy plays: `bassline`, one bar of sixteenths
/// ("C2 . . G2 . . C3 ." twice) on one `kick` row per pitch, voice 0 only.
fn bassline_song() -> Song {
    let row = |note| Row {
        sound: "kick".into(),
        note: Some(note),
        vol: 127,
        pan: 0,
    };
    let note = |at, row, len| Note {
        at,
        row,
        len,
        vel: 127,
        voice: Some(0),
        nudge: 0,
        end_nudge: 0,
    };
    Song {
        tempo: 12000, // 120 BPM
        swing: 0,
        key: 0,
        voice_mask: 0x01,
        rows: vec![row(36), row(43), row(48)], // C2, G2, C3
        patterns: vec![Pattern {
            name: "bar 1".into(),
            length: 192, // 16 steps x 12 units
            tempo: None,
            notes: vec![
                note(0, 0, 36),
                note(36, 1, 36),
                note(72, 2, 24),
                note(96, 0, 36),
                note(132, 1, 36),
                note(168, 2, 24),
            ],
        }],
        arrangement: vec![0],
        loop_start: 0,
    }
}

/// The toy. Byte-identical to the fenced block in docs/audio.md
/// (`chapter_carries_the_toy_verbatim` pins it) — see `tutorial_first_light.rs`
/// for the same convention.
const MAIN_SRC: &str = r#"-- ppu.toys tutorial :: first-note — a sample, a song, an sfx on A, and a light that follows the envelope
-- The sound chip (S-DSP) is a second machine bolted onto the console: its
-- own 64 KB of sound RAM (ARAM), its own clock, its own eight voices. dma()
-- on a sample source copies BRR-encoded bytes into that RAM and hands back
-- a directory entry (kick.id is the SRCN a voice points at). score{} plays a
-- song source (bassline, made in the sequencer) on the timeline: its
-- position is t itself, so seeking the timeline seeks the song. sfx() keys
-- a voice on demand; voice[n].envx is that voice's live envelope — the same
-- number you can wire straight into the picture.
local kick = dma("kick")                          -- BRR bytes land in sound RAM at 0x0500 + a directory entry; kick.id is the SRCN
local hit  = instrument{ sample = kick.id, adsr = { a = 15, d = 0, s = 7, r = 0 }, pan = 0.5, base = "C3" }
-- score{} places its own copy of "kick" through bank() (this upload takes the built-in's name over),
-- so voice 0's SRCN is that copy, not kick.id; its "kick" row plays with a bass envelope, not the drum preset's
BANK.kick.adsr = { a = 15, d = 4, s = 4, r = 8 }
local tune = score{ song = "bassline" }           -- C2 . . G2 . . C3 . on voice 0, 120 BPM, looping

function init()
  dsp.mvol = { l = 127, r = 127 }                 -- power-on MVOL is 0 (silent) — turn the speakers on
end

local was_a = false
function frame(t, f)
  local pressed = pad.a and not was_a             -- edge, not level: sfx() restarts the voice every call
  was_a = pad.a
  if pressed then sfx(hit, 1, "C4") end
  brightness = floor(voice[0].envx / 8)           -- ENVX is 0..127; brightness is 0..15
  local step = floor(tune.tick / 31.25) % 16      -- 250 ticks a second, a sixteenth every 31.25 at 120 BPM: 0..15
  cgram[0] = hsl(200 + step * 12, 0.6, 0.3)       -- the backdrop hue drifts with the song's own step
end
-- Try: swap "C4" for a different sfx pitch, hold A twice fast to hear the retrigger, or drive brightness from voice[1].envx instead
"#;

const GOLDEN_PNG: &str = "tests/fixtures/golden_tutorial_first_note.png";
const GOLDEN_PCM: &str = "tests/fixtures/tutorial_first_note.bin";

fn build_engine() -> LuaEngine {
    common::engine_with(
        &mut |e| {
            common::add_sample(e, "kick");
            common::add_song_source(e, "bassline", &bassline_song());
        },
        &[("main.lua", MAIN_SRC)],
    )
}

fn is_silent(audio: &[i16]) -> bool {
    audio.iter().all(|&s| s == 0)
}

/// Drives the toy for 40 frames (`t = f / 60.0`), holding A for frames 20
/// and 21, and returns the final frame's rendered framebuffer plus every
/// call's audio concatenated in order.
fn run() -> (Vec<u8>, Vec<i16>) {
    let mut e = build_engine();
    let mut pcm = Vec::new();
    let mut lt = None;
    for f in 0..=39u32 {
        if f == 20 {
            e.set_pad(1 << 4); // pad.a
        }
        if f == 22 {
            e.set_pad(0);
        }
        lt = Some(e.frame(f as f64 / 60.0, f).unwrap());
        pcm.extend_from_slice(e.audio());
    }
    let fb = render_frame(&lt.unwrap(), e.memory());
    (fb, pcm)
}

#[test]
fn sample_is_placed_in_sound_ram() {
    let e = build_engine();
    // Two copies: the toy's own `dma("kick")` (id 0, the `hit` sfx), then the
    // one `score{}` places through bank() for the song's "kick" rows (id 1,
    // right behind it), so the bass can carry its own envelope.
    let own = DspSampleView {
        id: 0,
        name: "kick".to_string(),
        start: 0x0500,
        end: 0x0500 + common::SAMPLE_BYTES,
    };
    let banked = DspSampleView {
        id: 1,
        start: own.end,
        end: own.end + common::SAMPLE_BYTES,
        ..own.clone()
    };
    assert_eq!(e.dsp_view().samples, vec![own, banked]);
}

#[test]
fn song_keys_the_bass_voice() {
    let mut e = build_engine();
    // score{} anchors the song at t = 0 and its first note is due at tick 0,
    // i.e. (0 + 1) / 250 s — inside frame 0 — so the bass keys on in frame 0
    // itself.
    e.frame(0.0, 0).unwrap();
    assert!(
        !is_silent(e.audio()),
        "frame 0 should already contain the first key-on (tick 0 lands inside it)"
    );
    assert!(
        e.dsp_view().voices[0].envx > 0,
        "voice 0's envelope should be attacking after frame 0"
    );
    e.frame(1.0 / 60.0, 1).unwrap();
    assert!(!is_silent(e.audio()), "the bass keeps sounding in frame 1");
    e.frame(2.0 / 60.0, 2).unwrap();
    assert!(
        e.dsp_view().voices[0].envx > 0,
        "voice 0's envelope should be attacking once its key-on has rendered"
    );
}

#[test]
fn a_press_fires_the_sfx_on_voice_1() {
    let mut e = build_engine();
    for f in 0..=19u32 {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    assert_eq!(
        e.dsp_view().voices[1].envx,
        0,
        "voice 1 should be silent before any A press"
    );

    e.set_pad(1 << 4); // pad.a
    e.frame(20.0 / 60.0, 20).unwrap();
    e.frame(21.0 / 60.0, 21).unwrap();

    let v1 = &e.dsp_view().voices[1];
    assert!(
        v1.envx > 0,
        "voice 1's envelope should be attacking once the sfx's key-on has rendered"
    );
    assert_eq!(v1.pitch, 8192, "base C3 + sfx note C4 is one octave up");
    assert_eq!(v1.vol.l, 64, "pan = 0.5 splits volume toward the right");
    assert_eq!(v1.vol.r, 127);
}

/// Exercises the one-frame read-back lag: `brightness` on frame `f` must
/// equal `floor(envx / 8)` from frame `f - 1`, not the envelope `frame()`
/// just rendered this frame. `saw_nonzero_at_or_after_1` is the
/// non-tautological bound — without it the assertion above would pass
/// trivially if brightness stayed 0 forever. The song keys on inside frame 0,
/// so the first non-zero brightness is already on frame 1.
#[test]
fn brightness_follows_the_envelope() {
    let mut e = build_engine();
    let mut envx_prev: u8 = 0;
    let mut saw_nonzero_at_or_after_1 = false;
    for f in 0..=3u32 {
        let lt = e.frame(f as f64 / 60.0, f).unwrap();
        let bri = lt.rows[0].brightness;
        if f == 0 {
            assert_eq!(bri, 0, "brightness at f=0 should be 0 before any envelope");
        }
        assert_eq!(
            bri,
            envx_prev / 8,
            "brightness at f={f} should track floor(envx_prev / 8)"
        );
        if f >= 1 && bri > 0 {
            saw_nonzero_at_or_after_1 = true;
        }
        envx_prev = e.dsp_view().voices[0].envx;
    }
    assert!(
        saw_nonzero_at_or_after_1,
        "expected brightness > 0 for some f >= 1"
    );
}

#[test]
fn first_note_matches_golden_png() {
    assert!(Path::new(GOLDEN_PNG).exists());
    let (fb, _pcm) = run();
    let expected = common::decode_png(GOLDEN_PNG);
    assert_eq!(fb.len(), expected.len());
    assert!(
        fb == expected,
        "first-note framebuffer differs from golden PNG"
    );
}

#[test]
fn first_note_matches_golden_pcm() {
    assert!(Path::new(GOLDEN_PCM).exists());
    let (_fb, pcm) = run();
    assert!(pcm.iter().any(|&s| s != 0), "pcm should not be all zero");
    let max_abs = pcm.iter().map(|&s| (s as i32).abs()).max().unwrap();
    assert!(
        max_abs > 1000,
        "max abs sample {max_abs} should exceed 1000"
    );

    let actual: Vec<u8> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
    let expected = std::fs::read(GOLDEN_PCM).unwrap();
    assert_eq!(actual, expected, "pcm differs from golden");
}

#[test]
#[ignore = "regenerates the committed first-note goldens"]
fn regen_golden_first_note() {
    let (fb, pcm) = run();
    common::write_png(GOLDEN_PNG, &fb);
    let bytes: Vec<u8> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
    std::fs::create_dir_all("tests/fixtures").unwrap();
    std::fs::write(GOLDEN_PCM, bytes).unwrap();
}

const AUDIO_CHAPTER: &str = include_str!("../../ppu-cli/docs/audio.md");

#[test]
fn chapter_carries_the_toy_verbatim() {
    let needle = format!("```lua\n{MAIN_SRC}```");
    assert!(
        AUDIO_CHAPTER.contains(&needle),
        "docs/audio.md should carry MAIN_SRC verbatim in a ```lua fence"
    );
}

/// Split the chapter on ```lua fences and run every block through the same
/// engine the toy itself uses (a `kick` sample source registered, pokes.lua
/// present, pad released) for a few frames — the chapter's own harness, so
/// a fact that no longer matches the engine breaks this test instead of
/// quietly rotting in prose.
#[test]
fn every_lua_snippet_in_the_chapter_runs() {
    let fence = "```lua\n";
    let mut blocks = Vec::new();
    let mut rest = AUDIO_CHAPTER;
    while let Some(start) = rest.find(fence) {
        let after = &rest[start + fence.len()..];
        let end = after
            .find("```")
            .expect("unterminated ```lua fence in docs/audio.md");
        blocks.push(&after[..end]);
        rest = &after[end..];
    }
    assert!(
        !blocks.is_empty(),
        "no ```lua blocks found in docs/audio.md"
    );
    for block in blocks {
        let first_line = block.lines().next().unwrap_or("");
        // `mybass` stands in for a user's own sample, so its snippets run too.
        let mut e = common::engine_with(
            &mut |e| {
                common::add_sample(e, "kick");
                common::add_sample(e, "mybass");
                common::add_song_source(e, "beat1", &beat1_song());
                common::add_song_source(e, "bassline", &bassline_song());
            },
            &[("main.lua", block)],
        );
        for f in 0..3u32 {
            // A is held on frame 1 only, so pad-gated branches run too.
            e.set_pad(if f == 1 { 1 << 4 } else { 0 });
            e.frame(f as f64 / 60.0, f).unwrap_or_else(|err| {
                panic!("docs/audio.md block starting {first_line:?} failed at frame {f}: {err:?}")
            });
        }
    }
}
