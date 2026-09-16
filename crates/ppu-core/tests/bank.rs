//! The built-in sample bank (`crates/ppu-core/src/bank.rs`) and its kit
//! sugar (`bank()`, `midi{}` auto-mapping), through `LuaEngine`'s public API.
use ppu_core::{bank, convert_sample, ConvertSampleOptions, LuaEngine, SourcePayload};

const FAST_ADSR: &str = "adsr = {a = 15, d = 0, s = 7, r = 0}";

fn is_silent(audio: &[i16]) -> bool {
    audio.iter().all(|&s| s == 0)
}

/// Every name encodes, is small, decodes cleanly, and loops block-aligned;
/// the whole bank stays SNES-sized.
#[test]
fn every_builtin_encodes_small_and_clean() {
    let mut total = 0;
    for name in bank::NAMES {
        let SourcePayload::Sample(s) = bank::get(name).expect(name) else {
            panic!("{name} is not a sample payload");
        };
        assert_eq!(s.brr.len() % 9, 0, "{name}: BRR is 9-byte blocks");
        assert!(
            s.brr.len() <= 5000,
            "{name}: {} bytes is too big for one built-in",
            s.brr.len()
        );
        let end_block = &s.brr[s.brr.len() - 9];
        assert_eq!(end_block & 1, 1, "{name}: last block carries END");
        let is_drum = ["kick", "snare", "hat", "ohat", "tom", "clap", "crash"].contains(&name);
        assert_eq!(
            s.loop_block.is_some(),
            !is_drum,
            "{name}: melodic loops, drums don't"
        );
        total += s.brr.len();
    }
    assert!(total < 24_000, "bank is {total} bytes, want < 24 KB");
    assert!(bank::get("nope").is_none());
}

/// `dma("piano")` with nothing uploaded places the built-in and a keyed
/// voice sounds.
#[test]
fn builtin_plays_without_any_upload() {
    let mut e = LuaEngine::new();
    e.set_source(&format!(
        "local p = dma('piano')\n\
         function init() dsp.mvol = {{ l = 127, r = 127 }} end\n\
         function frame(t, f)\n\
           voice[0].sample = p.id voice[0].pitch = 0x1000\n\
           voice[0].vol = {{ l = 127, r = 127 }} voice[0].{FAST_ADSR}\n\
           if f == 1 then kon(0) end\n\
         end"
    ))
    .unwrap();
    let placed = e.dsp_view().samples;
    assert_eq!(placed.len(), 1);
    assert_eq!(placed[0].start, 0x0500);
    for f in 0..4 {
        e.frame(0.0, f).unwrap();
    }
    assert!(!is_silent(e.audio()), "built-in piano should sound");
}

/// A user source registered as "piano" shadows the built-in: the placement
/// takes the user's byte length, not the bank's.
#[test]
fn user_source_shadows_builtin_of_the_same_name() {
    let pcm: Vec<i16> = (0..640).map(|i| ((i % 32) as i16 - 16) * 500).collect();
    let (payload, _) = convert_sample(
        &pcm,
        &ConvertSampleOptions {
            loop_start: Some(0),
        },
    )
    .unwrap();
    let mut e = LuaEngine::new();
    e.add_source("piano", &payload.encode()).unwrap();
    e.set_source("p = dma('piano') function frame() end")
        .unwrap();
    let placed = &e.dsp_view().samples[0];
    assert_eq!(
        placed.end - placed.start,
        360,
        "640 frames = 40 blocks = 360 bytes"
    );
    // Removing the upload uncovers the built-in again.
    assert!(e.remove_source("piano"));
    e.set_source("p = dma('piano') function frame() end")
        .unwrap();
    let SourcePayload::Sample(b) = bank::get("piano").unwrap() else {
        unreachable!()
    };
    let placed = &e.dsp_view().samples[0];
    assert_eq!(placed.end - placed.start, b.brr.len() as u32);
}

/// Default `dma()` placements chain upward (so `bank()` calls never
/// overlap), presets carry (drums at pitch 0x0800), and opts override.
#[test]
fn bank_chains_placements_and_applies_presets() {
    let mut e = LuaEngine::new();
    e.set_source(
        "piano = bank('piano')\n\
         kick = bank('kick', { vol = 60 })\n\
         function frame(t, f)\n\
           if f == 0 then sfx(kick, 0) sfx(piano, 1, 'C4') end\n\
           vram[0] = kick.addr vram[1] = piano.next_addr\n\
           vram[3] = voice[0].pitch vram[4] = voice[0].vol.l vram[5] = voice[1].pitch\n\
         end",
    )
    .unwrap();
    e.frame(0.0, 0).unwrap();
    let v = &e.memory().vram;
    assert_eq!(v[0], v[1], "kick starts where piano ends");
    assert_eq!(v[3], 0x0800, "drum preset pitch");
    assert_eq!(v[4], 60, "opts.vol overrides the preset");
    assert_eq!(v[5], 0x1000, "C4 on a C4-based melodic sample");
    let err = e.set_source("bank('kazoo')").unwrap_err();
    assert!(
        err.message.contains("no built-in sample 'kazoo'"),
        "{err:?}"
    );
}

/// `midi{ data = tune }` with no tracks maps GM programs and channel 10 to
/// the bank and plays: a bass line (prog 33) and a kick on ch 10 both key on
/// within the first frames, on disjoint voices.
#[test]
fn midi_auto_maps_gm_to_the_bank() {
    let mut e = LuaEngine::new();
    e.set_source(
        "kon_log = {}\n\
         local real_kon = kon\n\
         function kon(v) kon_log[#kon_log + 1] = v real_kon(v) end\n\
         tune = { length = 1, tracks = {\n\
           { name = 'Bass', ch = 1, prog = 33, notes = { {0, 0.5, 36, 100} } },\n\
           { name = 'Drums', ch = 9, notes = { {0, 0.1, 36, 100}, {0, 0.1, 42, 80} } },\n\
         } }\n\
         h = midi{ data = tune }\n\
         function frame(t, f)\n\
           vram[0] = #kon_log\n\
           for i = 1, #kon_log do vram[i] = kon_log[i] end\n\
         end",
    )
    .unwrap();
    // bass + kick + hat placed, in first-use order.
    let placed = e.dsp_view().samples;
    assert_eq!(placed.len(), 3, "{placed:?}");
    for f in 0..3 {
        e.frame(0.0, f).unwrap();
    }
    let v = &e.memory().vram;
    let mut voices: Vec<u16> = (1..=v[0] as usize).map(|i| v[i]).collect();
    voices.sort();
    assert_eq!(
        voices,
        vec![0, 6, 7],
        "bass on voice 0, two drums on the kit's voices 6/7"
    );
}

/// The shorthands the Studio's Audio panel writes: `inst = "name"` resolves a
/// built-in through bank(), `inst = "gm"` follows the track's program (or the
/// drum kit on ch10), `drums = true` builds the kit for the keys used, and one
/// name is placed once however many tracks share it.
#[test]
fn midi_track_shorthands_resolve_names_gm_and_drums() {
    let mut e = LuaEngine::new();
    e.set_source(
        "kon_log = {}\n\
         local real_kon = kon\n\
         function kon(v) kon_log[#kon_log + 1] = v real_kon(v) end\n\
         tune = { length = 1, tracks = {\n\
           { name = 'Bass', ch = 1, prog = 33, notes = { {0, 0.5, 36, 100} } },\n\
           { name = 'Lead', ch = 2, prog = 80, notes = { {0, 0.5, 72, 100} } },\n\
           { name = 'Drums', ch = 9, notes = { {0, 0.1, 36, 100}, {0, 0.1, 42, 80} } },\n\
         } }\n\
         h = midi{ data = tune, tracks = {\n\
           [1] = { inst = 'bell', voices = { 0 } },\n\
           [2] = { inst = 'gm', voices = { 1, 2 } },\n\
           [3] = { drums = true, voices = { 5 } },\n\
         } }\n\
         function frame(t, f)\n\
           vram[0] = #kon_log\n\
           for i = 1, #kon_log do vram[i] = kon_log[i] end\n\
         end",
    )
    .unwrap();
    let mut placed: Vec<String> = e
        .dsp_view()
        .samples
        .iter()
        .map(|s| s.name.clone())
        .collect();
    placed.sort();
    assert_eq!(
        placed,
        vec!["bell", "hat", "kick", "lead"],
        "prog 80 is the lead family"
    );
    for f in 0..3 {
        e.frame(0.0, f).unwrap();
    }
    let v = &e.memory().vram;
    let mut voices: Vec<u16> = (1..=v[0] as usize).map(|i| v[i]).collect();
    voices.sort();
    assert_eq!(
        voices,
        vec![0, 1, 5, 5],
        "bell on 0, lead on 1, both drums round-robin on 5"
    );
}

/// `midi{ data = tune }` on a program that never touches the mixer is audible:
/// the kit opens the power-on-silent master volume when it starts a song. A
/// program that set its own level keeps it.
#[test]
fn midi_opens_master_volume_when_unset() {
    let data = "tune = { length = 2, tracks = { { name = 'x', ch = 0, prog = 0, notes = { {0, 0.5, 60, 100}, {0.5, 0.5, 64, 100} } } } }";
    let mut e = LuaEngine::new();
    e.set_source(&format!(
        "{data}\nh = midi{{ data = tune }}\nfunction frame(t, f) end"
    ))
    .unwrap();
    let mut peak = 0i32;
    for f in 0..30 {
        e.frame(f as f64 / 60.0, f).unwrap();
        peak = peak.max(
            e.audio()
                .iter()
                .map(|&s| (s as i32).abs())
                .max()
                .unwrap_or(0),
        );
    }
    assert!(
        peak > 1000,
        "peak {peak}: the song should be audible with no mixer line"
    );
    assert_eq!(e.dsp_view().mvol.l, 127);

    let mut quiet = LuaEngine::new();
    quiet
        .set_source(&format!(
            "{data}\ndsp.mvol = {{ l = 20, r = 20 }}\nh = midi{{ data = tune }}\nfunction frame(t, f) end"
        ))
        .unwrap();
    quiet.frame(0.0, 0).unwrap();
    assert_eq!(
        quiet.dsp_view().mvol.l,
        20,
        "an explicit level is left alone"
    );
}

/// Renders a showcase of every bank sound to `$BANK_DEMO_WAV` (16-bit
/// stereo 32 kHz) and prints each sample's size and encoder SNR. Ignored:
/// it is a listening aid, not an assertion.
///
///     BANK_DEMO_WAV=/tmp/bank.wav cargo test -p ppu-core --test bank -- --ignored --nocapture
#[test]
#[ignore]
fn render_demo_wav() {
    let Ok(path) = std::env::var("BANK_DEMO_WAV") else {
        return;
    };
    for name in bank::NAMES {
        let SourcePayload::Sample(s) = bank::get(name).unwrap() else {
            unreachable!()
        };
        println!("{name:8} {:5} bytes  loop {:?}", s.brr.len(), s.loop_block);
    }
    let mut e = LuaEngine::new();
    e.set_source(
        "melodic = { 'piano', 'bass', 'lead', 'strings', 'organ', 'bell', 'flute', 'pluck' }\n\
         insts = {}\n\
         for i, n in ipairs(melodic) do insts[i] = bank(n) end\n\
         drums = { bank('kick'), bank('snare'), bank('hat'), bank('ohat'), bank('tom'), bank('clap'), bank('crash') }\n\
         function init() dsp.mvol = { l = 127, r = 127 } end\n\
         -- each melodic: 4 notes 0.3 s apart (C4 E4 G4 C5), then 0.6 s of rest; drums: 1 hit each, 0.4 s apart\n\
         local seq = { 'C4', 'E4', 'G4', 'C5' }\n\
         function frame(t, f)\n\
           local slot = math.floor(f / 108)\n\
           local step = math.floor((f % 108) / 18)\n\
           if slot < #melodic then\n\
             if f % 18 == 0 and step < 4 then sfx(insts[slot + 1], 0, seq[step + 1]) end\n\
             if f % 18 == 17 and step == 3 then koff(0) end\n\
           else\n\
             local d = math.floor((f - #melodic * 108) / 24) + 1\n\
             if f % 24 == 0 and drums[d] then sfx(drums[d], 1) end\n\
           end\n\
         end",
    )
    .unwrap();
    let frames = (8.0 * 1.8 + 7.0 * 0.4 + 1.0) * 60.0;
    let mut pcm: Vec<i16> = Vec::new();
    for f in 0..frames as u32 {
        e.frame(f as f64 / 60.0, f).unwrap();
        pcm.extend_from_slice(e.audio());
    }
    let bytes = pcm.len() * 2;
    let mut out = Vec::with_capacity(44 + bytes);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + bytes) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&2u16.to_le_bytes()); // stereo
    out.extend_from_slice(&32000u32.to_le_bytes());
    out.extend_from_slice(&(32000u32 * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(bytes as u32).to_le_bytes());
    for s in pcm {
        out.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(&path, out).unwrap();
    println!("wrote {path}");
}
