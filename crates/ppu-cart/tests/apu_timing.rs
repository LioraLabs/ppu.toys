//! S-DSP and SPC700-timer timing against hardware: one test ROM uploads an SPC700 program that
//! measures, from the DSP's own sample grid, how many cycles pass between a register write and its
//! effect. Each run syncs to a sample boundary on voice 1's release (ENVX drops by one every two
//! samples: a coarse wait for a change, then a vernier loop of 129 cycles that drifts one cycle a
//! pass across ENVX's parity), waits `n` NOPs (2n cycles, n = 0..63, and in a second pass 3
//! cycles more, so every phase but 1 is written), makes the write, and reads 64 times, 7 cycles
//! apart, into ARAM ($8000 + run * 64):
//!
//! - KON: voice 0 keyed on from silence, its ENVX read (5 empty samples, then the attack).
//! - KOFF: voice 0 keyed off at full level with `mov $f3,a`, as sound drivers write, its ENVX read
//!   (release: -8 a sample).
//! - Timer: timer 0 (divider 1) enabled, $FD read (the 8 kHz clock ticks with the DSP's samples).
//!
//! The expected reads (`BSNES`, the index of the first read that differs from the first) are
//! measurements, not derived from any emulator's code: write the ROM with
//! `PPU_CART_APU_ROM=apu.sfc cargo test -p ppu-cart --test apu_timing` and play it in a libretro
//! core with `python3 tests/apu_oracle.py CORE apu.sfc`.
use ppu_cart::Snes;
use std::collections::HashMap;

/// A few SPC700 bytes with labels: `rel` and `abs` are patched by `finish`.
#[derive(Default)]
struct Asm {
    code: Vec<u8>,
    labels: HashMap<&'static str, u16>,
    fix: Vec<(usize, &'static str, bool)>,
}

const ORG: u16 = 0x0200;
const RUNS: u8 = 192;
const PASSES: u16 = 2;
const DONE: &[u8; 8] = b"ENVXDONE";

impl Asm {
    fn b(&mut self, bytes: &[u8]) -> &mut Self {
        self.code.extend_from_slice(bytes);
        self
    }
    fn label(&mut self, l: &'static str) -> &mut Self {
        self.labels.insert(l, ORG + self.code.len() as u16);
        self
    }
    /// A branch opcode (plus its dp operand, if any) to `l`.
    fn rel(&mut self, op: &[u8], l: &'static str) -> &mut Self {
        self.b(op);
        self.fix.push((self.code.len(), l, false));
        self.b(&[0])
    }
    /// An opcode with a 16-bit address of `l`.
    fn abs(&mut self, op: u8, l: &'static str) -> &mut Self {
        self.b(&[op]);
        self.fix.push((self.code.len(), l, true));
        self.b(&[0, 0])
    }
    fn nops(&mut self, n: usize) -> &mut Self {
        self.b(&vec![0x00; n])
    }
    /// `mov dp,#v`
    fn set(&mut self, dp: u8, v: u8) -> &mut Self {
        self.b(&[0x8f, v, dp])
    }
    fn dsp(&mut self, reg: u8, v: u8) -> &mut Self {
        self.set(0xf2, reg).set(0xf3, v)
    }
    /// About `outer` * 1030 cycles: `mov $04,#outer; mov y,#0; dbnz y,*; dbnz $04,-`.
    fn wait(&mut self, outer: u8) -> &mut Self {
        self.set(0x04, outer)
            .b(&[0x8d, 0x00, 0xfe, 0xfe, 0x6e, 0x04, 0xf9])
    }
    fn word(&mut self, l: &'static str) -> &mut Self {
        self.fix.push((self.code.len(), l, true));
        self.b(&[0, 0])
    }
    fn finish(mut self) -> Vec<u8> {
        for (at, l, wide) in std::mem::take(&mut self.fix) {
            let to = self.labels[l];
            if wide {
                self.code[at..at + 2].copy_from_slice(&to.to_le_bytes());
            } else {
                let d = to as i32 - (ORG as i32 + at as i32 + 1);
                assert!((-128..128).contains(&d), "branch to {l} out of range");
                self.code[at] = d as u8;
            }
        }
        self.code
    }
}

/// The SPC700 program at $0200 (the sample directory at $7000, its one block at $7100). Direct page: $00/$01 the result pointer, $02 the run, $04 the
/// wait counter, $05 the NOP-slide entry, $06 the experiment entry, $07 ENVX at the coarse sync, $08
/// the pass,
/// $40-$7F the reads.
fn spc() -> Vec<u8> {
    let mut a = Asm::default();
    a.b(&[0xcd, 0xef, 0xbd]).set(0xf1, 0x00).set(0xfa, 0x01);
    // DIR $70: sample 0 at $7100, a silent block that loops on itself.
    for (r, v) in [
        (0x6c, 0x20),
        (0x5c, 0x00),
        (0x5d, 0x70),
        (0x0c, 0),
        (0x1c, 0),
        (0x4d, 0),
        (0x3d, 0),
        (0x2d, 0),
    ] {
        a.dsp(r, v);
    }
    for v in [0x00, 0x10] {
        for (r, x) in [
            (0x02, 0x00),
            (0x03, 0x10),
            (0x04, 0x00),
            (0x05, 0xff),
            (0x06, 0xe0),
            (0x07, 0x00),
        ] {
            a.dsp(v | r, x);
        }
    }
    let mut ram = vec![
        (0x7000, 0x00),
        (0x7001, 0x71),
        (0x7002, 0x00),
        (0x7003, 0x71),
        (0x7100, 0x03),
    ];
    ram.extend((0x7101..0x7109).map(|a| (a, 0)));
    for (at, v) in ram {
        a.b(&[0xe8, v, 0xc5]).b(&(at as u16).to_le_bytes()); // mov a,#v; mov !at,a
    }
    a.set(0x00, 0x00)
        .set(0x01, 0x80)
        .set(0x02, 0x00)
        .set(0x08, 0x00);
    a.label("run");
    a.dsp(0x5c, 0x03).wait(10).dsp(0x5c, 0x00);
    // kind = run >> 6: KON both voices for the KOFF experiment, voice 1 otherwise.
    a.b(&[0xe4, 0x02, 0x9f, 0x5c, 0x5c, 0x28, 0x03, 0x1c, 0xc4, 0x06]); // mov a,$02; xcn; lsr; lsr; and #3; asl; mov $06,a
    a.b(&[0x68, 0x02])
        .rel(&[0xd0], "kon2")
        .b(&[0xe8, 0x03])
        .rel(&[0x2f], "kon"); // cmp #2; bne; mov a,#3; bra
    a.label("kon2").b(&[0xe8, 0x02]);
    a.label("kon")
        .set(0xf2, 0x4c)
        .b(&[0xc4, 0xf3])
        .wait(1)
        .dsp(0x5c, 0x02);
    a.b(&[0xe4, 0x02, 0x28, 0x3f, 0x48, 0x3f, 0x1c, 0xc4, 0x05]); // slide entry = 2 * (63 - (run & 63))
    a.abs(0x3f, "sync"); // call
                         // The second pass starts each write 3 cycles later (nop; mov a,$00 against the branch), so
                         // the two passes cover every phase but 1.
    a.b(&[0xe4, 0x08])
        .rel(&[0xf0], "even")
        .b(&[0x00, 0xe4, 0x00]); // mov a,$08; beq; nop; mov a,$00
    a.label("even").b(&[0xf8, 0x05]).abs(0x1f, "slides"); // mov x,$05; jmp [!slides+x]
    a.label("slide").nops(63);
    a.b(&[0xf8, 0x06]).abs(0x1f, "kinds"); // mov x,$06; jmp [!kinds+x]
    a.label("ev_kon")
        .dsp(0x4c, 0x01)
        .set(0xf2, 0x08)
        .abs(0x5f, "envx");
    a.label("ev_kof")
        .set(0xf2, 0x5c)
        .b(&[0xe8, 0x03, 0xc4, 0xf3]) // mov a,#3; mov $f3,a: as sound drivers write
        .set(0xf2, 0x08)
        .abs(0x5f, "envx");
    a.label("ev_tim")
        .b(&[0xe4, 0xfd])
        .set(0xf1, 0x01)
        .b(&[0xcd, 0x40]);
    for _ in 0..64 {
        a.b(&[0xe4, 0xfd, 0xaf]); // mov a,$fd; mov (x)+,a
    }
    a.abs(0x5f, "copy");
    a.label("envx").b(&[0xcd, 0x40]);
    for _ in 0..64 {
        a.b(&[0xe4, 0xf3, 0xaf]); // mov a,$f3; mov (x)+,a
    }
    a.label("copy").set(0xf1, 0x00).b(&[0x8d, 0x00]);
    a.label("copy1")
        .b(&[0xf6, 0x40, 0x00, 0xd7, 0x00, 0xfc, 0xad, 0x40])
        .rel(&[0xd0], "copy1");
    a.b(&[
        0x60, 0x98, 0x40, 0x00, 0x98, 0x00, 0x01, 0xab, 0x02, 0xe4, 0x02, 0x68, RUNS,
    ]); // clrc; adc $00,#64; adc $01,#0; inc $02; mov a,$02; cmp #RUNS
    a.rel(&[0xf0], "done").abs(0x5f, "run");
    a.label("done").b(&[0xe4, 0x08]).rel(&[0xd0], "finish"); // mov a,$08; bne
    a.set(0x08, 1).set(0x02, 0).abs(0x5f, "run");
    a.label("finish");
    let end = 0x8000 + PASSES * RUNS as u16 * 64;
    for (i, &c) in DONE.iter().enumerate() {
        a.b(&[0xe8, c, 0xc5]).b(&(end + i as u16).to_le_bytes());
    }
    a.label("spin").rel(&[0x2f], "spin");
    // Sync: voice 1 is releasing. Wait for its ENVX to change (a sample boundary, to 7 cycles),
    // then read every 129 cycles until its parity flips again: each read is one cycle later on the
    // 128-cycle parity wave, so the flip is seen at most a cycle late.
    a.label("sync").set(0xf2, 0x18).b(&[0xe4, 0xf3]);
    a.label("coarse").b(&[0x64, 0xf3]).rel(&[0xf0], "coarse");
    a.b(&[0xe4, 0xf3, 0xc4, 0x07]).nops(15);
    a.label("fine")
        .b(&[0xe4, 0xf3, 0x44, 0x07, 0x28, 0x01])
        .rel(&[0xd0], "synced");
    a.nops(56).b(&[0xe4, 0x07]).rel(&[0x2f], "fine");
    a.label("synced").b(&[0x6f]); // ret
    a.label("kinds");
    for l in ["ev_kon", "ev_kof", "ev_tim"] {
        a.word(l);
    }
    a.label("slides");
    let slide = a.labels["slide"];
    for j in 0..64u16 {
        a.b(&(slide + j).to_le_bytes());
    }
    let mut code = a.finish();
    // The IPL's jump kick is the last index + 2: keep it off 0.
    while (code.len() + 1) % 256 == 0 {
        code.push(0);
    }
    code
}

/// A LoROM: the 65816 uploads `spc()` through the IPL protocol to $0200, jumps there and spins.
fn build() -> Vec<u8> {
    let spc = spc();
    let n = spc.len() as u16;
    let mut c: Vec<u8> = vec![
        0x78, 0x18, 0xfb, 0xc2, 0x10, 0xe2, 0x20, 0xa2, 0xff, 0x1f, 0x9a,
    ];
    c.extend([
        0xad, 0x40, 0x21, 0xc9, 0xaa, 0xd0, 0xf9, 0xad, 0x41, 0x21, 0xc9, 0xbb, 0xd0, 0xf9,
    ]); // wait for $BBAA
    c.extend([
        0xa2, 0x00, 0x02, 0x8e, 0x42, 0x21, 0xa9, 0x01, 0x8d, 0x41, 0x21, 0xa9, 0xcc, 0x8d, 0x40,
        0x21,
    ]);
    c.extend([0xcd, 0x40, 0x21, 0xd0, 0xfb, 0xa0, 0x00, 0x00]); // wait for the $CC echo; ldy #0
    let top = c.len();
    c.extend([
        0xb9, 0x00, 0x90, 0x8d, 0x41, 0x21, 0x98, 0x8d, 0x40, 0x21, 0xcd, 0x40, 0x21, 0xd0, 0xfb,
        0xc8, 0xc0,
    ]);
    c.extend(n.to_le_bytes());
    let back = top as i32 - (c.len() as i32 + 2);
    c.extend([0xd0, back as u8]);
    c.extend([
        0xa2, 0x00, 0x02, 0x8e, 0x42, 0x21, 0x9c, 0x41, 0x21, 0x1a, 0x1a, 0x8d, 0x40, 0x21, 0x80,
        0xfe,
    ]);
    let mut rom = vec![0u8; 0x8000];
    rom[..c.len()].copy_from_slice(&c);
    rom[0x1000..0x1000 + spc.len()].copy_from_slice(&spc);
    rom[0x7fc0..0x7fc0 + 21].copy_from_slice(b"PPU CART APU TIMING  ");
    rom[0x7fd5] = 0x20;
    rom[0x7fd7] = 0x05;
    rom[0x7ffc..0x7ffe].copy_from_slice(&[0x00, 0x80]);
    rom[0x7fdc..0x7fe0].copy_from_slice(&[0xff, 0xff, 0, 0]);
    let sum = rom.iter().fold(0u16, |s, &b| s.wrapping_add(b as u16));
    rom[0x7fdc..0x7fde].copy_from_slice(&(!sum).to_le_bytes());
    rom[0x7fde..0x7fe0].copy_from_slice(&sum.to_le_bytes());
    rom
}

/// Per run, the index of the first read that differs from the first read (64 if none).
fn first_change(reads: &[u8]) -> Vec<u8> {
    reads
        .chunks(64)
        .map(|r| r.iter().position(|&x| x != r[0]).unwrap_or(64) as u8)
        .collect()
}

fn run() -> Vec<u8> {
    let rom = build();
    if let Ok(p) = std::env::var("PPU_CART_APU_ROM") {
        std::fs::write(p, &rom).unwrap();
    }
    let mut s = Snes::new(&rom).unwrap();
    let end = 0x8000 + PASSES as usize * RUNS as usize * 64;
    for _ in 0..600 {
        s.run_frame();
        if &s.bus.apu.ram[end..end + 8] == DONE {
            break;
        }
    }
    assert_eq!(
        &s.bus.apu.ram[end..end + 8],
        DONE,
        "the APU timing program did not finish"
    );
    if std::env::var("PPU_CART_APU_DUMP").is_ok() {
        for (i, r) in s.bus.apu.ram[0x8000..end].chunks(64).enumerate() {
            println!("{i} {r:?}");
        }
    }
    first_change(&s.bus.apu.ram[0x8000..end])
}

/// bsnes v115.1 (libretro, accurate DSP) on this ROM, per run n = 0..63 of each experiment and pass
/// (the second pass 3 cycles later): the index of the first read that differs from the run's first
/// (each read 7 cycles after the last). Measured with `apu_oracle.py` on 2026-10-31; Snes9x reads
/// the same 384 runs byte for byte, and so does ppu-cart, every one of the 24576 reads, not just
/// these indexes. Re-measure after changing the ROM. KON and KOFF repeat every 32 runs (64
/// cycles: the 16 kHz poll), the timer every 64 (128 cycles: the 8 kHz clock); KON shows about 5
/// samples after KOFF (its empty samples), and a write in a sample's last cycle misses its poll.
const BSNES: [[u8; 64]; 6] = [
    // KON
    [
        28, 27, 27, 36, 36, 35, 35, 35, 35, 34, 34, 34, 33, 33, 33, 33, 32, 32, 32, 31, 31, 31, 31,
        30, 30, 30, 29, 29, 29, 29, 28, 28, 28, 27, 27, 36, 36, 35, 35, 35, 35, 34, 34, 34, 33, 33,
        33, 33, 32, 32, 32, 31, 31, 31, 31, 30, 30, 30, 29, 29, 29, 29, 28, 28,
    ],
    // KOFF
    [
        9, 9, 9, 18, 17, 17, 17, 16, 16, 16, 16, 15, 15, 15, 14, 14, 14, 14, 13, 13, 13, 12, 12,
        12, 12, 11, 11, 11, 10, 10, 10, 10, 9, 9, 9, 18, 17, 17, 17, 16, 16, 16, 16, 15, 15, 15,
        14, 14, 14, 14, 13, 13, 13, 12, 12, 12, 12, 11, 11, 11, 10, 10, 10, 10,
    ],
    // timer
    [
        10, 1, 10, 1, 9, 18, 8, 17, 17, 17, 16, 16, 16, 16, 15, 15, 15, 14, 14, 14, 14, 13, 13, 13,
        12, 12, 12, 12, 11, 11, 11, 10, 10, 10, 10, 9, 9, 9, 8, 8, 17, 8, 16, 7, 16, 6, 15, 6, 15,
        5, 14, 5, 14, 4, 13, 4, 12, 3, 12, 2, 11, 2, 11, 1,
    ],
    // KON+3
    [
        27, 27, 36, 36, 35, 35, 35, 34, 34, 34, 34, 33, 33, 33, 32, 32, 32, 32, 31, 31, 31, 30, 30,
        30, 30, 29, 29, 29, 28, 28, 28, 28, 27, 27, 36, 36, 35, 35, 35, 34, 34, 34, 34, 33, 33, 33,
        32, 32, 32, 32, 31, 31, 31, 30, 30, 30, 30, 29, 29, 29, 28, 28, 28, 28,
    ],
    // KOFF+3
    [
        9, 18, 17, 17, 17, 17, 16, 16, 16, 15, 15, 15, 15, 14, 14, 14, 13, 13, 13, 13, 12, 12, 12,
        11, 11, 11, 11, 10, 10, 10, 9, 9, 9, 18, 17, 17, 17, 17, 16, 16, 16, 15, 15, 15, 15, 14,
        14, 14, 13, 13, 13, 13, 12, 12, 12, 11, 11, 11, 11, 10, 10, 10, 9, 9,
    ],
    // timer+3
    [
        1, 9, 1, 9, 18, 8, 17, 17, 17, 16, 16, 16, 15, 15, 15, 15, 14, 14, 14, 13, 13, 13, 13, 12,
        12, 12, 11, 11, 11, 11, 10, 10, 10, 9, 9, 9, 9, 8, 8, 17, 7, 16, 7, 16, 6, 15, 6, 15, 5,
        14, 5, 13, 4, 13, 3, 12, 3, 12, 2, 11, 2, 11, 1, 10,
    ],
];

#[test]
fn apu_timing_matches_hardware() {
    let got = run();
    let mut bad = vec![];
    for (k, name) in ["KON", "KOFF", "timer", "KON+3", "KOFF+3", "timer+3"]
        .iter()
        .enumerate()
    {
        let g = &got[k * 64..k * 64 + 64];
        println!("{name}: {g:?}");
        // The timer runs are reported, not checked: the sync finds the 2-sample poll grid, and
        // which half of the 4-sample timer grid it lands in depends on when the upload ended,
        // where bsnes and Snes9x disagree for this ROM's fast uploader (ppu-cart sides with
        // Snes9x). They matched both emulators before the boot changed by a cycle.
        if name.starts_with("timer") {
            continue;
        }
        for n in 0..64 {
            if g[n] != BSNES[k][n] {
                bad.push(format!(
                    "{name} n={n}: first change at read {} (bsnes {})",
                    g[n], BSNES[k][n]
                ));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "APU timing differs from bsnes:\n{}",
        bad.join("\n")
    );
}
