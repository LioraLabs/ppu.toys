//! The 65816. Owned by the CPU work; the system drives it only through this surface.
//!
//! Written from the W65C816S data sheet (cycle table 5-7) and documented hardware behaviour.
//! Every bus cycle of an instruction is one `Bus::read`, `Bus::write` or `Bus::idle`, in the
//! data sheet's order: an internal operation (VDA=VPA=0) is `idle`, so the system can charge
//! 6, 8 or 12 master clocks per access.
//!
//! Surface:
//! - `new`, `reset`, `step`, `nmi`, `set_irq`, `waiting`, `pc` (PBR:PC as one 24-bit value).
//! - Additions: the registers are public fields (`a`, `x`, `y`, `s`, `d`, `dbr`, `p`, `e`) for
//!   debuggers, save states and tests; `stopped()` is true after STP.
//!
//! Interrupts: `nmi()` latches an edge and `set_irq()` sets the line level; both are only looked
//! at at the start of `step()`. A step that finds one pending services it (2 internal cycles,
//! the pushes and the vector read, like BRK without its signature fetch) and returns without
//! running an instruction. While waiting after WAI, or for good after STP, a step is one `idle`
//! so the master clock keeps moving; WAI wakes on NMI or on the IRQ line even with I set (then
//! it resumes without taking the IRQ). MVN/MVP move one byte per step, re-running the opcode
//! until A wraps to $FFFF, as hardware does (so interrupts can land between bytes).
use crate::Bus;

const C: u8 = 0x01;
const Z: u8 = 0x02;
const I: u8 = 0x04;
const D: u8 = 0x08;
const XF: u8 = 0x10;
const M: u8 = 0x20;
const V: u8 = 0x40;
const N: u8 = 0x80;

/// Marks an effective address in bank 0 whose second byte wraps at $FFFF (direct page, stack).
const WRAP16: u32 = 1 << 24;

#[derive(Clone, Debug, Default)]
pub struct Cpu {
    /// Program bank and counter as one 24-bit address (PBR:PC).
    pub pc: u32,
    pub a: u16,
    pub x: u16,
    pub y: u16,
    pub s: u16,
    pub d: u16,
    pub dbr: u8,
    /// Status flags NVMXDIZC (M and X read 1 in emulation mode).
    pub p: u8,
    /// Emulation mode.
    pub e: bool,
    nmi_pending: bool,
    irq_line: bool,
    wait: bool,
    stop: bool,
}

// Group 1 addressing modes (ORA AND EOR ADC STA LDA CMP SBC), by `opcode & 0x1f`.
const DPX_IND: u8 = 0x01;
const SR: u8 = 0x03;
const DP: u8 = 0x05;
const DPL_IND: u8 = 0x07;
const IMM: u8 = 0x09;
const ABS: u8 = 0x0d;
const LONG: u8 = 0x0f;
const DP_IND_Y: u8 = 0x11;
const DP_IND: u8 = 0x12;
const SR_IND_Y: u8 = 0x13;
const DPX: u8 = 0x15;
const DPL_IND_Y: u8 = 0x17;
const ABSY: u8 = 0x19;
const ABSX: u8 = 0x1d;
const LONGX: u8 = 0x1f;
// Not a group 1 encoding; LDX/STX dp,Y.
const DPY: u8 = 0x16;

impl Cpu {
    pub fn new() -> Self {
        Self::default()
    }

    /// Power-on/reset: emulation mode, vector at $00:FFFC.
    pub fn reset(&mut self, bus: &mut impl Bus) {
        self.e = true;
        self.p = M | XF | I;
        self.s = 0x0100 | (self.s & 0xff);
        self.x &= 0xff;
        self.y &= 0xff;
        self.d = 0;
        self.dbr = 0;
        self.wait = false;
        self.stop = false;
        self.nmi_pending = false;
        let lo = bus.read(0xfffc) as u32;
        let hi = bus.read(0xfffd) as u32;
        self.pc = hi << 8 | lo;
    }

    /// Execute one instruction, or service a pending NMI/IRQ first.
    pub fn step(&mut self, bus: &mut impl Bus) {
        if self.stop {
            bus.idle();
            return;
        }
        if self.wait {
            if self.nmi_pending || self.irq_line {
                self.wait = false;
            } else {
                bus.idle();
                return;
            }
        }
        if self.nmi_pending {
            self.nmi_pending = false;
            bus.idle();
            bus.idle();
            self.interrupt(bus, 0xffea, 0xfffa, false);
            return;
        }
        if self.irq_line && self.p & I == 0 {
            bus.idle();
            bus.idle();
            self.interrupt(bus, 0xffee, 0xfffe, false);
            return;
        }
        let op = self.fetch(bus);
        self.exec(bus, op);
    }

    /// Edge-triggered: the system raises it at the start of VBlank when enabled.
    pub fn nmi(&mut self) {
        self.nmi_pending = true;
    }
    /// Level-triggered IRQ line.
    pub fn set_irq(&mut self, level: bool) {
        self.irq_line = level;
    }
    /// True after WAI until an interrupt; STP stops for good.
    pub fn waiting(&self) -> bool {
        self.wait || self.stop
    }
    /// True after STP (only reset restarts the CPU).
    pub fn stopped(&self) -> bool {
        self.stop
    }

    // ---- small helpers ----

    #[inline(always)]
    fn m16(&self) -> bool {
        self.p & M == 0
    }
    #[inline(always)]
    fn x16(&self) -> bool {
        self.p & XF == 0
    }
    #[inline(always)]
    fn pbr(&self) -> u32 {
        self.pc & 0xff0000
    }
    #[inline(always)]
    fn set_pc16(&mut self, v: u16) {
        self.pc = self.pbr() | v as u32;
    }
    #[inline(always)]
    fn fetch(&mut self, bus: &mut impl Bus) -> u8 {
        let v = bus.read(self.pc);
        self.pc = self.pbr() | (self.pc.wrapping_add(1) & 0xffff);
        v
    }
    #[inline(always)]
    fn fetch16(&mut self, bus: &mut impl Bus) -> u16 {
        let lo = self.fetch(bus) as u16;
        lo | (self.fetch(bus) as u16) << 8
    }
    #[inline(always)]
    fn dl_penalty(&mut self, bus: &mut impl Bus) {
        if self.d & 0xff != 0 {
            bus.idle();
        }
    }
    #[inline(always)]
    fn nz(&mut self, v: u16, w16: bool) {
        self.p &= !(N | Z);
        if w16 {
            if v == 0 {
                self.p |= Z;
            }
            if v & 0x8000 != 0 {
                self.p |= N;
            }
        } else {
            if v & 0xff == 0 {
                self.p |= Z;
            }
            if v & 0x80 != 0 {
                self.p |= N;
            }
        }
    }
    #[inline(always)]
    fn flag(&mut self, f: u8, on: bool) {
        if on {
            self.p |= f;
        } else {
            self.p &= !f;
        }
    }
    #[inline(always)]
    fn set_a(&mut self, v: u16, w16: bool) {
        self.a = if w16 {
            v
        } else {
            (self.a & 0xff00) | (v & 0xff)
        };
    }
    /// P after PLP/REP/SEP/RTI: emulation keeps M and X set; 8-bit index clears XH and YH.
    #[inline(always)]
    fn set_p(&mut self, v: u8) {
        self.p = if self.e { v | M | XF } else { v };
        if self.p & XF != 0 {
            self.x &= 0xff;
            self.y &= 0xff;
        }
    }
    #[inline(always)]
    fn fix_s(&mut self) {
        if self.e {
            self.s = 0x0100 | (self.s & 0xff);
        }
    }

    // ---- stack: 6502-era instructions wrap within page 1 in emulation mode; the 65816's own
    // instructions (`*_n`) run S as 16 bits and truncate it after (`fix_s`).

    #[inline(always)]
    fn push(&mut self, bus: &mut impl Bus, v: u8) {
        bus.write(self.s as u32, v);
        self.s = self.s.wrapping_sub(1);
        self.fix_s();
    }
    #[inline(always)]
    fn pull(&mut self, bus: &mut impl Bus) -> u8 {
        self.s = self.s.wrapping_add(1);
        self.fix_s();
        bus.read(self.s as u32)
    }
    #[inline(always)]
    fn push_n(&mut self, bus: &mut impl Bus, v: u8) {
        bus.write(self.s as u32, v);
        self.s = self.s.wrapping_sub(1);
    }
    #[inline(always)]
    fn pull_n(&mut self, bus: &mut impl Bus) -> u8 {
        self.s = self.s.wrapping_add(1);
        bus.read(self.s as u32)
    }
    #[inline(always)]
    fn push16(&mut self, bus: &mut impl Bus, v: u16) {
        self.push(bus, (v >> 8) as u8);
        self.push(bus, v as u8);
    }
    #[inline(always)]
    fn pull16(&mut self, bus: &mut impl Bus) -> u16 {
        let lo = self.pull(bus) as u16;
        lo | (self.pull(bus) as u16) << 8
    }
    #[inline(always)]
    fn push16_n(&mut self, bus: &mut impl Bus, v: u16) {
        self.push_n(bus, (v >> 8) as u8);
        self.push_n(bus, v as u8);
    }
    #[inline(always)]
    fn pull16_n(&mut self, bus: &mut impl Bus) -> u16 {
        let lo = self.pull_n(bus) as u16;
        lo | (self.pull_n(bus) as u16) << 8
    }

    // ---- data access at an effective address (24-bit, or WRAP16 for bank-0 wrapping) ----

    #[inline(always)]
    fn next(ea: u32) -> u32 {
        if ea & WRAP16 != 0 {
            (ea & 0xff0000) | (ea.wrapping_add(1) & 0xffff)
        } else {
            ea.wrapping_add(1) & 0xffffff
        }
    }
    #[inline(always)]
    fn rd(&mut self, bus: &mut impl Bus, ea: u32, w16: bool) -> u16 {
        let lo = bus.read(ea & 0xffffff) as u16;
        if w16 {
            lo | (bus.read(Self::next(ea)) as u16) << 8
        } else {
            lo
        }
    }
    #[inline(always)]
    fn wr(&mut self, bus: &mut impl Bus, ea: u32, v: u16, w16: bool) {
        bus.write(ea & 0xffffff, v as u8);
        if w16 {
            bus.write(Self::next(ea), (v >> 8) as u8);
        }
    }

    /// Direct page address `o` (+ index), with the emulation-mode page wrap when DL = 0.
    #[inline(always)]
    fn dp_addr(&self, o: u16) -> u32 {
        if self.e && self.d & 0xff == 0 {
            WRAP16 | (self.d | (o & 0xff)) as u32
        } else {
            WRAP16 | self.d.wrapping_add(o) as u32
        }
    }
    /// A 16-bit pointer from the direct page at `addr` (high byte wraps in-page in emulation, DL=0).
    #[inline(always)]
    fn dp_ptr(&mut self, bus: &mut impl Bus, addr: u16) -> u16 {
        let lo = bus.read(addr as u32) as u16;
        let hi_addr = if self.e && self.d & 0xff == 0 {
            (addr & 0xff00) | (addr.wrapping_add(1) & 0xff)
        } else {
            addr.wrapping_add(1)
        };
        lo | (bus.read(hi_addr as u32) as u16) << 8
    }
    #[inline(always)]
    fn index_penalty(&mut self, bus: &mut impl Bus, base: u32, ea: u32, write: bool) {
        if write || self.x16() || (base ^ ea) & 0xffff00 != 0 {
            bus.idle();
        }
    }

    /// Operand fetch and address calculation for a memory mode, with its internal cycles.
    /// `write` adds the abs,X / abs,Y / (dp),Y index cycle unconditionally (stores and RMW).
    #[inline(always)]
    fn ea(&mut self, bus: &mut impl Bus, mode: u8, write: bool) -> u32 {
        match mode {
            DP => {
                let o = self.fetch(bus) as u16;
                self.dl_penalty(bus);
                WRAP16 | self.d.wrapping_add(o) as u32
            }
            DPX | DPY => {
                let o = self.fetch(bus) as u16;
                self.dl_penalty(bus);
                bus.idle();
                let i = if mode == DPX { self.x } else { self.y };
                self.dp_addr(o.wrapping_add(i))
            }
            ABS => {
                let a = self.fetch16(bus) as u32;
                (self.dbr as u32) << 16 | a
            }
            ABSX | ABSY => {
                let base = (self.dbr as u32) << 16 | self.fetch16(bus) as u32;
                let i = if mode == ABSX { self.x } else { self.y };
                let ea = (base + i as u32) & 0xffffff;
                self.index_penalty(bus, base, ea, write);
                ea
            }
            LONG | LONGX => {
                let lo = self.fetch16(bus) as u32;
                let a = (self.fetch(bus) as u32) << 16 | lo;
                if mode == LONGX {
                    (a + self.x as u32) & 0xffffff
                } else {
                    a
                }
            }
            DP_IND | DP_IND_Y => {
                let o = self.fetch(bus) as u16;
                self.dl_penalty(bus);
                let ptr = self.dp_ptr(bus, self.d.wrapping_add(o));
                let base = (self.dbr as u32) << 16 | ptr as u32;
                if mode == DP_IND {
                    return base;
                }
                let ea = (base + self.y as u32) & 0xffffff;
                self.index_penalty(bus, base, ea, write);
                ea
            }
            DPX_IND => {
                let o = self.fetch(bus) as u16;
                self.dl_penalty(bus);
                bus.idle();
                let addr = self.dp_addr(o.wrapping_add(self.x)) as u16;
                (self.dbr as u32) << 16 | self.dp_ptr(bus, addr) as u32
            }
            DPL_IND | DPL_IND_Y => {
                let o = self.fetch(bus) as u16;
                self.dl_penalty(bus);
                let p = self.d.wrapping_add(o);
                let lo = bus.read(p as u32) as u32;
                let mid = bus.read(p.wrapping_add(1) as u32) as u32;
                let hi = bus.read(p.wrapping_add(2) as u32) as u32;
                let a = hi << 16 | mid << 8 | lo;
                if mode == DPL_IND_Y {
                    (a + self.y as u32) & 0xffffff
                } else {
                    a
                }
            }
            SR => {
                let o = self.fetch(bus) as u16;
                bus.idle();
                WRAP16 | self.s.wrapping_add(o) as u32
            }
            SR_IND_Y => {
                let o = self.fetch(bus) as u16;
                bus.idle();
                let p = self.s.wrapping_add(o);
                let lo = bus.read(p as u32) as u32;
                let hi = bus.read(p.wrapping_add(1) as u32) as u32;
                bus.idle();
                (((self.dbr as u32) << 16 | hi << 8 | lo) + self.y as u32) & 0xffffff
            }
            _ => unreachable!(),
        }
    }

    // ---- ALU ----

    /// ADC, and SBC as ADC of the complement (`sub`), binary or BCD at the M width.
    fn add(&mut self, v: u16, sub: bool) {
        let w16 = self.m16();
        let (mask, sign) = if w16 {
            (0xffffu32, 0x8000u32)
        } else {
            (0xff, 0x80)
        };
        let a = self.a as u32 & mask;
        let v = if sub {
            !(v as u32) & mask
        } else {
            v as u32 & mask
        };
        let c = (self.p & C) as u32;
        let r;
        if self.p & D == 0 {
            r = a + v + c;
            self.flag(V, !(a ^ v) & (a ^ r) & sign != 0);
            self.flag(C, r > mask);
        } else {
            let bits = if w16 { 16 } else { 8 };
            let mut acc = 0u32;
            let mut carry = c;
            let mut shift = 0;
            while shift < bits {
                let mut d = ((a >> shift) & 0xf) + ((v >> shift) & 0xf) + carry;
                if shift == bits - 4 {
                    // V comes from the top digit before its decimal adjust.
                    let pre = acc | (d << shift);
                    self.flag(V, !(a ^ v) & (a ^ pre) & sign != 0);
                }
                if sub {
                    carry = (d > 0xf) as u32;
                    if carry == 0 {
                        d = d.wrapping_sub(6);
                    }
                } else {
                    if d > 9 {
                        d += 6;
                    }
                    carry = (d > 0xf) as u32;
                }
                acc |= (d & 0xf) << shift;
                shift += 4;
            }
            r = acc;
            self.flag(C, carry != 0);
        }
        let r = (r & mask) as u16;
        self.set_a(r, w16);
        self.nz(r, w16);
    }

    #[inline(always)]
    fn cmp(&mut self, reg: u16, v: u16, w16: bool) {
        let mask = if w16 { 0xffff } else { 0xff };
        let (r, borrow) = (reg & mask).overflowing_sub(v & mask);
        self.flag(C, !borrow);
        self.nz(r, w16);
    }

    /// Group 1 operation `k` (opcode >> 5) on a value.
    #[inline(always)]
    fn group1(&mut self, k: u8, v: u16) {
        let w16 = self.m16();
        match k {
            0 => {
                let r = self.a | v;
                self.set_a(r, w16);
                self.nz(r, w16);
            }
            1 => {
                let r = self.a & v;
                self.set_a(r, w16);
                self.nz(r, w16);
            }
            2 => {
                let r = self.a ^ v;
                self.set_a(r, w16);
                self.nz(r, w16);
            }
            3 => self.add(v, false),
            5 => {
                self.set_a(v, w16);
                self.nz(v, w16);
            }
            6 => self.cmp(self.a, v, w16),
            7 => self.add(v, true),
            _ => unreachable!(),
        }
    }

    #[inline(always)]
    fn bit(&mut self, v: u16, imm: bool) {
        let w16 = self.m16();
        let mask = if w16 { 0xffff } else { 0xff };
        self.flag(Z, self.a & v & mask == 0);
        if !imm {
            let sign = if w16 { 0x8000 } else { 0x80 };
            self.flag(N, v & sign != 0);
            self.flag(V, v & (sign >> 1) != 0);
        }
    }

    /// The shift/rotate/inc/dec/TSB/TRB result for `op` on `v` at the M width.
    #[inline(always)]
    fn modify(&mut self, op: u8, v: u16) -> u16 {
        let w16 = self.m16();
        let (mask, sign) = if w16 {
            (0xffffu16, 0x8000u16)
        } else {
            (0xff, 0x80)
        };
        let v = v & mask;
        if op & 0xe7 == 0x04 {
            // TSB ($04/$0C) and TRB ($14/$1C): Z from A AND memory, no N.
            self.flag(Z, self.a & v & mask == 0);
            return if op & 0x10 == 0 {
                v | self.a
            } else {
                v & !self.a
            } & mask;
        }
        let r = match op >> 5 {
            0 => {
                self.flag(C, v & sign != 0);
                (v << 1) & mask
            }
            1 => {
                let c = (self.p & C) as u16;
                self.flag(C, v & sign != 0);
                ((v << 1) | c) & mask
            }
            2 => {
                self.flag(C, v & 1 != 0);
                v >> 1
            }
            3 => {
                let c = if self.p & C != 0 { sign } else { 0 };
                self.flag(C, v & 1 != 0);
                (v >> 1) | c
            }
            6 => v.wrapping_sub(1) & mask,
            7 => v.wrapping_add(1) & mask,
            _ => unreachable!(),
        };
        self.nz(r, w16);
        r
    }

    #[inline(always)]
    fn rmw(&mut self, bus: &mut impl Bus, op: u8, ea: u32) {
        let w16 = self.m16();
        let v = self.rd(bus, ea, w16);
        if self.e {
            // Emulation mode writes the unmodified value back during the modify cycle.
            bus.write(ea & 0xffffff, v as u8);
        } else {
            bus.idle();
        }
        let r = self.modify(op, v);
        if w16 {
            bus.write(Self::next(ea), (r >> 8) as u8);
        }
        bus.write(ea & 0xffffff, r as u8);
    }

    /// Accumulator-mode shift/inc/dec: one internal cycle.
    #[inline(always)]
    fn rmw_a(&mut self, bus: &mut impl Bus, op: u8) {
        bus.idle();
        let w16 = self.m16();
        let r = self.modify(op, self.a);
        self.set_a(r, w16);
    }

    #[inline(always)]
    fn branch(&mut self, bus: &mut impl Bus, taken: bool) {
        let off = self.fetch(bus) as i8 as u16;
        if taken {
            bus.idle();
            let pc = self.pc as u16;
            let to = pc.wrapping_add(off);
            if self.e && (pc ^ to) & 0xff00 != 0 {
                bus.idle();
            }
            self.set_pc16(to);
        }
    }

    /// Push PBR (native), PC and P, then jump through the vector in bank 0.
    fn interrupt(&mut self, bus: &mut impl Bus, native: u16, emu: u16, brk: bool) {
        let pc = self.pc as u16;
        if self.e {
            self.push16(bus, pc);
            let p = if brk { self.p | XF } else { self.p & !XF };
            self.push(bus, p);
        } else {
            self.push(bus, (self.pc >> 16) as u8);
            self.push16(bus, pc);
            self.push(bus, self.p);
        }
        self.p = (self.p | I) & !D;
        let vec = if self.e { emu } else { native };
        let lo = bus.read(vec as u32) as u32;
        let hi = bus.read(vec as u32 + 1) as u32;
        self.pc = hi << 8 | lo;
    }

    fn exec(&mut self, bus: &mut impl Bus, op: u8) {
        // Group 1: every odd opcode except the $xB column, plus (dp) at $x2 with an odd high nibble.
        if (op & 1 == 1 && op & 0x0f != 0x0b) || op & 0x1f == 0x12 {
            let k = op >> 5;
            let mode = op & 0x1f;
            let w16 = self.m16();
            if mode == IMM {
                let v = if w16 {
                    self.fetch16(bus)
                } else {
                    self.fetch(bus) as u16
                };
                if k == 4 {
                    self.bit(v, true); // $89 BIT #imm
                } else {
                    self.group1(k, v);
                }
            } else if k == 4 {
                let ea = self.ea(bus, mode, true);
                self.wr(bus, ea, self.a, w16);
            } else {
                let ea = self.ea(bus, mode, false);
                let v = self.rd(bus, ea, w16);
                self.group1(k, v);
            }
            return;
        }

        match op {
            // ---- read-modify-write ----
            0x06 | 0x26 | 0x46 | 0x66 | 0xc6 | 0xe6 | 0x04 | 0x14 => {
                let ea = self.ea(bus, DP, true);
                self.rmw(bus, op, ea);
            }
            0x16 | 0x36 | 0x56 | 0x76 | 0xd6 | 0xf6 => {
                let ea = self.ea(bus, DPX, true);
                self.rmw(bus, op, ea);
            }
            0x0e | 0x2e | 0x4e | 0x6e | 0xce | 0xee | 0x0c | 0x1c => {
                let ea = self.ea(bus, ABS, true);
                self.rmw(bus, op, ea);
            }
            0x1e | 0x3e | 0x5e | 0x7e | 0xde | 0xfe => {
                let ea = self.ea(bus, ABSX, true);
                self.rmw(bus, op, ea);
            }
            0x0a | 0x2a | 0x4a | 0x6a => self.rmw_a(bus, op),
            0x3a => self.rmw_a(bus, 0xc6),
            0x1a => self.rmw_a(bus, 0xe6),

            // ---- index loads, stores, compares ----
            0xa2 | 0xa0 | 0xe0 | 0xc0 => {
                let v = if self.x16() {
                    self.fetch16(bus)
                } else {
                    self.fetch(bus) as u16
                };
                self.index_op(op, v);
            }
            0xa6 | 0xa4 | 0xe4 | 0xc4 | 0xae | 0xac | 0xec | 0xcc | 0xb6 | 0xb4 | 0xbe | 0xbc => {
                let mode = match op {
                    0xa6 | 0xa4 | 0xe4 | 0xc4 => DP,
                    0xae | 0xac | 0xec | 0xcc => ABS,
                    0xb6 => DPY,
                    0xb4 => DPX,
                    0xbe => ABSY,
                    _ => ABSX,
                };
                let ea = self.ea(bus, mode, false);
                let v = self.rd(bus, ea, self.x16());
                self.index_op(op, v);
            }
            0x86 | 0x84 | 0x8e | 0x8c | 0x96 | 0x94 => {
                let mode = match op {
                    0x86 | 0x84 => DP,
                    0x8e | 0x8c => ABS,
                    0x96 => DPY,
                    _ => DPX,
                };
                let ea = self.ea(bus, mode, true);
                let v = if op & 0x02 != 0 { self.x } else { self.y };
                self.wr(bus, ea, v, self.x16());
            }

            // ---- STZ, BIT ----
            0x64 | 0x74 | 0x9c | 0x9e => {
                let mode = match op {
                    0x64 => DP,
                    0x74 => DPX,
                    0x9c => ABS,
                    _ => ABSX,
                };
                let ea = self.ea(bus, mode, true);
                self.wr(bus, ea, 0, self.m16());
            }
            0x24 | 0x2c | 0x34 | 0x3c => {
                let mode = match op {
                    0x24 => DP,
                    0x2c => ABS,
                    0x34 => DPX,
                    _ => ABSX,
                };
                let ea = self.ea(bus, mode, false);
                let v = self.rd(bus, ea, self.m16());
                self.bit(v, false);
            }

            // ---- branches ----
            0x10 => self.branch(bus, self.p & N == 0),
            0x30 => self.branch(bus, self.p & N != 0),
            0x50 => self.branch(bus, self.p & V == 0),
            0x70 => self.branch(bus, self.p & V != 0),
            0x90 => self.branch(bus, self.p & C == 0),
            0xb0 => self.branch(bus, self.p & C != 0),
            0xd0 => self.branch(bus, self.p & Z == 0),
            0xf0 => self.branch(bus, self.p & Z != 0),
            0x80 => self.branch(bus, true),
            0x82 => {
                let off = self.fetch16(bus);
                bus.idle();
                self.set_pc16((self.pc as u16).wrapping_add(off));
            }

            // ---- flags ----
            0x18 | 0x38 | 0x58 | 0x78 | 0xb8 | 0xd8 | 0xf8 => {
                bus.idle();
                match op {
                    0x18 => self.p &= !C,
                    0x38 => self.p |= C,
                    0x58 => self.p &= !I,
                    0x78 => self.p |= I,
                    0xb8 => self.p &= !V,
                    0xd8 => self.p &= !D,
                    _ => self.p |= D,
                }
            }
            0xc2 | 0xe2 => {
                let v = self.fetch(bus);
                bus.idle();
                let p = if op == 0xc2 { self.p & !v } else { self.p | v };
                self.set_p(p);
            }
            0xfb => {
                bus.idle();
                let c = self.p & C != 0;
                self.flag(C, self.e);
                self.e = c;
                if c {
                    self.set_p(self.p);
                    self.fix_s();
                }
            }

            // ---- transfers ----
            0xaa | 0xa8 => {
                bus.idle();
                let w = self.x16();
                let v = if w { self.a } else { self.a & 0xff };
                if op == 0xaa {
                    self.x = v
                } else {
                    self.y = v
                }
                self.nz(v, w);
            }
            0x8a | 0x98 => {
                bus.idle();
                let w = self.m16();
                let v = if op == 0x8a { self.x } else { self.y };
                self.set_a(v, w);
                self.nz(v, w);
            }
            0xba => {
                bus.idle();
                let w = self.x16();
                self.x = if w { self.s } else { self.s & 0xff };
                self.nz(self.x, w);
            }
            0x9a => {
                bus.idle();
                self.s = if self.e {
                    0x100 | (self.x & 0xff)
                } else {
                    self.x
                };
            }
            0x9b | 0xbb => {
                bus.idle();
                let w = self.x16();
                let v = if op == 0x9b { self.x } else { self.y };
                if op == 0x9b {
                    self.y = v
                } else {
                    self.x = v
                }
                self.nz(v, w);
            }
            0x5b => {
                bus.idle();
                self.d = self.a;
                self.nz(self.d, true);
            }
            0x7b => {
                bus.idle();
                self.a = self.d;
                self.nz(self.a, true);
            }
            0x1b => {
                bus.idle();
                self.s = self.a;
                self.fix_s();
            }
            0x3b => {
                bus.idle();
                self.a = self.s;
                self.nz(self.a, true);
            }
            0xeb => {
                bus.idle();
                bus.idle();
                self.a = self.a.rotate_left(8);
                self.nz(self.a, false);
            }
            0xe8 | 0xc8 | 0xca | 0x88 => {
                bus.idle();
                let w = self.x16();
                let mask = if w { 0xffff } else { 0xff };
                let r = if op == 0xe8 || op == 0xca {
                    &mut self.x
                } else {
                    &mut self.y
                };
                *r = if op == 0xe8 || op == 0xc8 {
                    r.wrapping_add(1)
                } else {
                    r.wrapping_sub(1)
                } & mask;
                let v = *r;
                self.nz(v, w);
            }

            // ---- stack ----
            0x48 => {
                bus.idle();
                if self.m16() {
                    self.push16(bus, self.a)
                } else {
                    self.push(bus, self.a as u8)
                }
            }
            0xda | 0x5a => {
                bus.idle();
                let v = if op == 0xda { self.x } else { self.y };
                if self.x16() {
                    self.push16(bus, v)
                } else {
                    self.push(bus, v as u8)
                }
            }
            0x08 => {
                bus.idle();
                self.push(bus, self.p);
            }
            0x8b => {
                bus.idle();
                self.push(bus, self.dbr);
            }
            0x4b => {
                bus.idle();
                self.push(bus, (self.pc >> 16) as u8);
            }
            0x0b => {
                bus.idle();
                self.push16_n(bus, self.d);
                self.fix_s();
            }
            0x68 => {
                bus.idle();
                bus.idle();
                let w = self.m16();
                let v = if w {
                    self.pull16(bus)
                } else {
                    self.pull(bus) as u16
                };
                self.set_a(v, w);
                self.nz(v, w);
            }
            0xfa | 0x7a => {
                bus.idle();
                bus.idle();
                let w = self.x16();
                let v = if w {
                    self.pull16(bus)
                } else {
                    self.pull(bus) as u16
                };
                if op == 0xfa {
                    self.x = v
                } else {
                    self.y = v
                }
                self.nz(v, w);
            }
            0x28 => {
                bus.idle();
                bus.idle();
                let v = self.pull(bus);
                self.set_p(v);
            }
            0xab => {
                bus.idle();
                bus.idle();
                self.dbr = self.pull_n(bus);
                self.fix_s();
                self.nz(self.dbr as u16, false);
            }
            0x2b => {
                bus.idle();
                bus.idle();
                self.d = self.pull16_n(bus);
                self.fix_s();
                self.nz(self.d, true);
            }
            0xf4 => {
                let v = self.fetch16(bus);
                self.push16_n(bus, v);
                self.fix_s();
            }
            0xd4 => {
                let o = self.fetch(bus) as u16;
                self.dl_penalty(bus);
                let a = self.d.wrapping_add(o);
                let lo = bus.read(a as u32) as u16;
                let hi = bus.read(a.wrapping_add(1) as u32) as u16;
                self.push16_n(bus, hi << 8 | lo);
                self.fix_s();
            }
            0x62 => {
                let off = self.fetch16(bus);
                bus.idle();
                self.push16_n(bus, (self.pc as u16).wrapping_add(off));
                self.fix_s();
            }

            // ---- jumps and calls ----
            0x4c => {
                let a = self.fetch16(bus);
                self.set_pc16(a);
            }
            0x5c => {
                let lo = self.fetch16(bus) as u32;
                let bank = self.fetch(bus) as u32;
                self.pc = bank << 16 | lo;
            }
            0x6c => {
                let p = self.fetch16(bus);
                let lo = bus.read(p as u32) as u16;
                let hi = bus.read(p.wrapping_add(1) as u32) as u16;
                self.set_pc16(hi << 8 | lo);
            }
            0x7c => {
                let p = self.fetch16(bus).wrapping_add(self.x);
                bus.idle();
                let lo = bus.read(self.pbr() | p as u32) as u16;
                let hi = bus.read(self.pbr() | p.wrapping_add(1) as u32) as u16;
                self.set_pc16(hi << 8 | lo);
            }
            0xdc => {
                let p = self.fetch16(bus);
                let lo = bus.read(p as u32) as u32;
                let mid = bus.read(p.wrapping_add(1) as u32) as u32;
                let bank = bus.read(p.wrapping_add(2) as u32) as u32;
                self.pc = bank << 16 | mid << 8 | lo;
            }
            0x20 => {
                let a = self.fetch16(bus);
                bus.idle();
                self.push16(bus, (self.pc as u16).wrapping_sub(1));
                self.set_pc16(a);
            }
            0xfc => {
                let lo = self.fetch(bus) as u16;
                self.push16_n(bus, self.pc as u16);
                let hi = self.fetch(bus) as u16;
                bus.idle();
                let p = (hi << 8 | lo).wrapping_add(self.x);
                let lo = bus.read(self.pbr() | p as u32) as u16;
                let hi = bus.read(self.pbr() | p.wrapping_add(1) as u32) as u16;
                self.fix_s();
                self.set_pc16(hi << 8 | lo);
            }
            0x22 => {
                let a = self.fetch16(bus) as u32;
                self.push_n(bus, (self.pc >> 16) as u8);
                bus.idle();
                let bank = self.fetch(bus) as u32;
                self.push16_n(bus, (self.pc as u16).wrapping_sub(1));
                self.fix_s();
                self.pc = bank << 16 | a;
            }
            0x60 => {
                bus.idle();
                bus.idle();
                let a = self.pull16(bus);
                bus.idle();
                self.set_pc16(a.wrapping_add(1));
            }
            0x6b => {
                bus.idle();
                bus.idle();
                let a = self.pull16_n(bus);
                let bank = self.pull_n(bus) as u32;
                self.fix_s();
                self.pc = bank << 16 | a.wrapping_add(1) as u32;
            }
            0x40 => {
                bus.idle();
                bus.idle();
                let p = self.pull(bus);
                self.set_p(p);
                let a = self.pull16(bus);
                if self.e {
                    self.set_pc16(a);
                } else {
                    let bank = self.pull(bus) as u32;
                    self.pc = bank << 16 | a as u32;
                }
            }

            // ---- interrupts and the rest ----
            0x00 | 0x02 => {
                self.fetch(bus); // signature byte
                if op == 0x00 {
                    self.interrupt(bus, 0xffe6, 0xfffe, true);
                } else {
                    self.interrupt(bus, 0xffe4, 0xfff4, true);
                }
            }
            0x42 => {
                // WDM: the signature byte is skipped with an internal cycle, not read.
                bus.idle();
                self.set_pc16((self.pc as u16).wrapping_add(1));
            }
            0xea => bus.idle(),
            0xcb => {
                bus.idle();
                bus.idle();
                self.wait = true;
            }
            0xdb => {
                bus.idle();
                bus.idle();
                self.stop = true;
            }
            0x44 | 0x54 => {
                let dst = self.fetch(bus);
                let src = self.fetch(bus);
                self.dbr = dst;
                let v = bus.read((src as u32) << 16 | self.x as u32);
                bus.write((dst as u32) << 16 | self.y as u32, v);
                bus.idle();
                bus.idle();
                let mask = if self.x16() { 0xffff } else { 0xff };
                if op == 0x54 {
                    self.x = self.x.wrapping_add(1) & mask;
                    self.y = self.y.wrapping_add(1) & mask;
                } else {
                    self.x = self.x.wrapping_sub(1) & mask;
                    self.y = self.y.wrapping_sub(1) & mask;
                }
                self.a = self.a.wrapping_sub(1);
                if self.a != 0xffff {
                    self.set_pc16((self.pc as u16).wrapping_sub(3));
                }
            }
            _ => unreachable!("opcode {op:02x} is group 1"),
        }
    }

    #[inline(always)]
    fn index_op(&mut self, op: u8, v: u16) {
        let w = self.x16();
        match op & 0xe3 {
            0xa2 => {
                self.x = v;
                self.nz(v, w)
            }
            0xa0 => {
                self.y = v;
                self.nz(v, w)
            }
            0xe0 => self.cmp(self.x, v, w),
            _ => self.cmp(self.y, v, w),
        }
    }
}

#[cfg(test)]
mod tests;
