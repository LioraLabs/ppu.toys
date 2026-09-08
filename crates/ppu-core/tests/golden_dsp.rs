//! Golden S-DSP output compare — a hand-built BRR sample keyed on through the
//! ported core, byte-compared against a committed raw i16 fixture.
use ppu_core::Dsp;
use std::path::Path;

const GOLDEN: &str = "tests/fixtures/dsp_voice.bin";
const ECHO_GOLDEN: &str = "tests/fixtures/dsp_echo.bin";
// One 60 Hz video frame at 32 kHz: 32000/60 truncates to 533, but the engine
// actually alternates 532/533 samples per frame to stay on average; this
// fixture just renders a fixed 533 samples per "frame" for a stable byte count.
const FRAME_LEN: usize = 533;

/// Writes `blocks` BRR blocks starting at `base` (9 bytes each): filter 0,
/// shift 12, alternating nibbles (eight `0x7`s then eight `0x9`s) for a rough
/// square wave. Every block but the last uses header `0xc0`; the last uses
/// `last_header` (so callers can set loop+end, or end-without-loop).
fn build_sample(aram: &mut [u8; 0x10000], base: u16, blocks: usize, last_header: u8) {
    for b in 0..blocks {
        let addr = base as usize + b * 9;
        aram[addr] = if b == blocks - 1 { last_header } else { 0xc0 };
        aram[addr + 1] = 0x77;
        aram[addr + 2] = 0x77;
        aram[addr + 3] = 0x77;
        aram[addr + 4] = 0x77;
        aram[addr + 5] = 0x99;
        aram[addr + 6] = 0x99;
        aram[addr + 7] = 0x99;
        aram[addr + 8] = 0x99;
    }
}

/// Writes sample directory entry `index` at `0x0100 + index*4`: little-endian
/// start address, then little-endian loop address.
fn write_dir_entry(aram: &mut [u8; 0x10000], index: u8, start: u16, loop_addr: u16) {
    let addr = 0x0100 + (index as usize) * 4;
    aram[addr] = start as u8;
    aram[addr + 1] = (start >> 8) as u8;
    aram[addr + 2] = loop_addr as u8;
    aram[addr + 3] = (loop_addr >> 8) as u8;
}

/// Registers common to voice 0's fixture, minus KON (so `silent_when_no_key_on`
/// can reuse it without triggering playback).
fn setup_voice0(dsp: &mut Dsp) {
    dsp.write(0x5d, 0x01); // DIR = 0x01 -> directory base 0x0100
    dsp.write(0x04, 0x00); // voice0 SRCN = 0
    dsp.write(0x02, 0x00); // voice0 PITCH(L)
    dsp.write(0x03, 0x10); // voice0 PITCH(H) -> 0x1000 (1:1)
    dsp.write(0x05, 0x8f); // voice0 ADSR1
    dsp.write(0x06, 0xe0); // voice0 ADSR2
    dsp.write(0x00, 0x7f); // voice0 VOL(L)
    dsp.write(0x01, 0x7f); // voice0 VOL(R)
    dsp.write(0x0c, 0x7f); // MVOL(L)
    dsp.write(0x1c, 0x7f); // MVOL(R)
    dsp.write(0x6c, 0x20); // FLG: noise clock 0, echo writes disabled, unmuted
}

/// Builds a 6-block looping BRR sample at 0x0200 (directory entry 0), wires
/// voice 0 to it, and keys it on.
fn voice_fixture(aram: &mut [u8; 0x10000]) -> Dsp {
    build_sample(aram, 0x0200, 6, 0xc3); // last block: loop + end
    write_dir_entry(aram, 0, 0x0200, 0x0200);

    let mut dsp = Dsp::new();
    setup_voice0(&mut dsp);
    dsp.write(0x4c, 0x01); // KON voice 0
    dsp
}

/// Extends `voice_fixture` with an active echo path: buffer at 0xF000, 2 KB
/// (0xF000..0xF800), feedback, both EVOL channels, all 8 FIR taps set to
/// snes-apu's reset coefficients (c0..c7 = 0x80, 0xff, 0x9a, 0xff, 0x67,
/// 0xff, 0x0f, 0xff), voice 0 as an echo source, and echo writes enabled
/// (FLG bit5 clear). Voice 0's KON from `voice_fixture` is left as the only
/// KON write here — reissuing it would just re-trigger the voice.
fn echo_fixture(aram: &mut [u8; 0x10000]) -> Dsp {
    let mut dsp = voice_fixture(aram);
    dsp.write(0x6d, 0xf0); // ESA -> echo buffer at 0xF000
    dsp.write(0x7d, 0x01); // EDL = 1 -> 2 KB, 0xF000..0xF800
    dsp.write(0x0d, 0x40); // EFB
    dsp.write(0x2c, 0x40); // EVOL(L)
    dsp.write(0x3c, 0x40); // EVOL(R)
    dsp.write(0x0f, 0x80); // FIR c0
    dsp.write(0x1f, 0xff); // FIR c1
    dsp.write(0x2f, 0x9a); // FIR c2
    dsp.write(0x3f, 0xff); // FIR c3
    dsp.write(0x4f, 0x67); // FIR c4
    dsp.write(0x5f, 0xff); // FIR c5
    dsp.write(0x6f, 0x0f); // FIR c6
    dsp.write(0x7f, 0xff); // FIR c7
    dsp.write(0x4d, 0x01); // EON voice 0
    dsp.write(0x6c, 0x00); // FLG: echo writes enabled, unmuted
    dsp
}

/// Renders `n` frames of 533 stereo samples (1066 i16s each) into one Vec.
fn render_frames(dsp: &mut Dsp, aram: &mut [u8; 0x10000], n: usize) -> Vec<i16> {
    let mut out = vec![0i16; n * FRAME_LEN * 2];
    dsp.render(aram, &mut out);
    out
}

/// Byte-compares `out` against the committed golden at `path`, and sanity
/// checks it's actually audio (not all zero, with some real amplitude).
fn assert_matches_golden(out: &[i16], path: &str) {
    assert!(
        Path::new(path).exists(),
        "golden missing — run: cargo test -p ppu-core regen_golden -- --ignored"
    );
    let actual: Vec<u8> = out.iter().flat_map(|s| s.to_le_bytes()).collect();
    let expected = std::fs::read(path).unwrap();
    assert_eq!(actual, expected, "dsp output differs from golden");

    assert!(out.iter().any(|&s| s != 0), "output should not be all zero");
    let max_abs = out.iter().map(|&s| (s as i32).abs()).max().unwrap();
    assert!(
        max_abs > 1000,
        "max abs sample {max_abs} should exceed 1000"
    );
}

#[test]
fn voice_matches_golden() {
    let mut aram = [0u8; 0x10000];
    let mut dsp = voice_fixture(&mut aram);
    let out = render_frames(&mut dsp, &mut aram, 8);
    assert_matches_golden(&out, GOLDEN);
}

#[test]
fn echo_matches_golden() {
    let mut aram = [0u8; 0x10000];
    let mut dsp = echo_fixture(&mut aram);
    let out = render_frames(&mut dsp, &mut aram, 8);
    assert_matches_golden(&out, ECHO_GOLDEN);
}

#[test]
fn echo_writes_aram() {
    let mut aram = [0u8; 0x10000];
    let mut dsp = echo_fixture(&mut aram);
    render_frames(&mut dsp, &mut aram, 8);

    assert!(
        aram[0xF000..0xF800].iter().any(|&b| b != 0),
        "echo buffer region should contain non-zero bytes after render"
    );
    assert!(
        aram[0xEF00..0xF000].iter().all(|&b| b == 0),
        "bytes just before the echo region should be untouched"
    );
    assert!(
        aram[0xF800..0xF900].iter().all(|&b| b == 0),
        "bytes just after the echo region should be untouched"
    );
}

#[test]
fn echo_corrupts_sample_in_region() {
    let mut aram = [0u8; 0x10000];
    let mut dsp = echo_fixture(&mut aram);
    build_sample(&mut aram, 0xF100, 6, 0xc3); // second copy, only here to be clobbered
    let before = aram[0xF100..0xF100 + 54].to_vec();

    render_frames(&mut dsp, &mut aram, 8);

    assert_ne!(
        aram[0xF100..0xF100 + 54],
        before[..],
        "echo writes should corrupt a sample stored inside the echo region"
    );
}

#[test]
fn echo_corrupts_playback_of_sample_in_region() {
    fn run(flg: u8) -> Vec<i16> {
        let mut aram = [0u8; 0x10000];
        build_sample(&mut aram, 0xF100, 6, 0xc3); // sample living inside the echo buffer
        write_dir_entry(&mut aram, 1, 0xF100, 0xF100);

        let mut dsp = echo_fixture(&mut aram);
        dsp.write(0x00, 0x00); // voice0 VOL(L) = 0: inaudible directly...
        dsp.write(0x01, 0x00); // voice0 VOL(R) = 0: ...but still feeds echo via EON (from echo_fixture)
        dsp.write(0x2c, 0x00); // EVOL(L) = 0: echo not mixed into output
        dsp.write(0x3c, 0x00); // EVOL(R) = 0: only the echo unit's ARAM writes matter here

        dsp.write(0x14, 1); // voice1 SRCN = 1
        dsp.write(0x12, 0x00); // voice1 PITCH(L)
        dsp.write(0x13, 0x10); // voice1 PITCH(H) -> 0x1000 (1:1)
        dsp.write(0x15, 0x8f); // voice1 ADSR1
        dsp.write(0x16, 0xe0); // voice1 ADSR2
        dsp.write(0x10, 0x7f); // voice1 VOL(L)
        dsp.write(0x11, 0x7f); // voice1 VOL(R)
        dsp.write(0x4c, 0x02); // KON voice1 (voice0's KON was already applied by echo_fixture)

        dsp.write(0x6c, flg); // FLG: echo writes per `flg`, unmuted

        render_frames(&mut dsp, &mut aram, 8)
    }

    let echo_on = run(0x00); // FLG bit5 clear: echo writes enabled
    let echo_off = run(0x20); // FLG bit5 set: echo writes disabled

    assert!(
        echo_on.iter().any(|&s| s != 0),
        "echo-writes-on output should not be all zero"
    );
    assert!(
        echo_off.iter().any(|&s| s != 0),
        "echo-writes-off output should not be all zero"
    );
    assert_ne!(
        echo_on, echo_off,
        "echo writes clobbering voice 1's sample should audibly change its playback"
    );
}

#[test]
fn echo_golden_differs_from_voice_golden() {
    let voice = std::fs::read(GOLDEN).unwrap();
    let echo = std::fs::read(ECHO_GOLDEN).unwrap();
    assert_ne!(
        voice, echo,
        "echo golden should not be a no-op copy of the voice golden"
    );
}

#[test]
#[ignore = "regenerates the committed DSP goldens"]
fn regen_golden() {
    let mut aram = [0u8; 0x10000];
    let mut dsp = voice_fixture(&mut aram);
    let out = render_frames(&mut dsp, &mut aram, 8);
    let bytes: Vec<u8> = out.iter().flat_map(|s| s.to_le_bytes()).collect();
    std::fs::create_dir_all("tests/fixtures").unwrap();
    std::fs::write(GOLDEN, bytes).unwrap();

    let mut echo_aram = [0u8; 0x10000];
    let mut echo_dsp = echo_fixture(&mut echo_aram);
    let echo_out = render_frames(&mut echo_dsp, &mut echo_aram, 8);
    let echo_bytes: Vec<u8> = echo_out.iter().flat_map(|s| s.to_le_bytes()).collect();
    std::fs::write(ECHO_GOLDEN, echo_bytes).unwrap();
}

#[test]
fn voice_readback() {
    let mut aram = [0u8; 0x10000];
    let mut dsp = voice_fixture(&mut aram);
    let _ = render_frames(&mut dsp, &mut aram, 1);
    assert_ne!(dsp.read(0x08), 0, "ENVX should be nonzero after a render");
    assert_ne!(dsp.read(0x09), 0, "OUTX should be nonzero after a render");

    // A second, non-looping copy of the sample on voice 1. Pitch must be
    // nonzero or playback never advances past the first BRR block and the
    // end bit is never reached.
    build_sample(&mut aram, 0x0300, 6, 0xc1); // last block: end, no loop
    write_dir_entry(&mut aram, 1, 0x0300, 0x0300);
    dsp.write(0x14, 1); // voice1 SRCN = 1
    dsp.write(0x12, 0x00); // voice1 PITCH(L)
    dsp.write(0x13, 0x10); // voice1 PITCH(H) -> 0x1000 (1:1)
    dsp.write(0x4c, 0x02); // KON voice 1

    let _ = render_frames(&mut dsp, &mut aram, 1);
    assert_ne!(
        dsp.read(0x7c) & 0x02,
        0,
        "ENDX bit 1 should be set once voice 1 hits its end block"
    );

    dsp.write(0x7c, 0);
    assert_eq!(dsp.read(0x7c), 0, "writing ENDX should clear all bits");
}

#[test]
fn silent_when_no_key_on() {
    let mut aram = [0u8; 0x10000];
    build_sample(&mut aram, 0x0200, 6, 0xc3);
    write_dir_entry(&mut aram, 0, 0x0200, 0x0200);
    let mut dsp = Dsp::new();
    setup_voice0(&mut dsp); // no KON write

    let out = render_frames(&mut dsp, &mut aram, 2);
    assert!(out.iter().all(|&s| s == 0), "no KON should mean silence");
}

#[test]
fn kon_then_kof_before_render_is_silent() {
    let mut aram = [0u8; 0x10000];
    let mut dsp = voice_fixture(&mut aram); // KON voice 0
    dsp.write(0x5c, 0x01); // KOF voice 0, before any render

    let out = render_frames(&mut dsp, &mut aram, 2);
    assert!(
        out.iter().all(|&s| s == 0),
        "KON then KOF before render should stay silent"
    );
    assert_eq!(dsp.read(0x08), 0, "ENVX should be 0 after KON;KOF;render");
}

#[test]
fn kon_kof_kon_before_render_plays() {
    let mut aram = [0u8; 0x10000];
    let mut dsp = voice_fixture(&mut aram); // KON voice 0
    dsp.write(0x5c, 0x01); // KOF voice 0
    dsp.write(0x4c, 0x01); // KON voice 0 again, before any render

    let out = render_frames(&mut dsp, &mut aram, 2);
    assert!(
        out.iter().any(|&s| s != 0),
        "KON;KOF;KON before render should play"
    );
}

#[test]
fn negative_volume_inverts_phase() {
    let mut aram = [0u8; 0x10000];
    let mut dsp = voice_fixture(&mut aram);
    dsp.write(0x00, 0x81); // VOL(L) = -127
    dsp.write(0x01, 0x7f); // VOL(R) = +127

    let out = render_frames(&mut dsp, &mut aram, 2);
    assert!(out.iter().any(|&s| s != 0), "output should not be all zero");
    // A single floor-shift multiply_volume allows a 1-LSB gap between two
    // opposite-sign equal-magnitude scalings of the same value; this fixture
    // compounds two such stages (voice VOL, then master MVOL, both 0x7f
    // magnitude in `voice_fixture`), so the observed gap is up to 2 LSB.
    for pair in out.chunks(2) {
        let (l, r) = (pair[0] as i32, pair[1] as i32);
        assert!(
            (l + r).abs() <= 2,
            "left/right should be near-inverses with opposite-sign equal-magnitude volumes: l={l} r={r}"
        );
    }
}

#[test]
fn mute_flag_silences_output() {
    let mut aram = [0u8; 0x10000];
    let mut dsp = voice_fixture(&mut aram);
    dsp.write(0x6c, 0x60); // FLG: mute (bit6) + echo-write-disable (bit5)

    let out = render_frames(&mut dsp, &mut aram, 2);
    assert!(
        out.iter().all(|&s| s == 0),
        "FLG mute should silence output"
    );
}
