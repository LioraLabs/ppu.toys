//! CPU-side timing against hardware: one test ROM times a list of experiments with the H/V
//! counter latch, and the counts must match what bsnes (accurate profile) reports for the same
//! ROM. Each experiment syncs to a V-IRQ with WAI (I set, so no handler runs), latches the
//! counters ($2137), runs its body and latches again; the ROM stores both latches in work RAM
//! ($7E:1000, 8 bytes each). Covered: every memory region's speed, FastROM, idle cycles, DRAM
//! refresh, DMA and HDMA overheads, HDMA init, the short line, NMI and IRQ entry, the long dots.
//!
//! The expected numbers (`BSNES`) are measurements, not derived from any emulator's code: write
//! the ROM with `PPU_CART_TIMING_ROM=timing.sfc cargo test -p ppu-cart --test timing` and play it
//! in a libretro core with `python3 tests/timing_oracle.py CORE timing.sfc 96`.
use ppu_cart::Snes;

/// One experiment: code before the sync (setup), the timed body, code after the end latch.
struct Exp {
    name: String,
    line: u16,
    setup: Vec<u8>,
    body: Vec<u8>,
    after: Vec<u8>,
}

fn exp(name: impl Into<String>, line: u16, setup: &[u8], body: &[u8], after: &[u8]) -> Exp {
    Exp { name: name.into(), line, setup: setup.to_vec(), body: body.to_vec(), after: after.to_vec() }
}

fn rep(n: usize, code: &[u8]) -> Vec<u8> {
    code.repeat(n)
}

/// `ldx #n; dex; bne -3`: 16-bit X.
fn delay(n: u16) -> Vec<u8> {
    vec![0xa2, n as u8, (n >> 8) as u8, 0xca, 0xd0, 0xfd]
}

/// `lda #v; sta addr`, or `stz addr` for 0.
fn store(addr: u16, v: u8) -> Vec<u8> {
    if v == 0 {
        vec![0x9c, addr as u8, (addr >> 8) as u8]
    } else {
        vec![0xa9, v, 0x8d, addr as u8, (addr >> 8) as u8]
    }
}

fn stores(list: &[(u16, u8)]) -> Vec<u8> {
    list.iter().flat_map(|&(a, v)| store(a, v)).collect()
}

/// Latch the counters and store H lo/hi, V lo/hi at $7E:slot (`,x` when `indexed`).
fn latch(slot: u16, indexed: bool) -> Vec<u8> {
    let mut c = vec![0xad, 0x37, 0x21, 0xad, 0x3f, 0x21];
    for (i, reg) in [0x3c, 0x3c, 0x3d, 0x3d].into_iter().enumerate() {
        let a = slot + i as u16;
        c.extend([0xad, reg, 0x21, if indexed { 0x9f } else { 0x8f }, a as u8, (a >> 8) as u8, 0x7e]);
    }
    c
}

/// DMA channel 0: `n` bytes from ROM $00:8000 to the WRAM port ($2180, address $7F:0000).
fn dma0(n: u8) -> Vec<u8> {
    stores(&[(0x2181, 0), (0x2182, 0), (0x2183, 1), (0x4300, 0), (0x4301, 0x80), (0x4302, 0), (0x4303, 0x80), (0x4304, 0), (0x4305, n), (0x4306, 0)])
}

/// HDMA channel `ch` to $21FF (nothing there): `dmap`, table at ROM `table`, indirect bank 0.
fn hdma(ch: u16, dmap: u8, table: u16) -> Vec<u8> {
    let b = 0x4300 + ch * 16;
    stores(&[(b, dmap), (b + 1, 0xff), (b + 2, table as u8), (b + 3, (table >> 8) as u8), (b + 4, 0), (b + 7, 0)])
}

const NMI_SLOT: u16 = 0x400;  // handler latches: $7E:1400 + 8i
const TABLES: u16 = 0xc000;

/// The ROM's HDMA tables at $00:C000, by name: (address, the DMAP to use).
fn tables() -> (Vec<u8>, Vec<(&'static str, u16, u8)>) {
    let mut t = vec![];
    let mut at = vec![];
    let mut add = |name, dmap: u8, bytes: Vec<u8>| {
        at.push((name, TABLES + t.len() as u16, dmap));
        t.extend(bytes);
        t.push(0);
    };
    // Direct: `lines` per entry (bit 7 = repeat), `unit` bytes a transfer, enough entries for 250 lines.
    let direct = |count: u8, unit: usize| {
        let n = (count & 0x7f) as usize;
        let per = if count & 0x80 != 0 { n * unit } else { unit };
        (0..250usize.div_ceil(n)).flat_map(|_| std::iter::once(count).chain(std::iter::repeat_n(0, per))).collect::<Vec<u8>>()
    };
    add("repeat", 0, direct(0xff, 1));
    add("reload each line", 0, direct(1, 1));
    add("reload every 2 lines", 0, direct(2, 1));
    add("no repeat", 0, direct(0x7f, 1));
    add("repeat 2 bytes", 1, direct(0xff, 2));
    add("repeat 4 bytes", 4, direct(0xff, 4));
    add("reload each line 4 bytes", 4, direct(1, 4));
    add("ended", 0, vec![]);
    // Indirect: data at $00:8000.
    add("indirect reload each line", 0x40, (0..250).flat_map(|_| [1, 0x00, 0x80]).collect());
    add("indirect repeat", 0x40, (0..2).flat_map(|_| [0xff, 0x00, 0x80]).collect());
    (t, at)
}

/// The table named `name`: (address, dmap).
fn table(name: &str) -> (u16, u8) {
    let (_, at) = tables();
    let &(_, a, d) = at.iter().find(|x| x.0 == name).unwrap();
    (a, d)
}

fn experiments() -> Vec<Exp> {
    let fast_on: &[u8] = &[0xa9, 1, 0x8d, 0x0d, 0x42, 0x5c]; // lda #1; sta $420d; jml (operand patched)
    let nop = [0xea];
    let mut e = vec![
        exp("empty", 10, &[], &[], &[]),
        exp("64 nop (slow ROM)", 10, &[], &rep(64, &nop), &[]),
        exp("64 xba (2 idle)", 10, &[], &rep(64, &[0xeb]), &[]),
        exp("64 lda $0000 (WRAM)", 10, &[], &rep(64, &[0xad, 0x00, 0x00]), &[]),
        exp("64 lda $21ff (B bus)", 10, &[], &rep(64, &[0xad, 0xff, 0x21]), &[]),
        exp("64 lda $4000 (slow I/O)", 10, &[], &rep(64, &[0xad, 0x00, 0x40]), &[]),
        exp("64 lda $41ff (slow I/O)", 10, &[], &rep(64, &[0xad, 0xff, 0x41]), &[]),
        exp("64 lda $4300 (DMA regs)", 10, &[], &rep(64, &[0xad, 0x00, 0x43]), &[]),
        exp("64 lda $5fff (fast)", 10, &[], &rep(64, &[0xad, 0xff, 0x5f]), &[]),
        exp("64 lda $6000 (expansion)", 10, &[], &rep(64, &[0xad, 0x00, 0x60]), &[]),
        exp("64 lda $8000 (ROM)", 10, &[], &rep(64, &[0xad, 0x00, 0x80]), &[]),
        exp("64 lda $7e0000 (long WRAM)", 10, &[], &rep(64, &[0xaf, 0x00, 0x00, 0x7e]), &[]),
        exp("64 lda $808000 (no FastROM)", 10, &[], &rep(64, &[0xaf, 0x00, 0x80, 0x80]), &[]),
        exp("64 lda $808000 (FastROM)", 10, &store(0x420d, 1), &rep(64, &[0xaf, 0x00, 0x80, 0x80]), &store(0x420d, 0)),
        exp("64 lda $c08000 (FastROM)", 10, &store(0x420d, 1), &rep(64, &[0xaf, 0x00, 0x80, 0xc0]), &store(0x420d, 0)),
        exp("64 lda $408000 (FastROM)", 10, &store(0x420d, 1), &rep(64, &[0xaf, 0x00, 0x80, 0x40]), &store(0x420d, 0)),
        // FastROM code: the body runs in bank $80 (`fast_on` jumps there, see `build`).
        exp("64 nop (FastROM code)", 10, fast_on, &rep(64, &nop), &[]),
        exp("delay 1000 (refresh)", 10, &[], &delay(1000), &[]),
        // Ends latched around the long dots 323 and 327.
        exp("55 nop", 10, &[], &rep(55, &nop), &[]),
        exp("56 nop", 10, &[], &rep(56, &nop), &[]),
        exp("57 nop", 10, &[], &rep(57, &nop), &[]),
        exp("58 nop", 10, &[], &rep(58, &nop), &[]),
        exp("59 nop", 10, &[], &rep(59, &nop), &[]),
        exp("60 nop", 10, &[], &rep(60, &nop), &[]),
        exp("delay 1 (empty loop)", 10, &[], &delay(1), &[]),
    ];
    for k in 0..4 {
        let pad = rep(k, &nop);
        e.push(exp(["nops 0", "nops 1", "nops 2", "nops 3"][k], 10, &[], &pad, &[]));
        let mut b = pad.clone();
        b.extend(store(0x420b, 1));
        e.push(exp(["dma 1 byte, 0 nops", "dma 1 byte, 1 nop", "dma 1 byte, 2 nops", "dma 1 byte, 3 nops"][k], 10, &dma0(1), &b, &[]));
    }
    e.push(exp("dma 16 bytes", 10, &dma0(16), &store(0x420b, 1), &[]));
    e.push(exp("dma 200 bytes", 10, &dma0(200), &store(0x420b, 1), &[]));
    let mut two = dma0(1);
    two.extend(stores(&[(0x4310, 0), (0x4311, 0x80), (0x4312, 0), (0x4313, 0x80), (0x4314, 0), (0x4315, 1), (0x4316, 0)]));
    e.push(exp("dma 2 channels x 1 byte", 10, &two, &store(0x420b, 3), &[]));
    e.push(exp("sta $420b #0 (no DMA)", 10, &[], &[0xa9, 0, 0x8d, 0x0b, 0x42], &[]));
    // HDMA over about 130 lines of a delay loop, in SlowROM (bank $00) and FastROM (bank $80) code.
    let hdma_runs: &[&[&str]] = &[
        &[],
        &["repeat"],
        &["reload each line"],
        &["reload every 2 lines"],
        &["no repeat"],
        &["ended"],
        &["repeat 2 bytes"],
        &["repeat 4 bytes"],
        &["reload each line 4 bytes"],
        &["indirect repeat"],
        &["indirect reload each line"],
        &["repeat", "repeat"],
        &["reload each line", "reload each line"],
        &["repeat", "repeat", "repeat", "repeat"],
        &["reload each line", "reload each line", "reload each line", "reload each line", "reload each line", "reload each line"],
    ];
    for fast in [false, true] {
        for run in hdma_runs {
            let mut s = vec![];
            let mut mask = 0;
            for (k, name) in run.iter().enumerate() {
                let (t, dmap) = table(name);
                s.extend(hdma(k as u16 + 1, dmap, t));
                mask |= 2 << k;
            }
            s.extend(store(0x420c, mask));
            if fast {
                s.extend(fast_on);
            }
            let name = format!("hdma {} x {} ({})", run.len(), run.first().unwrap_or(&"-"), if fast { "FastROM" } else { "SlowROM" });
            e.push(exp(name, 10, &s, &delay(5000), &store(0x420c, 0)));
        }
    }
    // DMA: 1-4 bytes after 0-3 nops (four phases mod 8), and the same on the next line.
    for line in [10, 11] {
        for bytes in 1..=4u8 {
            for k in 0..4 {
                if line == 11 && bytes > 1 {
                    continue;
                }
                let mut b = rep(k, &nop);
                b.extend(store(0x420b, 1));
                e.push(exp(format!("dma {bytes} bytes after {k} nops, line {line}"), line, &dma0(bytes), &b, &[]));
            }
        }
    }
    let (t, _) = table("repeat");
    let (ti, _) = table("indirect reload each line");
    // Frame start: line 260 through line 1 (HDMA init at V=0).
    e.push(exp("frame start, no hdma", 260, &[], &delay(200), &[]));
    let mut s = hdma(1, 0, t);
    s.extend(store(0x420c, 2));
    e.push(exp("frame start, hdma init", 260, &s, &delay(200), &store(0x420c, 0)));
    let mut s = hdma(1, 0x40, ti);
    s.extend(store(0x420c, 2));
    e.push(exp("frame start, hdma indirect init", 260, &s, &delay(200), &store(0x420c, 0)));
    // The short line: line 238 to 242 in two frames in a row (one of them has a short line 240).
    e.push(exp("lines 238-242 a", 238, &[], &delay(150), &[]));
    e.push(exp("lines 238-242 b", 238, &[], &delay(150), &[]));
    // NMI: enable at line 224 and WAI (the handler latches), or spin on a flag the handler sets.
    let mut wai = vec![0xa2, 0, 0]; // ldx #slot (patched in build)
    wai.extend(store(0x4200, 0x80));
    wai.push(0xcb);
    e.push(exp("nmi from wai", 224, &[], &wai, &[]));
    for (name, spin) in [("nmi in nop spin", vec![0xea, 0xa5, 0x20, 0xf0, 0xfb]), ("nmi in lda spin", vec![0xa5, 0x20, 0xf0, 0xfc])] {
        let mut b = vec![0xa2, 0, 0, 0x64, 0x20]; // ldx #slot; stz $20
        b.extend(store(0x4200, 0x80));
        b.extend(spin);
        e.push(exp(name, 224, &[], &b, &[]));
    }
    // H/V IRQ at H=100 of line 20, handled (cli): the handler latches like the NMI one.
    let mut b = vec![0xa2, 0, 0, 0x64, 0x20];
    b.extend(stores(&[(0x4207, 100), (0x4208, 0), (0x4209, 20), (0x420a, 0), (0x4200, 0x30)]));
    b.extend([0x58, 0xa5, 0x20, 0xf0, 0xfc, 0x78]); // cli; spin; sei
    e.push(exp("h/v irq in lda spin", 10, &[], &b, &[]));
    e
}

/// Written to $7E:17F8 when every experiment ran (the oracle finds work RAM by it in save states).
const DONE: &[u8; 8] = b"TIMEDONE";

/// Assemble the ROM: experiments in order, then the done marker and a spin.
fn build() -> Vec<u8> {
    let mut rom = vec![0u8; 0x8000];
    let mut c: Vec<u8> = vec![
        0x78, 0x18, 0xfb, // sei; clc; xce
        0xc2, 0x10, 0xe2, 0x20, // rep #$10; sep #$20
        0xa2, 0xff, 0x1f, 0x9a, // ldx #$1fff; txs
    ];
    c.extend(stores(&[(0x4200, 0), (0x2100, 0x80), (0x420c, 0), (0x420d, 0), (0x4201, 0xff)]));
    let at = |c: &Vec<u8>| 0x8000 + c.len() as u32;
    for (i, e) in experiments().iter().enumerate() {
        let slot = 0x1000 + i as u16 * 8;
        let mut setup = e.setup.clone();
        let fast = setup.ends_with(&[0x5c]);
        if fast {
            // jml $80:<next>
            let next = at(&c) + setup.len() as u32 + 3 | 0x80_0000;
            setup.extend([next as u8, (next >> 8) as u8, (next >> 16) as u8]);
        }
        c.extend(setup);
        c.extend(stores(&[(0x4209, e.line as u8), (0x420a, (e.line >> 8) as u8), (0x4200, 0x20)]));
        c.extend([0xcb, 0xad, 0x11, 0x42]); // wai; lda $4211
        c.extend(store(0x4200, 0));
        c.extend(latch(slot, false));
        let mut body = e.body.clone();
        if body.first() == Some(&0xa2) && body[1] == 0 && body[2] == 0 {
            body[1] = (NMI_SLOT + slot) as u8;
            body[2] = ((NMI_SLOT + slot) >> 8) as u8;
        }
        c.extend(body);
        c.extend(latch(slot + 4, false));
        c.extend(&e.after);
        if fast {
            let next = at(&c) + 4;
            c.extend([0x5c, next as u8, (next >> 8) as u8, 0x00]); // jml $00:<next>
            c.extend(store(0x420d, 0));
        }
    }
    for (i, &b) in DONE.iter().enumerate() {
        c.extend([0xa9, b, 0x8f, 0xf8 + i as u8, 0x17, 0x7e]); // the done marker at $7E:17F8
    }
    c.extend([0x80, 0xfe]); // bra *
    // NMI and IRQ handler: latch to $70:(x), set the flag at $20, stop interrupts, return.
    let handler = at(&c) as u16;
    c.extend(latch(0, true));
    c.extend([0xe6, 0x20, 0xad, 0x11, 0x42, 0x9c, 0x00, 0x42, 0x40]); // inc $20; lda $4211; stz $4200; rti
    assert!(c.len() < 0x4000, "code overruns the tables");
    rom[..c.len()].copy_from_slice(&c);
    let (t, _) = tables();
    rom[0x4000..0x4000 + t.len()].copy_from_slice(&t);
    rom[0x7fc0..0x7fc0 + 21].copy_from_slice(b"PPU CART TIMING      ");
    rom[0x7fd5] = 0x30; // LoROM, FastROM-capable
    rom[0x7fd6] = 0x02; // ROM + SRAM
    rom[0x7fd7] = 0x05; // 32 KiB
    rom[0x7fd8] = 0x01; // 2 KiB SRAM
    for v in [0x7fea, 0x7fee] {
        rom[v..v + 2].copy_from_slice(&handler.to_le_bytes());
    }
    rom[0x7ffc..0x7ffe].copy_from_slice(&[0x00, 0x80]);
    rom[0x7fdc..0x7fe0].copy_from_slice(&[0xff, 0xff, 0, 0]);
    let sum = rom.iter().fold(0u16, |s, &b| s.wrapping_add(b as u16));
    rom[0x7fdc..0x7fde].copy_from_slice(&(!sum).to_le_bytes());
    rom[0x7fde..0x7fe0].copy_from_slice(&sum.to_le_bytes());
    rom
}

/// Master clocks from H/V: dots 323 and 327 are 6 clocks, the others 4.
fn clocks(h: u16, v: u16) -> i64 {
    let h = h as i64;
    v as i64 * 1364 + h * 4 + 2 * (h > 323) as i64 + 2 * (h > 327) as i64
}

/// Per experiment: start H, V, end H, V, interrupt-handler H, V.
fn run() -> Vec<[u16; 6]> {
    let rom = build();
    if let Ok(p) = std::env::var("PPU_CART_TIMING_ROM") {
        std::fs::write(p, &rom).unwrap();
    }
    let mut s = Snes::new(&rom).unwrap();
    let ram = |s: &Snes| s.wram()[0x1000..0x1800].to_vec();
    for _ in 0..200 {
        s.run_frame();
        if &ram(&s)[0x7f8..] == DONE {
            break;
        }
    }
    let m = ram(&s);
    assert_eq!(&m[0x7f8..], DONE, "the timing ROM did not finish");
    let w = |a: usize| u16::from_le_bytes([m[a], m[a + 1]]) & 0x1ff;
    (0..experiments().len()).map(|i| [0, 2, 4, 6, 0x400, 0x402].map(|k| w(i * 8 + k))).collect()
}

/// bsnes v115.1 (libretro, accurate PPU) on this ROM, per experiment: start H, V, end H, V,
/// handler H, V (the handler pair is only written by the NMI/IRQ experiments). Measured with
/// `timing_oracle.py` on 2026-10-31; Snes9x agrees on every delta to the latch's resolution except
/// HDMA (Snes9x charges a flat 18 clocks a line, bsnes 16 plus a CPU-sync of 0-4) and the H/V IRQ
/// handler (the two differ by 48 clocks). Re-measure after changing the ROM.
const BSNES: [[u16; 6]; 96] = [
    [29, 10, 114, 10, 507, 511], [259, 10, 237, 11, 511, 511], [28, 10, 102, 11, 511, 511],
    [29, 10, 305, 11, 511, 511], [28, 10, 272, 11, 511, 511], [28, 10, 27, 12, 511, 511],
    [29, 10, 28, 12, 511, 511], [29, 10, 273, 11, 511, 511], [28, 10, 272, 11, 511, 511],
    [28, 10, 304, 11, 511, 511], [29, 10, 305, 11, 511, 511], [28, 10, 91, 12, 511, 511],
    [28, 10, 91, 12, 511, 511], [28, 10, 59, 12, 511, 511], [29, 10, 60, 12, 511, 511],
    [29, 10, 92, 12, 511, 511], [23, 10, 293, 10, 511, 511], [28, 10, 191, 37, 511, 511],
    [29, 10, 316, 10, 511, 511], [28, 10, 319, 10, 511, 511], [28, 10, 322, 10, 511, 511],
    [28, 10, 325, 10, 511, 511], [29, 10, 329, 10, 511, 511], [28, 10, 332, 10, 511, 511],
    [29, 10, 127, 10, 511, 511], [273, 10, 17, 11, 511, 511], [28, 10, 133, 10, 511, 511],
    [278, 10, 26, 11, 511, 511], [29, 10, 147, 10, 511, 511], [282, 10, 33, 11, 511, 511],
    [28, 10, 150, 10, 511, 511], [285, 10, 40, 11, 511, 511], [28, 10, 153, 10, 511, 511],
    [29, 10, 173, 10, 511, 511], [29, 10, 210, 11, 511, 511], [29, 10, 149, 10, 511, 511],
    [285, 10, 40, 11, 511, 511], [29, 10, 102, 146, 511, 511], [29, 10, 249, 149, 511, 511],
    [28, 10, 249, 149, 511, 511], [28, 10, 84, 149, 511, 511], [29, 10, 64, 149, 511, 511],
    [28, 10, 101, 146, 511, 511], [29, 10, 190, 150, 511, 511], [28, 10, 235, 152, 511, 511],
    [28, 10, 238, 152, 511, 511], [28, 10, 250, 149, 511, 511], [28, 10, 260, 151, 511, 511],
    [28, 10, 259, 151, 511, 511], [29, 10, 259, 151, 511, 511], [28, 10, 92, 155, 511, 511],
    [29, 10, 314, 158, 511, 511], [24, 10, 202, 123, 511, 511], [24, 10, 164, 126, 511, 511],
    [24, 10, 165, 126, 511, 511], [24, 10, 40, 126, 511, 511], [24, 10, 259, 125, 511, 511],
    [24, 10, 202, 123, 511, 511], [24, 10, 38, 127, 511, 511], [23, 10, 226, 128, 511, 511],
    [23, 10, 229, 128, 511, 511], [24, 10, 164, 126, 511, 511], [24, 10, 323, 127, 511, 511],
    [24, 10, 323, 127, 511, 511], [23, 10, 323, 127, 511, 511], [23, 10, 333, 130, 511, 511],
    [24, 10, 23, 134, 511, 511], [29, 10, 133, 10, 511, 511], [28, 10, 146, 10, 511, 511],
    [29, 10, 150, 10, 511, 511], [28, 10, 153, 10, 511, 511], [28, 10, 146, 10, 511, 511],
    [28, 10, 148, 10, 511, 511], [28, 10, 153, 10, 511, 511], [28, 10, 155, 10, 511, 511],
    [28, 10, 147, 10, 511, 511], [29, 10, 151, 10, 511, 511], [29, 10, 154, 10, 511, 511],
    [29, 10, 158, 10, 511, 511], [28, 10, 149, 10, 511, 511], [28, 10, 152, 10, 511, 511],
    [28, 10, 156, 10, 511, 511], [29, 10, 160, 10, 511, 511], [45, 11, 160, 11, 511, 511],
    [29, 11, 147, 11, 511, 511], [29, 11, 152, 11, 511, 511], [29, 11, 154, 11, 511, 511],
    [28, 260, 272, 3, 511, 511], [28, 260, 312, 3, 511, 511], [29, 260, 332, 3, 511, 511],
    [29, 238, 154, 242, 511, 511], [28, 238, 154, 242, 511, 511], [28, 224, 164, 225, 32, 225],
    [29, 224, 178, 225, 26, 225], [28, 224, 171, 225, 28, 225], [29, 10, 285, 20, 143, 20],
];

#[test]
fn timing_matches_hardware() {
    let got = run();
    let exps = experiments();
    assert_eq!(got.len(), BSNES.len(), "the ROM changed: re-measure BSNES");
    let mut bad = vec![];
    for (i, ((e, r), b)) in exps.iter().zip(&got).zip(&BSNES).enumerate() {
        let delta = |r: &[u16; 6]| (clocks(r[2], r[3]) - clocks(r[0], r[1])).rem_euclid(262 * 1364);
        let (d, db) = (delta(r), delta(b));
        println!("{i} {} {} {} {} {} {}  # {:<34} {d:6} clocks, bsnes {db:6}", r[0], r[1], r[2], r[3], r[4], r[5], e.name);
        // Each latch reads a 4-clock dot, so two of them are good to 8 clocks. HDMA: bsnes adds a
        // 0-4 clock CPU sync per line that ppu-cart (like Snes9x) folds into a flat overhead.
        let tol = match e.name.as_str() {
            n if n.starts_with("hdma") => 5 * (b[3] - b[1]) as i64 / 2,
            "nmi from wai" => 32,
            "h/v irq in lda spin" => 48,
            _ => 8,
        };
        // Where the sync lands (WAI wake after a V-IRQ): same dot, or within a few when the IRQ
        // was raised mid-line by the write that enabled it.
        let start_tol = if b[0] > 100 { 24 } else { 8 };
        let start = (clocks(r[0], r[1]) - clocks(b[0], b[1])).abs();
        if (d - db).abs() > tol || start > start_tol {
            bad.push(format!("{}: {d} clocks (bsnes {db}, within {tol}), start {},{} (bsnes {},{})", e.name, r[0], r[1], b[0], b[1]));
        }
    }
    assert!(bad.is_empty(), "timing differs from bsnes:\n{}", bad.join("\n"));
}
