//! The console around the CPU: cartridge mapping, WRAM, MMIO, DMA/HDMA, timing, the PPU bridge
//! into ppu-core, and the frame loop.
//!
//! Timing is counted in master clocks (21.477 MHz NTSC). Every CPU access advances the clock by its
//! region's speed (6, 8 or 12; FastROM via $420D), a scanline is 1364 clocks (1360 for line 240
//! of every other non-interlaced frame), a frame 262 lines. Per line, in order:
//! - H=0: line start (V=0: end of VBlank, HDMA init; V=225: VBlank, NMI, joypad auto-read);
//! - the H/V IRQ at HTIME*4+14 (V-only IRQs at H=10);
//! - H=88: the line renders, whole, with the registers as they are then (lines 1-224);
//! - H=538: DRAM refresh, 40 clocks the CPU loses;
//! - H=1104: HDMA (lines 0-224), its clocks also taken from the CPU.
//!
//! Simplifications (each documented where it lives): the multiply/divide results are ready at
//! once instead of after 8/16 CPU cycles; general DMA aligns to 8 clocks before it starts and not
//! after; ROM sizes that are not a power of two mirror by modulo.
mod ppu;

use crate::{Apu, Bus, Cpu};
use ppu::Ppu;
use std::collections::HashMap;

pub const LINE_CLOCKS: u32 = 1364;
pub const LINES: u16 = 262;
const RENDER_H: u32 = 88;
const REFRESH_H: u32 = 538;
const HDMA_H: u32 = 1104;
const VDISP: u16 = 225;
/// Auto-read takes about 4224 master clocks from VBlank start ($4212 bit 0 busy).
const AUTOJOY_CLOCKS: u64 = 4224;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Ev {
    Irq,
    Render,
    Refresh,
    Hdma,
    LineEnd,
}

#[derive(Clone, Copy, Default)]
struct Channel {
    dmap: u8,
    bbad: u8,
    a1t: u16,
    a1b: u8,
    das: u16,
    dasb: u8,
    a2a: u16,
    ntrl: u8,
    unused: u8,
    /// HDMA: this frame's table has ended.
    done: bool,
    /// HDMA: transfer on the next line (set by a fresh line count or the repeat bit).
    do_transfer: bool,
}

/// B-bus offsets of one transfer unit, by DMAP transfer mode (bits 0-2).
const MODE_OFFSETS: [&[u8]; 8] = [&[0], &[0, 1], &[0, 0], &[0, 0, 1, 1], &[0, 1, 2, 3], &[0, 1, 0, 1], &[0, 0], &[0, 0, 1, 1]];

/// A multiplicative hash for the per-PC profile (the default SipHash costs more than the
/// instruction it counts).
#[derive(Default)]
pub struct PcHasher(u64);
impl std::hash::Hasher for PcHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, b: &[u8]) {
        for &x in b {
            self.0 = (self.0 ^ x as u64).wrapping_mul(0x100000001b3);
        }
    }
    fn write_u32(&mut self, v: u32) {
        self.0 = (v as u64).wrapping_mul(0x9e3779b97f4a7c15);
    }
}
type PcMap = HashMap<u32, u64, std::hash::BuildHasherDefault<PcHasher>>;

/// Everything the 65816 sees through `Bus`.
pub struct System {
    pub apu: Apu,
    pub ppu: Ppu,
    rom: Vec<u8>,
    pub sram: Vec<u8>,
    hirom: bool,
    pub wram: Vec<u8>,
    /// The data bus's last value (open bus).
    mdr: u8,
    /// Master clocks since power-on, and the position in the current line.
    pub clock: u64,
    h: u32,
    pub v: u16,
    line_len: u32,
    evs: [(u32, Ev); 5],
    ev_n: usize,
    ev_i: usize,
    next_ev: u32,
    field: bool,
    /// Set at VBlank start; `run_frame` returns on it.
    frame_done: bool,
    pub framebuffer: Vec<u8>,
    // $42xx
    nmitimen: u8,
    wrio: u8,
    wrmpya: u8,
    wrdiv: u16,
    rddiv: u16,
    rdmpy: u16,
    htime: u16,
    vtime: u16,
    hdmaen: u8,
    fastrom: bool,
    nmi_flag: bool,
    irq_flag: bool,
    cpu_nmi: bool,
    cpu_irq: Option<bool>,
    joy: [u16; 4],
    joy_busy_until: u64,
    pub pads: [u16; 2],
    serial: [u16; 2],
    strobe: bool,
    wmadd: u32,
    dma: [Channel; 8],
}

pub struct Snes {
    pub cpu: Cpu,
    /// The rest of the console (`bus.apu` is the audio side).
    pub bus: System,
    /// Count master clocks per PC for `take_pc_ticks` (off by default: it costs a hash per instruction).
    pub profile: bool,
    pc_ticks: PcMap,
}

/// Score a header candidate at `base` ($7FC0 LoROM, $FFC0 HiROM).
fn header_score(rom: &[u8], base: usize, hi: bool) -> i32 {
    if rom.len() < base + 0x40 {
        return -100;
    }
    let h = &rom[base..base + 0x40];
    let mut s = 0;
    let map = h[0x15];
    if (map & 0xe0) == 0x20 && (map & 1 == 1) == hi {
        s += 4;
    }
    let sum = u16::from_le_bytes([h[0x1e], h[0x1f]]);
    let cpl = u16::from_le_bytes([h[0x1c], h[0x1d]]);
    if sum ^ cpl == 0xffff {
        s += 4;
    }
    let reset = u16::from_le_bytes([h[0x3c], h[0x3d]]);
    if reset >= 0x8000 {
        s += 2;
    }
    if h[..21].iter().all(|&c| (0x20..0x7f).contains(&c)) {
        s += 1;
    }
    s
}

impl System {
    fn new(rom: Vec<u8>, hirom: bool, sram: usize) -> System {
        let ch = Channel { dmap: 0xff, bbad: 0xff, a1t: 0xffff, a1b: 0xff, das: 0xffff, dasb: 0xff, a2a: 0xffff, ntrl: 0xff, unused: 0xff, ..Default::default() };
        let mut s = System {
            apu: Apu::new(),
            ppu: Ppu::default(),
            rom,
            sram: vec![0; sram],
            hirom,
            wram: vec![0; 0x20000],
            mdr: 0,
            clock: 0,
            h: 0,
            v: 0,
            line_len: LINE_CLOCKS,
            evs: [(0, Ev::LineEnd); 5],
            ev_n: 0,
            ev_i: 0,
            next_ev: 0,
            field: false,
            frame_done: false,
            framebuffer: vec![0; 256 * 224 * 4],
            nmitimen: 0,
            wrio: 0xff,
            wrmpya: 0xff,
            wrdiv: 0xffff,
            rddiv: 0,
            rdmpy: 0,
            htime: 0x1ff,
            vtime: 0x1ff,
            hdmaen: 0,
            fastrom: false,
            nmi_flag: false,
            irq_flag: false,
            cpu_nmi: false,
            cpu_irq: None,
            joy: [0; 4],
            joy_busy_until: 0,
            pads: [0; 2],
            serial: [0; 2],
            strobe: false,
            wmadd: 0,
            dma: [ch; 8],
        };
        s.framebuffer.chunks_exact_mut(4).for_each(|p| p[3] = 255);
        s.schedule(-1);
        s
    }

    // ---- timing ----

    /// Build this line's remaining events after position `from`.
    fn schedule(&mut self, from: i64) {
        let mut evs = [(0, Ev::LineEnd); 5];
        let mut n = 0;
        let cands = [
            (self.irq_pos(), Ev::Irq),
            ((1..VDISP).contains(&self.v).then_some(RENDER_H), Ev::Render),
            (Some(REFRESH_H), Ev::Refresh),
            ((self.v < VDISP).then_some(HDMA_H), Ev::Hdma),
            (Some(self.line_len), Ev::LineEnd),
        ];
        for (pos, e) in cands {
            if let Some(p) = pos.filter(|&p| p as i64 > from) {
                evs[n] = (p, e);
                n += 1;
            }
        }
        evs[..n].sort_by_key(|e| e.0);
        self.evs = evs;
        self.ev_n = n;
        self.ev_i = 0;
        self.next_ev = self.evs[0].0;
    }

    /// Where this line's H/V IRQ fires, if it does.
    fn irq_pos(&self) -> Option<u32> {
        let (hen, ven) = (self.nmitimen & 0x10 != 0, self.nmitimen & 0x20 != 0);
        if ven && self.v != self.vtime {
            return None;
        }
        match (hen, ven) {
            (true, _) if self.htime <= 339 => Some(self.htime as u32 * 4 + 14),
            (false, true) => Some(10),
            _ => None,
        }
    }

    #[inline]
    fn tick(&mut self, n: u32) {
        self.clock += n as u64;
        self.h += n;
        while self.h >= self.next_ev {
            self.event();
        }
    }

    fn event(&mut self) {
        let (pos, e) = self.evs[self.ev_i];
        self.ev_i += 1;
        self.next_ev = if self.ev_i < self.ev_n { self.evs[self.ev_i].0 } else { u32::MAX };
        match e {
            Ev::Irq => {
                self.irq_flag = true;
                self.cpu_irq = Some(true);
            }
            Ev::Render => {
                let y = self.v as usize - 1;
                let out = &mut self.framebuffer[y * 1024..(y + 1) * 1024];
                self.ppu.render_line(y, out);
            }
            Ev::Refresh => self.steal(40),
            Ev::Hdma => {
                if self.hdmaen != 0 {
                    let c = self.hdma_line();
                    self.steal(c);
                }
            }
            Ev::LineEnd => {
                self.h -= pos;
                self.line_start();
            }
        }
    }

    /// Clocks the CPU loses (refresh, HDMA): the clock moves, later events still fire in order.
    fn steal(&mut self, n: u32) {
        self.clock += n as u64;
        self.h += n;
    }

    fn line_start(&mut self) {
        self.v += 1;
        if self.v == LINES {
            self.v = 0;
            self.field = !self.field;
            self.nmi_flag = false;
            self.ppu.frame_start();
            for c in &mut self.dma {
                c.done = false;
            }
            if self.hdmaen != 0 {
                let c = self.hdma_init();
                self.steal(c);
            }
        }
        // shortcut: interlace (SETINI bit 0) is not modelled, so line 240 is short every other frame
        self.line_len = if self.v == 240 && self.field { 1360 } else { LINE_CLOCKS };
        if self.v == VDISP {
            self.ppu.vblank();
            self.nmi_flag = true;
            if self.nmitimen & 0x80 != 0 {
                self.cpu_nmi = true;
            }
            if self.nmitimen & 1 != 0 {
                self.joy = [self.pads[0], self.pads[1], 0, 0];
                self.serial = [0xffff; 2];
                self.joy_busy_until = self.clock + AUTOJOY_CLOCKS;
            }
            self.frame_done = true;
            self.apu.run_until(self.clock);
        }
        self.schedule(-1);
    }

    fn hdot(&self) -> u16 {
        (self.h / 4).min(339) as u16
    }

    // ---- memory map ----

    fn rom_at(&self, a: usize) -> u8 {
        // shortcut: non-power-of-two ROMs mirror by modulo; upgrade if such a ROM needs it
        self.rom[a % self.rom.len()]
    }

    /// Master clocks for an access at `addr`.
    #[inline]
    fn speed(&self, addr: u32) -> u32 {
        let bank = addr >> 16;
        let off = addr & 0xffff;
        let rom = if bank & 0x80 != 0 && self.fastrom { 6 } else { 8 };
        if bank & 0x40 != 0 {
            return rom;
        }
        match off {
            0x0000..=0x1fff => 8,
            0x2000..=0x3fff => 6,
            0x4000..=0x41ff => 12,
            0x4200..=0x5fff => 6,
            0x6000..=0x7fff => 8,
            _ => rom,
        }
    }

    /// The cartridge's answer at `addr` (None = nothing drives the bus).
    fn cart(&self, bank: u32, off: u32) -> Option<u8> {
        if let Some(a) = self.sram_addr(bank, off) {
            return Some(self.sram[a]);
        }
        if self.hirom {
            if bank & 0x40 != 0 || off >= 0x8000 {
                return Some(self.rom_at((((bank & 0x3f) << 16) | off) as usize));
            }
        } else if off >= 0x8000 || bank & 0x40 != 0 {
            return Some(self.rom_at((((bank & 0x7f) << 15) | (off & 0x7fff)) as usize));
        }
        None
    }

    /// SRAM: HiROM $20-$3F/$A0-$BF:6000-7FFF, LoROM $70-$7D/$F0-$FF:0000-7FFF.
    fn sram_addr(&self, bank: u32, off: u32) -> Option<usize> {
        if self.sram.is_empty() {
            return None;
        }
        let b = bank & 0x7f;
        let a = if self.hirom {
            if !((0x6000..0x8000).contains(&off) && (0x20..0x40).contains(&b)) {
                return None;
            }
            (b & 0x1f) << 13 | (off - 0x6000)
        } else {
            if !(off < 0x8000 && b >= 0x70 && (bank >= 0xf0 || b < 0x7e)) {
                return None;
            }
            (b & 0x0f) << 15 | off
        };
        Some(a as usize % self.sram.len())
    }

    /// Read without timing (the CPU's `read` ticks first; DMA ticks per byte itself).
    fn peek(&mut self, addr: u32) -> u8 {
        let bank = addr >> 16;
        let off = addr & 0xffff;
        if bank & 0xfe == 0x7e {
            return self.wram[(addr & 0x1ffff) as usize];
        }
        if bank & 0x40 == 0 && off < 0x6000 {
            return match off {
                0x0000..=0x1fff => self.wram[off as usize],
                0x2100..=0x21ff => self.read_b(off as u8),
                0x4016 => (self.mdr & 0xfc) | self.serial_bit(0),
                0x4017 => (self.mdr & 0xe0) | 0x1c | self.serial_bit(1),
                0x4210..=0x421f => self.read_cpu_reg(off),
                0x4300..=0x437f => self.read_dma_reg(off),
                _ => self.mdr,
            };
        }
        self.cart(bank, off).unwrap_or(self.mdr)
    }

    fn poke(&mut self, addr: u32, v: u8) {
        let bank = addr >> 16;
        let off = addr & 0xffff;
        if bank & 0xfe == 0x7e {
            self.wram[(addr & 0x1ffff) as usize] = v;
            return;
        }
        if bank & 0x40 == 0 && off < 0x6000 {
            match off {
                0x0000..=0x1fff => self.wram[off as usize] = v,
                0x2100..=0x21ff => self.write_b(off as u8, v),
                0x4016 => {
                    let s = v & 1 != 0;
                    if s {
                        self.serial = self.pads;
                    }
                    self.strobe = s;
                }
                0x4200..=0x420f => self.write_cpu_reg(off, v),
                0x4300..=0x437f => self.write_dma_reg(off, v),
                _ => {}
            }
            return;
        }
        if let Some(a) = self.sram_addr(bank, off) {
            self.sram[a] = v;
        }
    }

    fn serial_bit(&mut self, port: usize) -> u8 {
        if self.strobe {
            return (self.pads[port] >> 15) as u8;
        }
        let b = (self.serial[port] >> 15) as u8;
        self.serial[port] = self.serial[port] << 1 | 1;
        b
    }

    // ---- B bus ----

    fn read_b(&mut self, reg: u8) -> u8 {
        match reg {
            0x00..=0x3f => {
                let latch = self.wrio & 0x80 != 0;
                let (hd, v) = (self.hdot(), self.v);
                self.ppu.read(reg, hd, v, latch)
            }
            0x40..=0x7f => {
                self.apu.run_until(self.clock);
                self.apu.cpu_read_port(reg & 3)
            }
            0x80 => {
                let v = self.wram[self.wmadd as usize];
                self.wmadd = (self.wmadd + 1) & 0x1ffff;
                v
            }
            _ => self.mdr,
        }
    }

    fn write_b(&mut self, reg: u8, v: u8) {
        match reg {
            0x00..=0x3f => {
                let active = (1..VDISP).contains(&self.v);
                self.ppu.write(reg, v, active)
            }
            0x40..=0x7f => {
                self.apu.run_until(self.clock);
                self.apu.cpu_write_port(reg & 3, v)
            }
            0x80 => {
                self.wram[self.wmadd as usize] = v;
                self.wmadd = (self.wmadd + 1) & 0x1ffff;
            }
            0x81 => self.wmadd = (self.wmadd & 0x1ff00) | v as u32,
            0x82 => self.wmadd = (self.wmadd & 0x100ff) | (v as u32) << 8,
            0x83 => self.wmadd = (self.wmadd & 0x0ffff) | ((v as u32 & 1) << 16),
            _ => {}
        }
    }

    // ---- $42xx ----

    fn write_cpu_reg(&mut self, off: u32, v: u8) {
        match off {
            0x4200 => {
                let was = self.nmitimen;
                self.nmitimen = v;
                // Enabling NMI inside VBlank with the flag still set raises it at once.
                if was & 0x80 == 0 && v & 0x80 != 0 && self.nmi_flag {
                    self.cpu_nmi = true;
                }
                if v & 0x30 == 0 {
                    self.irq_flag = false;
                    self.cpu_irq = Some(false);
                }
                self.reschedule_irq();
            }
            0x4201 => {
                if self.wrio & 0x80 != 0 && v & 0x80 == 0 {
                    let (hd, line) = (self.hdot(), self.v);
                    self.ppu.latch_counters(hd, line);
                }
                self.wrio = v;
            }
            0x4202 => self.wrmpya = v,
            0x4203 => {
                // shortcut: the product is ready at once (hardware: 8 CPU cycles later); a read
                // that early is a game bug the compiler's runtime does not have
                self.rdmpy = self.wrmpya as u16 * v as u16;
                self.rddiv = v as u16;
            }
            0x4204 => self.wrdiv = (self.wrdiv & 0xff00) | v as u16,
            0x4205 => self.wrdiv = (self.wrdiv & 0x00ff) | (v as u16) << 8,
            0x4206 => {
                // shortcut: the quotient is ready at once (hardware: 16 CPU cycles later)
                if v == 0 {
                    self.rddiv = 0xffff;
                    self.rdmpy = self.wrdiv;
                } else {
                    self.rddiv = self.wrdiv / v as u16;
                    self.rdmpy = self.wrdiv % v as u16;
                }
            }
            0x4207 => self.htime = (self.htime & 0x100) | v as u16,
            0x4208 => self.htime = (self.htime & 0xff) | ((v as u16 & 1) << 8),
            0x4209 => self.vtime = (self.vtime & 0x100) | v as u16,
            0x420a => self.vtime = (self.vtime & 0xff) | ((v as u16 & 1) << 8),
            0x420b => self.run_dma(v),
            0x420c => self.hdmaen = v,
            0x420d => self.fastrom = v & 1 != 0,
            _ => {}
        }
        if (0x4207..=0x420a).contains(&off) {
            self.reschedule_irq();
        }
    }

    /// The IRQ position may have moved: rebuild what is left of this line.
    fn reschedule_irq(&mut self) {
        self.schedule(self.h as i64);
    }

    fn read_cpu_reg(&mut self, off: u32) -> u8 {
        match off {
            0x4210 => {
                let v = (self.nmi_flag as u8) << 7 | (self.mdr & 0x70) | 2;
                self.nmi_flag = false;
                v
            }
            0x4211 => {
                let v = (self.irq_flag as u8) << 7 | (self.mdr & 0x7f);
                self.irq_flag = false;
                self.cpu_irq = Some(false);
                v
            }
            0x4212 => {
                let vblank = self.v >= VDISP;
                let hblank = self.h >= 1096 || self.h < 4;
                (vblank as u8) << 7 | (hblank as u8) << 6 | (self.mdr & 0x3e) | (self.clock < self.joy_busy_until) as u8
            }
            0x4213 => self.wrio,
            0x4214 => self.rddiv as u8,
            0x4215 => (self.rddiv >> 8) as u8,
            0x4216 => self.rdmpy as u8,
            0x4217 => (self.rdmpy >> 8) as u8,
            0x4218..=0x421f => {
                let j = self.joy[((off - 0x4218) / 2) as usize];
                if off & 1 == 0 {
                    j as u8
                } else {
                    (j >> 8) as u8
                }
            }
            _ => self.mdr,
        }
    }

    // ---- DMA ----

    fn read_dma_reg(&mut self, off: u32) -> u8 {
        let c = &self.dma[((off >> 4) & 7) as usize];
        match off & 0xf {
            0 => c.dmap,
            1 => c.bbad,
            2 => c.a1t as u8,
            3 => (c.a1t >> 8) as u8,
            4 => c.a1b,
            5 => c.das as u8,
            6 => (c.das >> 8) as u8,
            7 => c.dasb,
            8 => c.a2a as u8,
            9 => (c.a2a >> 8) as u8,
            0xa => c.ntrl,
            0xb | 0xf => c.unused,
            _ => self.mdr,
        }
    }

    fn write_dma_reg(&mut self, off: u32, v: u8) {
        let c = &mut self.dma[((off >> 4) & 7) as usize];
        match off & 0xf {
            0 => c.dmap = v,
            1 => c.bbad = v,
            2 => c.a1t = (c.a1t & 0xff00) | v as u16,
            3 => c.a1t = (c.a1t & 0x00ff) | (v as u16) << 8,
            4 => c.a1b = v,
            5 => c.das = (c.das & 0xff00) | v as u16,
            6 => c.das = (c.das & 0x00ff) | (v as u16) << 8,
            7 => c.dasb = v,
            8 => c.a2a = (c.a2a & 0xff00) | v as u16,
            9 => c.a2a = (c.a2a & 0x00ff) | (v as u16) << 8,
            0xa => c.ntrl = v,
            0xb | 0xf => c.unused = v,
            _ => {}
        }
    }

    /// A-bus addresses DMA cannot reach: the B bus and the DMA registers themselves.
    fn dma_blocked(addr: u32) -> bool {
        let off = addr & 0xffff;
        addr & 0x40_0000 == 0 && ((0x2100..0x2200).contains(&off) || (0x4300..0x4380).contains(&off) || off == 0x420b || off == 0x420c)
    }

    /// Move one byte between `a` (A bus) and B-bus register `b` in the channel's direction.
    fn dma_byte(&mut self, dmap: u8, a: u32, b: u8) {
        if dmap & 0x80 == 0 {
            let v = if Self::dma_blocked(a) { self.mdr } else { self.peek(a) };
            self.write_b(b, v);
        } else {
            let v = self.read_b(b);
            if !Self::dma_blocked(a) {
                self.poke(a, v);
            }
        }
    }

    /// General DMA ($420B): channels in order, 8 clocks per byte, 8 per channel, after an 8-clock
    /// alignment. Events (HDMA, refresh, line ends) keep firing while it runs.
    fn run_dma(&mut self, mask: u8) {
        if mask == 0 {
            return;
        }
        let align = (8 - (self.clock % 8) as u32) % 8;
        self.tick(align + 8);
        for i in 0..8 {
            if mask & (1 << i) == 0 {
                continue;
            }
            self.tick(8);
            let mut k = 0usize;
            loop {
                let c = self.dma[i];
                let offs = MODE_OFFSETS[(c.dmap & 7) as usize];
                let b = c.bbad.wrapping_add(offs[k % offs.len()]);
                self.dma_byte(c.dmap, (c.a1b as u32) << 16 | c.a1t as u32, b);
                let c = &mut self.dma[i];
                match (c.dmap >> 3) & 3 {
                    0 => c.a1t = c.a1t.wrapping_add(1),
                    2 => c.a1t = c.a1t.wrapping_sub(1),
                    _ => {}
                }
                c.das = c.das.wrapping_sub(1);
                k += 1;
                let end = c.das == 0;
                self.tick(8);
                if end {
                    break;
                }
            }
        }
    }

    /// Frame start: reload every enabled channel's table. Returns the clocks it took.
    fn hdma_init(&mut self) -> u32 {
        let mut cost = 18;
        for i in 0..8 {
            if self.hdmaen & (1 << i) == 0 {
                continue;
            }
            self.dma[i].a2a = self.dma[i].a1t;
            cost += 8 + self.hdma_reload(i);
        }
        cost
    }

    /// Load the next line count (and indirect address); a zero count ends the table.
    fn hdma_reload(&mut self, i: usize) -> u32 {
        let c = self.dma[i];
        let bank = (c.a1b as u32) << 16;
        let n = self.peek(bank | c.a2a as u32);
        let mut a2a = c.a2a.wrapping_add(1);
        let mut cost = 8;
        let mut das = c.das;
        if c.dmap & 0x40 != 0 {
            let lo = self.peek(bank | a2a as u32);
            let hi = self.peek(bank | a2a.wrapping_add(1) as u32);
            das = (hi as u16) << 8 | lo as u16;
            a2a = a2a.wrapping_add(2);
            cost += 16;
        }
        let c = &mut self.dma[i];
        c.ntrl = n;
        c.a2a = a2a;
        c.das = das;
        c.do_transfer = true;
        c.done = n == 0;
        cost
    }

    /// One line of HDMA. Returns the clocks it took.
    fn hdma_line(&mut self) -> u32 {
        let mut cost = 0;
        for i in 0..8 {
            if self.hdmaen & (1 << i) == 0 || self.dma[i].done {
                continue;
            }
            cost += 8;
            let c = self.dma[i];
            if c.do_transfer {
                let offs = MODE_OFFSETS[(c.dmap & 7) as usize];
                let indirect = c.dmap & 0x40 != 0;
                let mut a = if indirect { c.das } else { c.a2a };
                let bank = if indirect { c.dasb } else { c.a1b } as u32;
                for &o in offs {
                    self.dma_byte(c.dmap, bank << 16 | a as u32, c.bbad.wrapping_add(o));
                    a = a.wrapping_add(1);
                    cost += 8;
                }
                let c = &mut self.dma[i];
                if indirect {
                    c.das = a;
                } else {
                    c.a2a = a;
                }
            }
            let c = &mut self.dma[i];
            c.ntrl = c.ntrl.wrapping_sub(1);
            c.do_transfer = c.ntrl & 0x80 != 0;
            if c.ntrl & 0x7f == 0 {
                cost += self.hdma_reload(i);
            }
        }
        if cost > 0 {
            cost + 18
        } else {
            0
        }
    }
}

impl Bus for System {
    #[inline]
    fn read(&mut self, addr: u32) -> u8 {
        let s = self.speed(addr);
        self.tick(s);
        let v = self.peek(addr);
        self.mdr = v;
        v
    }
    #[inline]
    fn write(&mut self, addr: u32, value: u8) {
        let s = self.speed(addr);
        self.tick(s);
        self.mdr = value;
        self.poke(addr, value);
    }
    #[inline]
    fn idle(&mut self) {
        self.tick(6);
    }
}

impl Snes {
    /// Load a `.sfc` (LoROM or HiROM, detected from the internal header; a 512-byte copier header
    /// is skipped).
    pub fn new(rom: &[u8]) -> Result<Snes, String> {
        let rom = if rom.len() % 1024 == 512 { &rom[512..] } else { rom };
        if rom.len() < 0x8000 {
            return Err(format!("ppu-cart: a {}-byte ROM is too small for a cartridge", rom.len()));
        }
        let hirom = header_score(rom, 0xffc0, true) > header_score(rom, 0x7fc0, false);
        let base = if hirom { 0xffc0 } else { 0x7fc0 };
        let sram_code = rom[base + 0x18];
        let sram = if sram_code > 0 && sram_code <= 8 { 1024 << sram_code } else { 0 };
        let mut s = Snes {
            cpu: Cpu::new(),
            bus: System::new(rom.to_vec(), hirom, sram),
            profile: false,
            pc_ticks: PcMap::default(),
        };
        s.cpu.reset(&mut s.bus);
        Ok(s)
    }

    /// True when the header says HiROM.
    pub fn hirom(&self) -> bool {
        self.bus.hirom
    }

    /// Run until the PPU finishes the next frame (to the start of the following VBlank).
    pub fn run_frame(&mut self) {
        self.bus.frame_done = false;
        while !self.bus.frame_done {
            if self.bus.cpu_nmi {
                self.bus.cpu_nmi = false;
                self.cpu.nmi();
            }
            if let Some(l) = self.bus.cpu_irq.take() {
                self.cpu.set_irq(l);
            }
            if self.profile && !self.cpu.waiting() {
                let (pc, c0) = (self.cpu.pc, self.bus.clock);
                self.cpu.step(&mut self.bus);
                *self.pc_ticks.entry(pc).or_default() += self.bus.clock - c0;
            } else {
                self.cpu.step(&mut self.bus);
            }
        }
        // The NMI raised at VBlank start reaches the CPU before the next frame's first step.
        self.bus.apu.run_until(self.bus.clock);
    }

    /// Held buttons for controller 0 or 1 (`crate::pad` bits), latched by the next auto-read.
    pub fn set_pad(&mut self, port: usize, buttons: u16) {
        if port < 2 {
            self.bus.pads[port] = buttons & 0xfff0;
        }
    }

    /// The last frame, 256x224 RGBA8.
    pub fn framebuffer(&self) -> &[u8] {
        &self.bus.framebuffer
    }

    /// Work RAM ($7E:0000-$7F:FFFF), for tools that read game state by symbol.
    pub fn wram(&self) -> &[u8] {
        &self.bus.wram
    }

    /// Master clocks spent at each 24-bit PC since the last call, for native profiling; cleared
    /// on read. Counts only while `profile` is set; time the CPU spends halted in WAI is not
    /// counted (it is idle, not busy).
    pub fn take_pc_ticks(&mut self) -> Vec<(u32, u64)> {
        self.pc_ticks.drain().collect()
    }
}

#[cfg(test)]
mod tests;
