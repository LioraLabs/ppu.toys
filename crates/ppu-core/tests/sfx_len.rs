//! sfx() keys its voice off `len` seconds after keying it on: 0.15 s by default on a looping
//! sample, never by default on a one-shot, and never with `len = false`.
use ppu_core::LuaEngine;

/// Runs `frame` bodies (with `f` in scope) for frames 0..=last and returns, per frame, voice 0's
/// KOF bit and ENVX.
fn run(setup: &str, body: &str, last: u32) -> (LuaEngine, Vec<(bool, u8)>) {
    let mut e = LuaEngine::new();
    e.set_source(&format!(
        "dsp.mvol = {{l = 127, r = 127}}\n{setup}\nfunction frame(t, f)\n{body}\nend"
    ))
    .unwrap();
    let mut seen = vec![];
    for f in 0..=last {
        e.frame(f as f64 / 60.0, f).unwrap();
        seen.push((e.dsp().read(0x5c) & 1 != 0, e.dsp_view().voices[0].envx));
    }
    (e, seen)
}

/// The first frame voice 0 is keyed off, if any.
fn key_off(seen: &[(bool, u8)]) -> Option<usize> {
    seen.iter().position(|s| s.0)
}

#[test]
fn a_looping_sample_stops_after_the_default_length() {
    let (_, seen) = run(
        "local organ = bank('organ')",
        "if f == 0 then sfx(organ, 0, 'C4') end",
        20,
    );
    assert_eq!(key_off(&seen), Some(9), "0.15 s is nine frames");
    assert!(seen[8].1 > 0, "it sounds until then: {:?}", seen[8]);
    assert_eq!(seen[10].1, 0, "and is silent a frame after the key-off");
}

#[test]
fn a_one_shot_drum_plays_to_its_end_with_no_key_off() {
    let (e, seen) = run(
        "local kick = bank('kick')",
        "if f == 0 then sfx(kick, 0) end",
        60,
    );
    assert_eq!(e.sfx_hold(0), 0);
    assert_eq!(key_off(&seen), None);
    assert!(seen[2].1 > 0, "the drum sounds");
}

#[test]
fn len_on_the_call_or_the_preset_sets_the_length() {
    let (_, seen) = run(
        "local organ = bank('organ')",
        "if f == 0 then sfx(organ, 0, 'C4', {len = 0.5}) end",
        40,
    );
    assert_eq!(key_off(&seen), Some(30));
    let (_, seen) = run(
        "local organ = bank('organ', {len = 0.25})",
        "if f == 0 then sfx(organ, 0) end",
        40,
    );
    assert_eq!(key_off(&seen), Some(15));
    // The call's len wins over the preset's.
    let (_, seen) = run(
        "local organ = bank('organ', {len = 0.25})",
        "if f == 0 then sfx(organ, 0, nil, {len = 0.1}) end",
        40,
    );
    assert_eq!(key_off(&seen), Some(6));
    // A one-shot cut short.
    let (_, seen) = run(
        "local crash = bank('crash', {len = 0.1})",
        "if f == 0 then sfx(crash, 0) end",
        40,
    );
    assert_eq!(key_off(&seen), Some(6));
}

#[test]
fn len_false_sustains_until_koff() {
    let (e, seen) = run(
        "local organ = bank('organ', {len = false})",
        "if f == 0 then sfx(organ, 0) end",
        60,
    );
    assert_eq!(key_off(&seen), None);
    assert!(seen[60].1 > 0, "still sounding after 1 s");
    assert_eq!(
        e.dsp_view().voices[0].sustains,
        None,
        "a deliberate long note is no warning"
    );
    let (_, seen) = run(
        "local organ = bank('organ')",
        "if f == 0 then sfx(organ, 0, nil, {len = false}) end\nif f == 30 then koff(0) end",
        40,
    );
    assert_eq!(key_off(&seen), Some(30));
}

#[test]
fn keying_the_voice_again_restarts_the_count() {
    let (_, seen) = run(
        "local organ = bank('organ')",
        "if f == 0 or f == 5 then sfx(organ, 0) end",
        30,
    );
    assert_eq!(key_off(&seen), Some(14));
    // A retrigger in the frame the count runs out wins.
    let (_, seen) = run(
        "local organ = bank('organ')",
        "if f == 0 or f == 9 then sfx(organ, 0) end",
        30,
    );
    assert_eq!(key_off(&seen), Some(18));
    // A drum on the voice cancels the organ's count.
    let (e, seen) = run(
        "local organ = bank('organ')\nlocal kick = bank('kick')",
        "if f == 0 then sfx(organ, 0) end\nif f == 3 then sfx(kick, 0) end",
        30,
    );
    assert_eq!((key_off(&seen), e.sfx_hold(0)), (None, 0));
}

#[test]
fn a_raw_key_on_cancels_the_count_and_reports_how_long_it_sustains() {
    let (e, seen) = run(
        "local piano = bank('piano')",
        "if f == 0 then sfx(piano, 0) end\nif f == 3 then kon(0) end",
        30,
    );
    assert_eq!(key_off(&seen), None);
    let sustains = e.dsp_view().voices[0]
        .sustains
        .expect("a looping voice with no key-off");
    let (_, seen) = run(
        "local piano = bank('piano')",
        "if f == 0 then sfx(piano, 0, nil, {len = false}) end",
        600,
    );
    let silent = seen
        .iter()
        .position(|s| s.1 == 0)
        .expect("piano (r = 10) ends") as f64
        / 60.0;
    assert!(
        (sustains - silent).abs() < silent * 0.1,
        "estimate {sustains} s, silent at {silent} s"
    );
    let (e, _) = run(
        "local organ = bank('organ', {adsr = {a = 15, d = 7, s = 7, r = 0}})",
        "if f == 0 then sfx(organ, 0, nil, {len = false}) kon(0) end",
        2,
    );
    assert_eq!(
        e.dsp_view().voices[0].sustains,
        None,
        "the same frame as an sfx is the sfx's key-on"
    );
    let (e, _) = run(
        "local organ = bank('organ', {adsr = {a = 15, d = 7, s = 7, r = 0}})",
        "if f == 0 then sfx(organ, 0) end\nif f == 1 then kon(0) end",
        2,
    );
    assert_eq!(
        e.dsp_view().voices[0].sustains,
        Some(f64::INFINITY),
        "r = 0 holds forever"
    );
}

#[test]
fn a_bad_len_is_an_error() {
    let mut e = LuaEngine::new();
    e.set_source("local organ = bank('organ')\nfunction frame() sfx(organ, 0, nil, {len = 0}) end")
        .unwrap();
    let err = e.frame(0.0, 0).unwrap_err();
    assert!(
        err.message.contains("len must be seconds"),
        "{}",
        err.message
    );
}
