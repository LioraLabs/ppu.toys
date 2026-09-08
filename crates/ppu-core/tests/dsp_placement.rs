//! `dma(name, { addr })` on a SAMPLE source writes the BRR bytes into ARAM +
//! a directory entry once, by `LuaEngine::set_sources` after `init()`
//! (never replayed per frame), and rejects overflow / directory-page /
//! echo-region / sample-sample overlap. Driven entirely through
//! `LuaEngine`'s public API
//! (`add_source`/`set_source`/`frame`/`audio`/`dsp_view`/`remove_source`) —
//! never reaching into `Dsp` or ARAM directly.
use ppu_core::{convert_sample, ConvertSampleOptions, DspSampleView, LuaEngine};

/// 1 kHz sine at 32 kHz: 32 samples/period, amplitude 20000, 640 samples
/// (20 periods) -> 640/16 = 40 BRR blocks, 40*9 = 360 bytes (see
/// `source_roundtrip.rs::sine_640`, same shape).
fn sine_640() -> Vec<i16> {
    (0..640)
        .map(|i| {
            let phase = (i as f64) / 32.0 * std::f64::consts::TAU;
            (phase.sin() * 20000.0).round() as i16
        })
        .collect()
}

const SAMPLE_BYTES: i64 = 360;

/// Registers a looping 640-sample sine sample under `name`.
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

fn is_silent(audio: &[i16]) -> bool {
    audio.iter().all(|&s| s == 0)
}

/// Lua `frame()` body driving voice 0 off `sample_id` every frame, with
/// `kon(0)` fired only on frame number `kon_frame`.
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

#[test]
fn two_samples_chain_and_the_keyed_voice_sounds() {
    let mut e = LuaEngine::new();
    add_sine(&mut e, "kick");
    add_sine(&mut e, "snare");

    let program = format!(
        "function init()\n\
           local a = dma(\"kick\", {{ addr = 0x0500 }})\n\
           local b = dma(\"snare\", {{ addr = a.next_addr }})\n\
           assert(a.id == 0, \"a.id\")\n\
           assert(b.id == 1, \"b.id\")\n\
           assert(a.next_addr == 0x0500 + {bytes}, \"a.next_addr\")\n\
           assert(b.addr == a.next_addr, \"b.addr\")\n\
           snare_id = b.id\n\
         end\n\
         {frame}",
        bytes = SAMPLE_BYTES,
        frame = voice0_frame("snare_id", 0),
    );

    assert!(e.set_source(&program).is_ok());
    assert!(e.frame(0.0, 0).is_ok());
    assert!(!is_silent(e.audio()), "keyed voice should sound");

    let start_a = 0x0500u32;
    let end_a = start_a + SAMPLE_BYTES as u32;
    let end_b = end_a + SAMPLE_BYTES as u32;
    let expected = vec![
        DspSampleView {
            id: 0,
            name: "kick".to_string(),
            start: start_a,
            end: end_a,
        },
        DspSampleView {
            id: 1,
            name: "snare".to_string(),
            start: end_a,
            end: end_b,
        },
    ];
    assert_eq!(e.dsp_view().samples, expected);
}

#[test]
fn dma_with_no_opts_defaults_to_the_reserved_page_end() {
    let mut e = LuaEngine::new();
    add_sine(&mut e, "kick");
    let program = "function init()\n\
                     local kick = dma(\"kick\")\n\
                     assert(kick.addr == 0x0500, \"kick.addr\")\n\
                   end\n\
                   function frame(t, f)\nend\n";
    assert!(e.set_source(program).is_ok());
}

#[test]
fn placement_rejects_the_echo_region() {
    let mut e = LuaEngine::new();
    add_sine(&mut e, "kick");
    let program = "function init()\n\
                     dsp.echo.delay = 4\n\
                     dma(\"kick\", { addr = 0xdf80 })\n\
                   end\n\
                   function frame(t, f)\nend\n";
    let err = e.set_source(program).unwrap_err();
    assert!(
        err.message.contains("echo region"),
        "message was: {}",
        err.message
    );
    assert!(
        err.message.contains("0xe000"),
        "message was: {}",
        err.message
    );

    // Compile-time check: `dsp.echo.delay` changing AFTER the dma() call
    // (inside init(), post-placement) must still be caught — not just the
    // value it held at dma() call time.
    let mut e2 = LuaEngine::new();
    add_sine(&mut e2, "kick");
    let program2 = "local pad = dma(\"kick\", { addr = 0xd000 })\n\
                     function init()\n\
                       dsp.echo.delay = 8\n\
                     end\n\
                     function frame(t, f)\n\
                     end\n";
    let err2 = e2.set_source(program2).unwrap_err();
    assert!(
        err2.message.contains("echo region"),
        "message was: {}",
        err2.message
    );
    assert!(
        err2.message.contains("0xc000"),
        "message was: {}",
        err2.message
    );
}

#[test]
fn placement_rejects_the_directory_page() {
    let mut e = LuaEngine::new();
    add_sine(&mut e, "kick");
    let program = "function init()\n\
                     dma(\"kick\", { addr = 0x0200 })\n\
                   end\n\
                   function frame(t, f)\nend\n";
    let err = e.set_source(program).unwrap_err();
    assert!(
        err.message.contains("directory page"),
        "message was: {}",
        err.message
    );
}

#[test]
fn placement_rejects_overflow() {
    let mut e = LuaEngine::new();
    add_sine(&mut e, "kick");
    let program = "function init()\n\
                     dma(\"kick\", { addr = 0xfff0 })\n\
                   end\n\
                   function frame(t, f)\nend\n";
    let err = e.set_source(program).unwrap_err();
    assert!(
        err.message.contains("sound RAM"),
        "message was: {}",
        err.message
    );
}

#[test]
fn placement_rejects_sample_overlap() {
    let mut e = LuaEngine::new();
    add_sine(&mut e, "kick");
    add_sine(&mut e, "snare");
    let program = "function init()\n\
                     dma(\"kick\", { addr = 0x0500 })\n\
                     dma(\"snare\", { addr = 0x0500 })\n\
                   end\n\
                   function frame(t, f)\nend\n";
    let err = e.set_source(program).unwrap_err();
    assert!(
        err.message.contains("overlaps sample 'kick'"),
        "message was: {}",
        err.message
    );
}

#[test]
fn removed_sample_source_fails_loudly_like_graphics() {
    let mut e = LuaEngine::new();
    add_sine(&mut e, "kick");
    let program = format!(
        "function init()\n\
           dma(\"kick\", {{ addr = 0x0500 }})\n\
         end\n\
         {frame}",
        frame = voice0_frame("0", -1),
    );
    assert!(e.set_source(&program).is_ok());
    assert!(e.frame(0.0, 0).is_ok());

    assert!(e.remove_source("kick"));

    let err = e.frame(0.0, 1).unwrap_err();
    assert!(
        err.message.contains("no source named 'kick'"),
        "message was: {}",
        err.message
    );
}

#[test]
fn placed_sample_is_not_replayed_per_frame() {
    let mut e = LuaEngine::new();
    add_sine(&mut e, "kick");
    let program = format!(
        "function init()\n\
           dma(\"kick\", {{ addr = 0x0500 }})\n\
         end\n\
         function frame(t, f)\n\
           voice[0].sample = 0\n\
           voice[0].pitch = 0x1000\n\
           voice[0].vol.l = 127\n\
           voice[0].vol.r = 127\n\
           voice[0].adsr = {{a = 15, d = 0, s = 7, r = 0}}\n\
           dsp.mvol.l = 127\n\
           dsp.mvol.r = 127\n\
           if f == 0 then\n\
             for k = 0, {last} do\n\
               aram[0x0500 + k] = 0\n\
             end\n\
           end\n\
           if f == 1 then\n\
             kon(0)\n\
           end\n\
         end\n",
        last = SAMPLE_BYTES - 1,
    );
    assert!(e.set_source(&program).is_ok());
    assert!(e.frame(0.0, 0).is_ok());
    assert!(e.frame(0.0, 1).is_ok());
    assert!(
        is_silent(e.audio()),
        "a per-frame replay would have restored the sample and produced sound"
    );
}

#[test]
fn sample_dma_rejects_graphics_opts() {
    let mut e = LuaEngine::new();
    add_sine(&mut e, "kick");
    let program = "function init()\n\
                     dma(\"kick\", { char = \"x\" })\n\
                   end\n\
                   function frame(t, f)\nend\n";
    let err = e.set_source(program).unwrap_err();
    assert!(
        err.message.contains("a sample source") && err.message.contains("only 'addr'"),
        "message was: {}",
        err.message
    );

    let mut e2 = LuaEngine::new();
    add_sine(&mut e2, "kick");
    let program2 = "function init()\n\
                      dma(\"kick\", { char = 0x2000 })\n\
                    end\n\
                    function frame(t, f)\nend\n";
    let err2 = e2.set_source(program2).unwrap_err();
    assert!(
        err2.message.contains("a sample source") && err2.message.contains("only 'addr'"),
        "message was: {}",
        err2.message
    );
}

#[test]
fn init_error_drops_recorded_sample_placements() {
    let mut e = LuaEngine::new();
    add_sine(&mut e, "kick");
    let program = "dma(\"kick\", {})\n\
                   function init()\n\
                     error(\"boom\")\n\
                   end\n\
                   function frame(t, f)\nend\n";
    assert!(e.set_source(program).is_err());
    assert!(e.dsp_view().samples.is_empty());
}

#[test]
fn dma_from_frame_is_an_error() {
    let mut e = LuaEngine::new();
    add_sine(&mut e, "kick");
    let program = "function frame(t, f)\n  dma(\"kick\")\nend\n";
    assert!(e.set_source(program).is_ok());
    let err = e.frame(0.0, 0).unwrap_err();
    assert!(
        err.message.contains("dma runs during setup"),
        "message was: {}",
        err.message
    );
}
