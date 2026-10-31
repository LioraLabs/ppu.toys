//! The SPC700 side: CPU, 64 KB audio RAM, timers, the four ports, and ppu-core's S-DSP.
//! Written from hardware documentation only.
//!
//! Interface notes for `system`:
//! - The ports are mailbox state at the time of access: call `run_until(now)` before every
//!   `cpu_read_port`/`cpu_write_port`, or the 65816's handshake loops starve the SPC700.
//! - Clock: SPC700 cycles = master clocks * 1.02528 MHz / 21.477272 MHz. One stereo sample frame
//!   is rendered every 32 SPC700 cycles (32.04 kHz), so ENVX/OUTX reads are current.
//! - Additions to the skeleton: `ram`, `dsp` and `cycles` are public for tools and tests.
//!
//! Boot: the console's IPL ROM is not included. While $F1 bit 7 maps the IPL region and the PC
//! is in $FFC0-$FFFF, `boot_step` performs the documented upload protocol instead of
//! executing instructions: $AA/$BB on ports 0/1, wait for a $CC kick, then blocks of
//! (index on port 0, data on port 1) echoed back, a kick ahead of the expected index ending
//! a block, and port 1 = 0 jumping to the address on ports 2/3. Each port access happens at
//! the SPC700 cycle the documented boot program makes it, so uploads take hardware's time.

use ppu_core::Dsp;

const N: u8 = 0x80;
const V: u8 = 0x40;
const P: u8 = 0x20;
const B: u8 = 0x10;
const H: u8 = 0x08;
const I: u8 = 0x04;
const Z: u8 = 0x02;
const C: u8 = 0x01;

/// The APU's resonator is nominally 24.576 MHz (SPC700 1.024 MHz, 32000 Hz samples), but consoles
/// measure about 0.13% fast, 32040 Hz samples: the rate bsnes and Snes9x run, and the one a
/// cartridge that reads the envelopes live is timed against.
const SPC_HZ: u128 = 1_025_280;
const MASTER_HZ: u128 = 21_477_272;

#[derive(Clone, Copy, Default)]
struct Timer {
    enabled: bool,
    target: u8,
    stage: u8,
    counter: u8,
    acc: u32,
}

/// Where the boot routine is, as the port access it makes next (see `boot_step`).
#[derive(Clone, Copy, PartialEq, Debug)]
enum Boot {
    /// Not inside the boot region yet: the next boot step resets and signals ready.
    Enter,
    /// $AA/$BB go out on ports 0/1 when the zero-fill loop ends.
    Ready,
    /// `cmp $f4,#$cc` reads port 0.
    WaitKick,
    /// `movw ya,$f4` has read ports 0/1 (2/3 a few cycles before): the command.
    Command,
    /// `mov $f4,a` echoes the kick, then a block or the jump.
    Echo { kick: u8, more: bool },
    /// `mov y,$f4` reads port 0: a block starts at index 0.
    BlockStart,
    /// `cmp y,$f4` in the transfer loop reads port 0.
    Data { index: u8 },
    /// `mov a,$f5` read the byte; `mov $f4,y` echoes the index.
    Store { index: u8 },
    /// The second `cmp y,$f4` that confirms a kick ahead of the index.
    Confirm { index: u8 },
}

pub struct Apu {
    pub ram: Box<[u8; 0x10000]>,
    pub dsp: Dsp,
    a: u8,
    x: u8,
    y: u8,
    sp: u8,
    pc: u16,
    psw: u8,
    /// SPC700 cycles since power-on.
    pub cycles: u64,
    halted: bool,
    from_cpu: [u8; 4],
    to_cpu: [u8; 4],
    control: u8,
    dsp_addr: u8,
    timers: [Timer; 3],
    sample_acc: u32,
    samples: Vec<i16>,
    boot: Boot,
    /// The SPC700 cycle of the boot routine's next port access.
    boot_at: u64,
}

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}

impl Apu {
    pub fn new() -> Self {
        Apu {
            ram: vec![0u8; 0x10000].into_boxed_slice().try_into().unwrap(),
            dsp: Dsp::new(),
            a: 0,
            x: 0,
            y: 0,
            sp: 0xef,
            pc: 0xffc0,
            psw: 0,
            cycles: 0,
            halted: false,
            from_cpu: [0; 4],
            to_cpu: [0; 4],
            control: 0x80,
            dsp_addr: 0,
            timers: [Timer::default(); 3],
            sample_acc: 0,
            samples: Vec::new(),
            boot: Boot::Enter,
            boot_at: 0,
        }
    }
    /// $2140-$2143 as the 65816 reads them.
    pub fn cpu_read_port(&mut self, port: u8) -> u8 {
        self.to_cpu[(port & 3) as usize]
    }
    /// $2140-$2143 as the 65816 writes them.
    pub fn cpu_write_port(&mut self, port: u8, value: u8) {
        self.from_cpu[(port & 3) as usize] = value;
    }
    /// Run the audio side up to `master_clock` (21.477 MHz NTSC master clocks since power-on).
    pub fn run_until(&mut self, master_clock: u64) {
        let target = (master_clock as u128 * SPC_HZ / MASTER_HZ) as u64;
        while self.cycles < target {
            if self.halted {
                // SLEEP/STOP: the CPU is gone but timers and the DSP keep running.
                let n = (target - self.cycles).min(1 << 20) as u32;
                self.tick(n);
                continue;
            }
            if self.control & 0x80 != 0 && self.pc >= 0xffc0 {
                // The boot routine acts at the exact cycle of each port access, so the 65816 sees
                // an echo no earlier, and the routine sees a write no later, than hardware.
                if self.boot == Boot::Enter {
                    self.boot_at = self.cycles;
                }
                if self.boot_at > target {
                    self.tick((target - self.cycles) as u32);
                    break;
                }
                self.tick((self.boot_at - self.cycles) as u32);
                self.boot_at += self.boot_step() as u64;
                continue;
            }
            let n = self.exec();
            self.tick(n);
        }
    }
    /// Stereo 32.04 kHz samples produced since the last call, interleaved L, R.
    pub fn take_samples(&mut self, out: &mut Vec<i16>) {
        out.append(&mut self.samples);
    }

    fn tick(&mut self, n: u32) {
        self.cycles += n as u64;
        for (i, t) in self.timers.iter_mut().enumerate() {
            let period = if i == 2 { 16 } else { 128 };
            t.acc += n;
            while t.acc >= period {
                t.acc -= period;
                if t.enabled {
                    t.stage = t.stage.wrapping_add(1);
                    // A target of 0 means 256: the stage wraps to 0 after 256 steps.
                    if t.stage == t.target {
                        t.stage = 0;
                        t.counter = (t.counter + 1) & 15;
                    }
                }
            }
        }
        self.sample_acc += n;
        while self.sample_acc >= 32 {
            self.sample_acc -= 32;
            let mut frame = [0i16; 2];
            self.dsp.render(&mut self.ram, &mut frame);
            // shortcut: unbounded if the host never takes samples; cap it if a host ever doesn't.
            self.samples.extend_from_slice(&frame);
        }
    }

    /// The boot routine's port access due now; returns the SPC700 cycles to its next one.
    ///
    /// Timed as the documented IPL program runs (fullsnes "SNES APU Boot ROM"), counting each
    /// instruction's documented SPC700 cycles to the cycle of its port access: a read in the last
    /// cycle of `cmp y,dp` (3), `mov a,dp` (3), `mov y,dp` (3), `movw ya,dp` (5, high byte last),
    /// the 4th of `cmp dp,#imm` (5), a write in the last cycle of `mov dp,y`/`mov dp,a` (4); taken
    /// branches 4, untaken 2.
    fn boot_step(&mut self) -> u32 {
        match self.boot {
            Boot::Enter => {
                // mov x,#$ef; mov sp,x; mov a,#0 (6), then mov (x),a; dec x; bne (10) for
                // x = $EF..1, the last bne untaken: $01-$EF are zeroed, $00 is not.
                self.sp = 0xef;
                self.psw = 0;
                self.ram[1..0xf0].fill(0);
                self.boot = Boot::Ready;
                6 + 0xef * 10 - 2 + 5 + 5
            }
            Boot::Ready => {
                // mov $f4,#$aa; mov $f5,#$bb (5 each), then cmp $f4,#$cc reads in its 4th cycle.
                self.to_cpu[0] = 0xaa;
                self.to_cpu[1] = 0xbb;
                self.boot = Boot::WaitKick;
                4
            }
            Boot::WaitKick => {
                if self.from_cpu[0] != 0xcc {
                    return 1 + 4 + 4;
                }
                // The cmp's last cycle, bne untaken, bra main (4); main's movw ya,$f6 /
                // movw $00,ya (10) and movw ya,$f4 (5).
                self.boot = Boot::Command;
                1 + 2 + 4 + 15
            }
            Boot::Command => {
                // movw ya,$f6 stored ports 2/3 at $00/$01: the block address or the jump target.
                // shortcut: ports 2/3 are taken here, 10-12 cycles after hardware reads them; the
                // 65816 sets them before the kick and waits for the echo, so no host sees it.
                self.ram[0] = self.from_cpu[2];
                self.ram[1] = self.from_cpu[3];
                self.boot = Boot::Echo {
                    kick: self.from_cpu[0],
                    more: self.from_cpu[1] != 0,
                };
                4
            }
            Boot::Echo { kick, more } => {
                self.to_cpu[0] = kick;
                if more {
                    // mov a,y; mov x,a (4), bne taken (4), mov y,$f4 (3).
                    self.boot = Boot::BlockStart;
                    return 4 + 4 + 3;
                }
                // mov a,y; mov x,a; bne untaken; jmp [!$0000+x] (6): the program starts with
                // A = the kick, X = Y = 0 (port 1), Z set by mov x,a.
                self.a = kick;
                self.x = 0;
                self.y = 0;
                self.psw = (self.psw & !N) | Z;
                self.boot = Boot::Enter;
                self.pc = u16::from_le_bytes([self.ram[0], self.ram[1]]);
                // The program's first instruction starts 12 cycles on.
                self.tick(4 + 2 + 6);
                0
            }
            Boot::BlockStart => {
                if self.from_cpu[0] != 0 {
                    return 4 + 3;
                }
                // bne untaken, cmp y,$f4.
                self.boot = Boot::Data { index: 0 };
                2 + 3
            }
            Boot::Data { index } => {
                let p = self.from_cpu[0];
                if p == index {
                    // bne untaken, mov a,$f5 (3), mov $f4,y (4).
                    self.boot = Boot::Store { index };
                    return 2 + 3 + 4;
                }
                // bne taken to `bpl`, which loops on cmp's N (index behind) or re-checks.
                if (index.wrapping_sub(p) as i8) >= 0 {
                    4 + 4 + 3
                } else {
                    self.boot = Boot::Confirm { index };
                    4 + 2 + 3
                }
            }
            Boot::Store { index } => {
                // shortcut: port 1 is taken at the echo, 4 cycles after hardware's read; the 65816
                // cannot change it in between, as it is waiting for this echo.
                self.to_cpu[0] = index;
                let addr =
                    u16::from_le_bytes([self.ram[0], self.ram[1]]).wrapping_add(index as u16);
                self.ram[addr as usize] = self.from_cpu[1];
                let index = index.wrapping_add(1);
                // mov [$00]+y,a (7), inc y (2), then bne taken (4) and cmp y,$f4 (3).
                if index != 0 {
                    self.boot = Boot::Data { index };
                    return 7 + 2 + 4 + 3;
                }
                // bne untaken, inc $01 (4), then bpl on its N: taken back to cmp, else the
                // second cmp.
                self.ram[1] = self.ram[1].wrapping_add(1);
                if self.ram[1] & 0x80 == 0 {
                    self.boot = Boot::Data { index };
                    7 + 2 + 2 + 4 + 4 + 3
                } else {
                    self.boot = Boot::Confirm { index };
                    7 + 2 + 2 + 4 + 2 + 3
                }
            }
            Boot::Confirm { index } => {
                if (index.wrapping_sub(self.from_cpu[0]) as i8) >= 0 {
                    // bpl taken back to the transfer loop's cmp.
                    self.boot = Boot::Data { index };
                    return 4 + 3;
                }
                // bpl untaken into main: movw ya,$f6 / movw $00,ya (10), movw ya,$f4 (5).
                self.boot = Boot::Command;
                2 + 15
            }
        }
    }

    // --- memory ---

    fn read(&mut self, addr: u16) -> u8 {
        if addr & 0xfff0 != 0x00f0 {
            // shortcut: reads of $FFC0-$FFFF see RAM even while the IPL region is mapped; no
            // ROM bytes exist here. Matters only for code that reads the IPL ROM itself.
            return self.ram[addr as usize];
        }
        match addr {
            0xf2 => self.dsp_addr,
            0xf3 => self.dsp.read(self.dsp_addr),
            0xf4..=0xf7 => self.from_cpu[(addr - 0xf4) as usize],
            0xf8 | 0xf9 => self.ram[addr as usize],
            0xfd..=0xff => {
                let t = &mut self.timers[(addr - 0xfd) as usize];
                std::mem::take(&mut t.counter)
            }
            _ => 0,
        }
    }

    fn write(&mut self, addr: u16, v: u8) {
        self.ram[addr as usize] = v;
        if addr & 0xfff0 != 0x00f0 {
            return;
        }
        match addr {
            0xf1 => {
                for (i, t) in self.timers.iter_mut().enumerate() {
                    let on = v & (1 << i) != 0;
                    if on && !t.enabled {
                        t.stage = 0;
                        t.counter = 0;
                    }
                    t.enabled = on;
                }
                if v & 0x10 != 0 {
                    self.from_cpu[0] = 0;
                    self.from_cpu[1] = 0;
                }
                if v & 0x20 != 0 {
                    self.from_cpu[2] = 0;
                    self.from_cpu[3] = 0;
                }
                self.control = v;
            }
            0xf2 => self.dsp_addr = v,
            0xf3 => self.dsp.write(self.dsp_addr, v),
            0xf4..=0xf7 => self.to_cpu[(addr - 0xf4) as usize] = v,
            0xfa..=0xfc => self.timers[(addr - 0xfa) as usize].target = v,
            _ => {}
        }
    }

    fn read16(&mut self, addr: u16) -> u16 {
        u16::from_le_bytes([self.read(addr), self.read(addr.wrapping_add(1))])
    }

    fn dp(&self, off: u8) -> u16 {
        (((self.psw & P) as u16) << 3) | off as u16
    }

    /// A 16-bit direct-page word; the high byte wraps within the page.
    fn read16_dp(&mut self, off: u8) -> u16 {
        let lo = self.read(self.dp(off));
        let hi = self.read(self.dp(off.wrapping_add(1)));
        u16::from_le_bytes([lo, hi])
    }

    fn write16_dp(&mut self, off: u8, v: u16) {
        self.write(self.dp(off), v as u8);
        self.write(self.dp(off.wrapping_add(1)), (v >> 8) as u8);
    }

    fn fetch(&mut self) -> u8 {
        let v = self.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        v
    }

    fn fetch16(&mut self) -> u16 {
        u16::from_le_bytes([self.fetch(), self.fetch()])
    }

    fn push(&mut self, v: u8) {
        self.write(0x100 | self.sp as u16, v);
        self.sp = self.sp.wrapping_sub(1);
    }

    fn pop(&mut self) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        self.read(0x100 | self.sp as u16)
    }

    fn push16(&mut self, v: u16) {
        self.push((v >> 8) as u8);
        self.push(v as u8);
    }

    fn pop16(&mut self) -> u16 {
        u16::from_le_bytes([self.pop(), self.pop()])
    }

    // --- flags and ALU ---

    fn set(&mut self, flag: u8, on: bool) {
        if on {
            self.psw |= flag
        } else {
            self.psw &= !flag
        }
    }

    fn nz(&mut self, v: u8) {
        self.psw = (self.psw & !(N | Z)) | (v & N) | if v == 0 { Z } else { 0 };
    }

    fn nz16(&mut self, v: u16) {
        self.set(N, v & 0x8000 != 0);
        self.set(Z, v == 0);
    }

    fn adc(&mut self, a: u8, b: u8) -> u8 {
        let r = a as u16 + b as u16 + (self.psw & C) as u16;
        let r8 = r as u8;
        self.set(C, r > 0xff);
        self.set(H, (a ^ b ^ r8) & 0x10 != 0);
        self.set(V, !(a ^ b) & (a ^ r8) & 0x80 != 0);
        self.nz(r8);
        r8
    }

    fn cmp(&mut self, a: u8, b: u8) {
        self.set(C, a >= b);
        self.nz(a.wrapping_sub(b));
    }

    /// OR, AND, EOR, CMP, ADC, SBC by the opcode's high nibble / 2. CMP returns `a` unchanged.
    fn alu(&mut self, op: u8, a: u8, b: u8) -> u8 {
        let r = match op {
            0 => a | b,
            1 => a & b,
            2 => a ^ b,
            3 => {
                self.cmp(a, b);
                return a;
            }
            4 => return self.adc(a, b),
            _ => return self.adc(a, !b),
        };
        self.nz(r);
        r
    }

    /// ASL, ROL, LSR, ROR, DEC, INC by the opcode's high nibble / 2.
    fn rmw(&mut self, op: u8, v: u8) -> u8 {
        let c = self.psw & C;
        let r = match op {
            0 | 1 => {
                self.set(C, v & 0x80 != 0);
                (v << 1) | if op == 1 { c } else { 0 }
            }
            2 | 3 => {
                self.set(C, v & 1 != 0);
                (v >> 1) | if op == 3 { c << 7 } else { 0 }
            }
            4 => v.wrapping_sub(1),
            _ => v.wrapping_add(1),
        };
        self.nz(r);
        r
    }

    fn branch(&mut self, cond: bool) -> u32 {
        let rel = self.fetch() as i8;
        if cond {
            self.pc = self.pc.wrapping_add(rel as u16);
            2
        } else {
            0
        }
    }

    /// Effective address of columns 4-7 (and its read cycle count).
    fn ea(&mut self, op: u8) -> (u16, u32) {
        let odd = op & 0x10 != 0;
        match (op & 0x0f, odd) {
            (4, false) => {
                let o = self.fetch();
                (self.dp(o), 3)
            }
            (4, true) => {
                let o = self.fetch().wrapping_add(self.x);
                (self.dp(o), 4)
            }
            (5, false) => (self.fetch16(), 4),
            (5, true) => (self.fetch16().wrapping_add(self.x as u16), 5),
            (6, false) => (self.dp(self.x), 3),
            (6, true) => (self.fetch16().wrapping_add(self.y as u16), 5),
            (_, false) => {
                let o = self.fetch().wrapping_add(self.x);
                (self.read16_dp(o), 6)
            }
            (_, true) => {
                let o = self.fetch();
                (self.read16_dp(o).wrapping_add(self.y as u16), 6)
            }
        }
    }

    fn mem_bit(&mut self) -> (u16, u8, bool) {
        let w = self.fetch16();
        let addr = w & 0x1fff;
        let bit = (w >> 13) as u8;
        let v = self.read(addr);
        (addr, bit, v & (1 << bit) != 0)
    }

    /// Execute one instruction; returns its SPC700 cycles.
    // shortcut: no dummy reads, so read-modify-write and store instructions never clear
    // $FD-$FF the way hardware's extra reads can. Add them if a game depends on it.
    fn exec(&mut self) -> u32 {
        let op = self.fetch();
        let h = op >> 4;
        match op & 0x0f {
            0x0 if h & 1 == 1 => {
                let cond = match h {
                    0x1 => self.psw & N == 0,
                    0x3 => self.psw & N != 0,
                    0x5 => self.psw & V == 0,
                    0x7 => self.psw & V != 0,
                    0x9 => self.psw & C == 0,
                    0xb => self.psw & C != 0,
                    0xd => self.psw & Z == 0,
                    _ => self.psw & Z != 0,
                };
                2 + self.branch(cond)
            }
            0x0 => match h {
                0x0 => 2,
                0x2 => {
                    self.psw &= !P;
                    2
                }
                0x4 => {
                    self.psw |= P;
                    2
                }
                0x6 => {
                    self.psw &= !C;
                    2
                }
                0x8 => {
                    self.psw |= C;
                    2
                }
                0xa => {
                    self.psw |= I;
                    3
                }
                0xc => {
                    self.psw &= !I;
                    3
                }
                _ => {
                    self.psw &= !(V | H);
                    2
                }
            },
            0x1 => {
                // TCALL n
                self.push16(self.pc);
                self.pc = self.read16(0xffde - 2 * h as u16);
                8
            }
            0x2 => {
                let o = self.fetch();
                let a = self.dp(o);
                let v = self.read(a);
                let m = 1 << (h >> 1);
                self.write(a, if h & 1 == 0 { v | m } else { v & !m });
                4
            }
            0x3 => {
                let o = self.fetch();
                let a = self.dp(o);
                let set = self.read(a) & (1 << (h >> 1)) != 0;
                5 + self.branch(set == (h & 1 == 0))
            }
            0x4..=0x7 => {
                let (addr, c) = self.ea(op);
                match h {
                    0xc | 0xd => {
                        self.write(addr, self.a);
                        c + 1
                    }
                    0xe | 0xf => {
                        self.a = self.read(addr);
                        self.nz(self.a);
                        c
                    }
                    _ => {
                        let v = self.read(addr);
                        self.a = self.alu(h >> 1, self.a, v);
                        c
                    }
                }
            }
            0x8 if h < 0xc => {
                let imm = self.fetch();
                if h & 1 == 0 {
                    self.a = self.alu(h >> 1, self.a, imm);
                    2
                } else {
                    let o = self.fetch();
                    let d = self.dp(o);
                    let v = self.read(d);
                    let r = self.alu(h >> 1, v, imm);
                    if h >> 1 != 3 {
                        self.write(d, r);
                    }
                    5
                }
            }
            0x9 if h < 0xc => {
                let (s, d, c) = if h & 1 == 0 {
                    let o = self.fetch();
                    let s = self.dp(o);
                    let s = self.read(s);
                    let o = self.fetch();
                    (s, self.dp(o), 6)
                } else {
                    let s = self.dp(self.y);
                    (self.read(s), self.dp(self.x), 5)
                };
                let v = self.read(d);
                let r = self.alu(h >> 1, v, s);
                if h >> 1 != 3 {
                    self.write(d, r);
                }
                c
            }
            0xb if h < 0xc => {
                let o = self.fetch();
                let a = self.dp(if h & 1 == 0 {
                    o
                } else {
                    o.wrapping_add(self.x)
                });
                let v = self.read(a);
                let r = self.rmw(h >> 1, v);
                self.write(a, r);
                4 + (h & 1) as u32
            }
            0xc if h < 0xc => {
                if h & 1 == 0 {
                    let a = self.fetch16();
                    let v = self.read(a);
                    let r = self.rmw(h >> 1, v);
                    self.write(a, r);
                    5
                } else {
                    self.a = self.rmw(h >> 1, self.a);
                    2
                }
            }
            _ => self.exec_misc(op),
        }
    }

    fn exec_misc(&mut self, op: u8) -> u32 {
        match op {
            0xc8 => {
                let v = self.fetch();
                self.cmp(self.x, v);
                2
            }
            0xd8 => {
                let o = self.fetch();
                self.write(self.dp(o), self.x);
                4
            }
            0xe8 => {
                self.a = self.fetch();
                self.nz(self.a);
                2
            }
            0xf8 => {
                let o = self.fetch();
                self.x = self.read(self.dp(o));
                self.nz(self.x);
                3
            }
            0xc9 => {
                let a = self.fetch16();
                self.write(a, self.x);
                5
            }
            0xd9 => {
                let o = self.fetch().wrapping_add(self.y);
                self.write(self.dp(o), self.x);
                5
            }
            0xe9 => {
                let a = self.fetch16();
                self.x = self.read(a);
                self.nz(self.x);
                4
            }
            0xf9 => {
                let o = self.fetch().wrapping_add(self.y);
                self.x = self.read(self.dp(o));
                self.nz(self.x);
                4
            }
            // column A: bit ops and 16-bit ops
            0x0a | 0x2a | 0x4a | 0x6a | 0x8a | 0xaa => {
                let (_, _, bit) = self.mem_bit();
                let c = self.psw & C != 0;
                let (r, cyc) = match op {
                    0x0a => (c | bit, 5),
                    0x2a => (c | !bit, 5),
                    0x4a => (c & bit, 4),
                    0x6a => (c & !bit, 4),
                    0x8a => (c ^ bit, 5),
                    _ => (bit, 4),
                };
                self.set(C, r);
                cyc
            }
            0xca | 0xea => {
                let (addr, b, bit) = self.mem_bit();
                let on = if op == 0xca { self.psw & C != 0 } else { !bit };
                let v = (self.read(addr) & !(1 << b)) | (on as u8) << b;
                self.write(addr, v);
                if op == 0xca {
                    6
                } else {
                    5
                }
            }
            0x1a | 0x3a => {
                let o = self.fetch();
                let v = self.read16_dp(o);
                let v = if op == 0x1a {
                    v.wrapping_sub(1)
                } else {
                    v.wrapping_add(1)
                };
                self.write16_dp(o, v);
                self.nz16(v);
                6
            }
            0x5a => {
                let o = self.fetch();
                let w = self.read16_dp(o);
                let ya = self.ya();
                self.set(C, ya >= w);
                self.nz16(ya.wrapping_sub(w));
                4
            }
            0x7a | 0x9a => {
                let o = self.fetch();
                let w = self.read16_dp(o);
                let (lo, hi) = (w as u8, (w >> 8) as u8);
                if op == 0x7a {
                    self.psw &= !C;
                    self.a = self.adc(self.a, lo);
                    self.y = self.adc(self.y, hi);
                } else {
                    self.psw |= C;
                    self.a = self.adc(self.a, !lo);
                    self.y = self.adc(self.y, !hi);
                }
                self.set(Z, self.a == 0 && self.y == 0);
                5
            }
            0xba => {
                let o = self.fetch();
                let w = self.read16_dp(o);
                self.a = w as u8;
                self.y = (w >> 8) as u8;
                self.nz16(w);
                5
            }
            0xda => {
                let o = self.fetch();
                self.write16_dp(o, self.ya());
                5
            }
            0xfa => {
                let o = self.fetch();
                let v = self.read(self.dp(o));
                let o = self.fetch();
                self.write(self.dp(o), v);
                5
            }
            0xcb => {
                let o = self.fetch();
                self.write(self.dp(o), self.y);
                4
            }
            0xdb => {
                let o = self.fetch().wrapping_add(self.x);
                self.write(self.dp(o), self.y);
                5
            }
            0xeb => {
                let o = self.fetch();
                self.y = self.read(self.dp(o));
                self.nz(self.y);
                3
            }
            0xfb => {
                let o = self.fetch().wrapping_add(self.x);
                self.y = self.read(self.dp(o));
                self.nz(self.y);
                4
            }
            0xcc => {
                let a = self.fetch16();
                self.write(a, self.y);
                5
            }
            0xdc => {
                self.y = self.y.wrapping_sub(1);
                self.nz(self.y);
                2
            }
            0xec => {
                let a = self.fetch16();
                self.y = self.read(a);
                self.nz(self.y);
                4
            }
            0xfc => {
                self.y = self.y.wrapping_add(1);
                self.nz(self.y);
                2
            }
            // column D
            0x0d => {
                self.push(self.psw);
                4
            }
            0x2d => {
                self.push(self.a);
                4
            }
            0x4d => {
                self.push(self.x);
                4
            }
            0x6d => {
                self.push(self.y);
                4
            }
            0x1d => {
                self.x = self.x.wrapping_sub(1);
                self.nz(self.x);
                2
            }
            0x3d => {
                self.x = self.x.wrapping_add(1);
                self.nz(self.x);
                2
            }
            0x5d => {
                self.x = self.a;
                self.nz(self.x);
                2
            }
            0x7d => {
                self.a = self.x;
                self.nz(self.a);
                2
            }
            0x8d => {
                self.y = self.fetch();
                self.nz(self.y);
                2
            }
            0x9d => {
                self.x = self.sp;
                self.nz(self.x);
                2
            }
            0xad => {
                let v = self.fetch();
                self.cmp(self.y, v);
                2
            }
            0xbd => {
                self.sp = self.x;
                2
            }
            0xcd => {
                self.x = self.fetch();
                self.nz(self.x);
                2
            }
            0xdd => {
                self.a = self.y;
                self.nz(self.a);
                2
            }
            0xed => {
                self.psw ^= C;
                3
            }
            0xfd => {
                self.y = self.a;
                self.nz(self.y);
                2
            }
            // column E
            0x0e | 0x4e => {
                let addr = self.fetch16();
                let v = self.read(addr);
                self.nz(self.a.wrapping_sub(v));
                self.write(addr, if op == 0x0e { v | self.a } else { v & !self.a });
                6
            }
            0x1e | 0x5e => {
                let addr = self.fetch16();
                let v = self.read(addr);
                self.cmp(if op == 0x1e { self.x } else { self.y }, v);
                4
            }
            0x3e | 0x7e => {
                let o = self.fetch();
                let v = self.read(self.dp(o));
                self.cmp(if op == 0x3e { self.x } else { self.y }, v);
                3
            }
            0x2e | 0xde => {
                let o = self.fetch();
                let d = self.dp(if op == 0x2e {
                    o
                } else {
                    o.wrapping_add(self.x)
                });
                let v = self.read(d);
                let c = if op == 0x2e { 5 } else { 6 };
                c + self.branch(self.a != v)
            }
            0x6e => {
                let o = self.fetch();
                let d = self.dp(o);
                let v = self.read(d).wrapping_sub(1);
                self.write(d, v);
                5 + self.branch(v != 0)
            }
            0xfe => {
                self.y = self.y.wrapping_sub(1);
                4 + self.branch(self.y != 0)
            }
            0x8e => {
                self.psw = self.pop();
                4
            }
            0xae => {
                self.a = self.pop();
                4
            }
            0xce => {
                self.x = self.pop();
                4
            }
            0xee => {
                self.y = self.pop();
                4
            }
            0x9e => {
                // DIV YA,X, including the documented results when the quotient overflows.
                let ya = self.ya() as u32;
                let (y, x) = (self.y as u32, self.x as u32);
                self.set(V, y >= x);
                self.set(H, (y & 15) >= (x & 15));
                if y < 2 * x {
                    self.a = (ya / x) as u8;
                    self.y = (ya % x) as u8;
                } else {
                    let r = ya - x * 512;
                    self.a = (255 - r / (256 - x)) as u8;
                    self.y = (x + r % (256 - x)) as u8;
                }
                self.nz(self.a);
                12
            }
            0xbe => {
                if self.psw & C == 0 || self.a > 0x99 {
                    self.a = self.a.wrapping_sub(0x60);
                    self.psw &= !C;
                }
                if self.psw & H == 0 || self.a & 15 > 9 {
                    self.a = self.a.wrapping_sub(6);
                }
                self.nz(self.a);
                3
            }
            0xdf => {
                if self.psw & C != 0 || self.a > 0x99 {
                    self.a = self.a.wrapping_add(0x60);
                    self.psw |= C;
                }
                if self.psw & H != 0 || self.a & 15 > 9 {
                    self.a = self.a.wrapping_add(6);
                }
                self.nz(self.a);
                3
            }
            // column F
            0x0f => {
                self.push16(self.pc);
                self.push(self.psw);
                self.psw = (self.psw | B) & !I;
                self.pc = self.read16(0xffde);
                8
            }
            0x1f => {
                let a = self.fetch16().wrapping_add(self.x as u16);
                self.pc = self.read16(a);
                6
            }
            0x2f => 2 + self.branch(true),
            0x3f => {
                let a = self.fetch16();
                self.push16(self.pc);
                self.pc = a;
                8
            }
            0x4f => {
                let a = 0xff00 | self.fetch() as u16;
                self.push16(self.pc);
                self.pc = a;
                6
            }
            0x5f => {
                self.pc = self.fetch16();
                3
            }
            0x6f => {
                self.pc = self.pop16();
                5
            }
            0x7f => {
                self.psw = self.pop();
                self.pc = self.pop16();
                6
            }
            0x8f => {
                let imm = self.fetch();
                let o = self.fetch();
                self.write(self.dp(o), imm);
                5
            }
            0x9f => {
                self.a = self.a.rotate_left(4);
                self.nz(self.a);
                5
            }
            0xaf => {
                self.write(self.dp(self.x), self.a);
                self.x = self.x.wrapping_add(1);
                4
            }
            0xbf => {
                self.a = self.read(self.dp(self.x));
                self.x = self.x.wrapping_add(1);
                self.nz(self.a);
                4
            }
            0xcf => {
                let r = self.y as u16 * self.a as u16;
                self.a = r as u8;
                self.y = (r >> 8) as u8;
                self.nz(self.y);
                9
            }
            0xef | 0xff => {
                self.halted = true;
                3
            }
            _ => unreachable!("opcode {op:#04x} is decoded by exec"),
        }
    }

    fn ya(&self) -> u16 {
        u16::from_le_bytes([self.a, self.y])
    }
}

#[cfg(test)]
mod tests;
