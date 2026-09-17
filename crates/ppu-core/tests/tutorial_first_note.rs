//! Tutorial toy :: first-note — a sample, a song, an sfx on A, and a light
//! that follows the envelope (see `tutorial_first_light.rs`, the register
//! tutorial this one mirrors in shape; this one is the sound-chip
//! counterpart, M12).
mod common;

use ppu_core::{render_frame, DspSampleView, LuaEngine};
use std::path::Path;

/// The toy. Byte-identical to the fenced block in docs/audio.md
/// (`chapter_carries_the_toy_verbatim` pins it) — see `tutorial_first_light.rs`
/// for the same convention.
const MAIN_SRC: &str = r#"-- ppu.toys tutorial :: first-note — a sample, a song, an sfx on A, and a light that follows the envelope
-- The sound chip (S-DSP) is a second machine bolted onto the console: its
-- own 64 KB of sound RAM (ARAM), its own clock, its own eight voices. dma()
-- on a sample source copies BRR-encoded bytes into that RAM and hands back
-- a directory entry (kick.id is the SRCN a voice points at). song{} steps a
-- pattern on a hardware timer, same idea as hdma but for sound. sfx() keys
-- a voice on demand; voice[n].envx is that voice's live envelope — the same
-- number you can wire straight into the picture.
local kick = dma("kick")                          -- BRR bytes land in sound RAM at 0x0500 + a directory entry; kick.id is the SRCN
local bass = instrument{ sample = kick.id, adsr = { a = 15, d = 4, s = 4, r = 8 } }
local hit  = instrument{ sample = kick.id, adsr = { a = 15, d = 0, s = 7, r = 0 }, pan = 0.5, base = "C3" }
local tune = song{ tempo = 120, tracks = {
  { voice = 0, inst = bass, pattern = "C2 . . G2 . . C3 ." },
} }

function init()
  dsp.mvol = { l = 127, r = 127 }                 -- power-on MVOL is 0 (silent) — turn the speakers on
end

local was_a = false
function frame(t, f)
  local pressed = pad.a and not was_a             -- edge, not level: sfx() restarts the voice every call
  was_a = pad.a
  if pressed then sfx(hit, 1, "C4") end
  brightness = floor(voice[0].envx / 8)           -- ENVX is 0..127; brightness is 0..15
  cgram[0] = hsl(200 + tune.step * 12, 0.6, 0.3)  -- the backdrop hue drifts with the song's own step counter
end
-- Try: swap "C4" for a different sfx pitch, hold A twice fast to hear the retrigger, or drive brightness from voice[1].envx instead
"#;

const GOLDEN_PNG: &str = "tests/fixtures/golden_tutorial_first_note.png";
const GOLDEN_PCM: &str = "tests/fixtures/tutorial_first_note.bin";

fn build_engine() -> LuaEngine {
    common::engine_with(
        &mut |e| common::add_sample(e, "kick"),
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
    let expected = vec![DspSampleView {
        id: 0,
        name: "kick".to_string(),
        start: 0x0500,
        end: 0x0500 + common::SAMPLE_BYTES,
    }];
    assert_eq!(e.dsp_view().samples, expected);
}

#[test]
fn song_keys_the_bass_voice() {
    let mut e = build_engine();
    e.frame(0.0, 0).unwrap();
    assert!(
        is_silent(e.audio()),
        "frame 0 should be silent — the first step keys on at sample 1000, past frame 0's length"
    );
    e.frame(1.0 / 60.0, 1).unwrap();
    assert!(
        !is_silent(e.audio()),
        "frame 1 should contain the first key-on, around sample offset 467"
    );
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
/// just rendered this frame. `saw_nonzero_at_or_after_2` is the
/// non-tautological bound — without it the assertion above would pass
/// trivially if brightness stayed 0 forever.
#[test]
fn brightness_follows_the_envelope() {
    let mut e = build_engine();
    let mut envx_prev: u8 = 0;
    let mut saw_nonzero_at_or_after_2 = false;
    for f in 0..=3u32 {
        let lt = e.frame(f as f64 / 60.0, f).unwrap();
        let bri = lt.rows[0].brightness;
        if f <= 1 {
            assert_eq!(
                bri, 0,
                "brightness at f={f} should be 0 before any envelope"
            );
        }
        assert_eq!(
            bri,
            envx_prev / 8,
            "brightness at f={f} should track floor(envx_prev / 8)"
        );
        if f >= 2 && bri > 0 {
            saw_nonzero_at_or_after_2 = true;
        }
        envx_prev = e.dsp_view().voices[0].envx;
    }
    assert!(
        saw_nonzero_at_or_after_2,
        "expected brightness > 0 for some f >= 2"
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
        // `castle` stands in for the data file a .mid upload generates, and
        // `mybass` for a user's own sample, so the MIDI snippets run too.
        let castle = "castle = { length = 1, tracks = {\n\
          { name = 'Lead', ch = 0, prog = 80, notes = { {0, 0.5, 60, 100}, {0.5, 0.5, 64, 90} } },\n\
          { name = 'Drums', ch = 9, prog = 0, notes = { {0, 0.1, 36, 100}, {0.5, 0.1, 38, 100} } },\n\
        } }";
        let mut e = common::engine_with(
            &mut |e| {
                common::add_sample(e, "kick");
                common::add_sample(e, "mybass");
            },
            &[("castle.lua", castle), ("main.lua", block)],
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
