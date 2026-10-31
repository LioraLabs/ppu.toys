//! One test per instruction group. The full opcode matrix (state and every bus cycle) was
//! checked locally against the MIT-licensed SingleStepTests 65816 suite, which is not vendored.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Ev {
    R(u32),
    W(u32, u8),
    I,
}

struct Ram {
    mem: Vec<u8>,
    log: Vec<Ev>,
}

impl Bus for Ram {
    fn read(&mut self, a: u32) -> u8 {
        self.log.push(Ev::R(a));
        self.mem[a as usize]
    }
    fn write(&mut self, a: u32, v: u8) {
        self.log.push(Ev::W(a, v));
        self.mem[a as usize] = v;
    }
    fn idle(&mut self) {
        self.log.push(Ev::I);
    }
}

/// A native-mode CPU (M=X=0 unless `p` says otherwise) with `code` at $00:8000.
fn setup(code: &[u8], native: bool, p: u8) -> (Cpu, Ram) {
    let mut ram = Ram {
        mem: vec![0; 1 << 24],
        log: Vec::new(),
    };
    ram.mem[0x8000..0x8000 + code.len()].copy_from_slice(code);
    let mut cpu = Cpu {
        pc: 0x8000,
        s: 0x01ff,
        p,
        e: !native,
        ..Cpu::default()
    };
    if !native {
        cpu.p |= M | XF;
    }
    (cpu, ram)
}

fn step(cpu: &mut Cpu, ram: &mut Ram) -> Vec<Ev> {
    ram.log.clear();
    cpu.step(ram);
    ram.log.clone()
}

#[test]
fn loads_stores_and_widths() {
    // LDA #$1234 (16-bit), STA $10 with DL=0: op, lo, read lo, read hi... cycle counts per width.
    let (mut cpu, mut ram) = setup(
        &[
            0xa9, 0x34, 0x12, 0x85, 0x10, 0xe2, 0x20, 0xa9, 0xff, 0x85, 0x12,
        ],
        true,
        0,
    );
    assert_eq!(step(&mut cpu, &mut ram).len(), 3);
    assert_eq!(cpu.a, 0x1234);
    assert_eq!(
        step(&mut cpu, &mut ram),
        [
            Ev::R(0x8003),
            Ev::R(0x8004),
            Ev::W(0x10, 0x34),
            Ev::W(0x11, 0x12)
        ]
    );
    step(&mut cpu, &mut ram); // SEP #$20
    assert_eq!(step(&mut cpu, &mut ram).len(), 2);
    assert_eq!(cpu.a, 0x12ff, "8-bit LDA keeps B");
    assert_eq!(cpu.p & (N | Z), N);
    assert_eq!(step(&mut cpu, &mut ram).len(), 3);
    assert_eq!(ram.mem[0x12..0x14], [0xff, 0]);
}

#[test]
#[allow(clippy::type_complexity)]
fn binary_and_decimal_arithmetic() {
    // (p, a, opcode, operand bytes, carry in, result, carry out)
    let cases: &[(u8, u16, u8, &[u8], bool, u16, bool)] = &[
        (M | XF, 0x7f, 0x69, &[0x01], false, 0x80, false),
        (D | M | XF, 0x15, 0x69, &[0x27], false, 0x42, false),
        (D | M | XF, 0x99, 0x69, &[0x01], false, 0x00, true),
        (D | M | XF, 0x42, 0xe9, &[0x15], true, 0x27, true),
        (D | M | XF, 0x00, 0xe9, &[0x01], true, 0x99, false),
        (D, 0x9999, 0x69, &[0x01, 0x00], false, 0x0000, true),
        (D, 0x1000, 0xe9, &[0x01, 0x00], true, 0x0999, true),
        (0, 0x8000, 0xe9, &[0x01, 0x00], true, 0x7fff, true),
    ];
    for &(p, a, op, operand, c, want, cout) in cases {
        let mut code = vec![op];
        code.extend_from_slice(operand);
        let (mut cpu, mut ram) = setup(&code, true, p | c as u8);
        cpu.a = a;
        step(&mut cpu, &mut ram);
        assert_eq!(
            (
                cpu.a & if p & M != 0 { 0xff } else { 0xffff },
                cpu.p & C != 0
            ),
            (want, cout),
            "{op:02x} {a:x}"
        );
    }
    let (mut cpu, mut ram) = setup(&[0x69, 0x01], true, M | XF);
    cpu.a = 0x7f;
    step(&mut cpu, &mut ram);
    assert!(cpu.p & V != 0, "signed overflow sets V");
}

#[test]
fn addressing_penalties() {
    // DL != 0 adds a cycle to direct page.
    let (mut cpu, mut ram) = setup(&[0xa5, 0x10], true, M | XF);
    cpu.d = 0x0001;
    assert_eq!(
        step(&mut cpu, &mut ram),
        [Ev::R(0x8000), Ev::R(0x8001), Ev::I, Ev::R(0x11)]
    );
    // abs,X with 8-bit X: extra cycle only on a page cross; with 16-bit X always.
    let (mut cpu, mut ram) = setup(&[0xbd, 0x00, 0x10, 0xbd, 0xf0, 0x10], true, M | XF);
    cpu.x = 0x20;
    assert_eq!(step(&mut cpu, &mut ram).len(), 4);
    assert_eq!(step(&mut cpu, &mut ram).len(), 5);
    let (mut cpu, mut ram) = setup(&[0xbd, 0x00, 0x10], true, M);
    assert_eq!(step(&mut cpu, &mut ram).len(), 5);
    // abs,X carries into the next bank.
    let (mut cpu, mut ram) = setup(&[0xbd, 0xff, 0xff], true, M);
    cpu.x = 2;
    assert_eq!(*step(&mut cpu, &mut ram).last().unwrap(), Ev::R(0x010001));
    // Emulation mode, DL=0: dp,X wraps within the page and (dp) fetches its high byte in-page.
    let (mut cpu, mut ram) = setup(&[0xb5, 0xf0, 0xb2, 0xff], false, 0);
    cpu.x = 0x20;
    assert_eq!(*step(&mut cpu, &mut ram).last().unwrap(), Ev::R(0x10));
    assert_eq!(step(&mut cpu, &mut ram)[2..4], [Ev::R(0xff), Ev::R(0x00)]);
    // [dp],Y and (sr,S),Y
    let (mut cpu, mut ram) = setup(&[0xb7, 0x10, 0xb3, 0x02], true, M | XF);
    ram.mem[0x10..0x13].copy_from_slice(&[0x00, 0x20, 0x7e]);
    ram.mem[0x7e2003] = 0x55;
    ram.mem[0x0201..0x0203].copy_from_slice(&[0x00, 0x30]);
    cpu.y = 3;
    step(&mut cpu, &mut ram);
    assert_eq!(cpu.a & 0xff, 0x55);
    assert_eq!(step(&mut cpu, &mut ram).len(), 7);
    assert_eq!(ram.log[6], Ev::R(0x3003));
}

#[test]
fn read_modify_write_cycles() {
    // Native 16-bit: read lo, read hi, internal, write hi, write lo.
    let (mut cpu, mut ram) = setup(&[0x0e, 0x00, 0x20], true, 0);
    ram.mem[0x2000..0x2002].copy_from_slice(&[0x01, 0x80]);
    let ev = step(&mut cpu, &mut ram);
    assert_eq!(
        ev[3..],
        [
            Ev::R(0x2000),
            Ev::R(0x2001),
            Ev::I,
            Ev::W(0x2001, 0x00),
            Ev::W(0x2000, 0x02)
        ]
    );
    assert!(cpu.p & C != 0);
    // Emulation: the modify cycle writes the old value back.
    let (mut cpu, mut ram) = setup(&[0xe6, 0x10], false, 0);
    ram.mem[0x10] = 0x41;
    assert_eq!(
        step(&mut cpu, &mut ram)[2..],
        [Ev::R(0x10), Ev::W(0x10, 0x41), Ev::W(0x10, 0x42)]
    );
    // TSB/TRB: Z from A AND memory.
    let (mut cpu, mut ram) = setup(&[0x04, 0x10, 0x14, 0x10], true, M | XF);
    cpu.a = 0x0f;
    ram.mem[0x10] = 0xf0;
    step(&mut cpu, &mut ram);
    assert_eq!((ram.mem[0x10], cpu.p & Z), (0xff, Z));
    step(&mut cpu, &mut ram);
    assert_eq!((ram.mem[0x10], cpu.p & Z), (0xf0, 0));
}

#[test]
fn branches() {
    // BNE not taken: 2; taken: 3; emulation page cross: 4; native page cross: 3; BRL: 4.
    let (mut cpu, mut ram) = setup(&[0xd0, 0x10], true, Z);
    assert_eq!(step(&mut cpu, &mut ram).len(), 2);
    let (mut cpu, mut ram) = setup(&[0x80, 0x10], true, 0);
    assert_eq!(step(&mut cpu, &mut ram).len(), 3);
    assert_eq!(cpu.pc, 0x8012);
    let (mut cpu, mut ram) = setup(&[0x80, 0x80], false, 0);
    assert_eq!(step(&mut cpu, &mut ram).len(), 4);
    assert_eq!(cpu.pc, 0x7f82);
    let (mut cpu, mut ram) = setup(&[0x80, 0x80], true, 0);
    assert_eq!(step(&mut cpu, &mut ram).len(), 3);
    let (mut cpu, mut ram) = setup(&[0x82, 0xfd, 0xff], true, 0);
    assert_eq!(step(&mut cpu, &mut ram).len(), 4);
    assert_eq!(cpu.pc, 0x8000);
}

#[test]
fn stack_calls_and_returns() {
    // JSL then RTL in native mode.
    let (mut cpu, mut ram) = setup(&[0x22, 0x00, 0x90, 0x01], true, 0);
    ram.mem[0x019000] = 0x6b;
    assert_eq!(step(&mut cpu, &mut ram).len(), 8);
    assert_eq!((cpu.pc, cpu.s), (0x019000, 0x01fc));
    assert_eq!(ram.mem[0x01fd..0x0200], [0x03, 0x80, 0x00]);
    assert_eq!(step(&mut cpu, &mut ram).len(), 6);
    assert_eq!((cpu.pc, cpu.s), (0x008004, 0x01ff));
    // JSR/RTS
    let (mut cpu, mut ram) = setup(&[0x20, 0x00, 0x90], true, 0);
    ram.mem[0x9000] = 0x60;
    assert_eq!(step(&mut cpu, &mut ram).len(), 6);
    assert_eq!(step(&mut cpu, &mut ram).len(), 6);
    assert_eq!(cpu.pc, 0x8003);
    // Emulation: PHA wraps within page 1; PLB (a 65816 instruction) reads past it at $0200.
    let (mut cpu, mut ram) = setup(&[0x48, 0xab], false, 0);
    cpu.s = 0x0100;
    cpu.a = 0x77;
    step(&mut cpu, &mut ram);
    assert_eq!((ram.mem[0x100], cpu.s), (0x77, 0x01ff));
    ram.mem[0x0200] = 0x42;
    step(&mut cpu, &mut ram);
    assert_eq!((cpu.dbr, cpu.s), (0x42, 0x0100));
    // 16-bit PHA pushes high first; PEA/PEI/PER.
    let (mut cpu, mut ram) = setup(&[0x48, 0xf4, 0x34, 0x12, 0x62, 0x00, 0x00], true, 0);
    cpu.a = 0xbeef;
    assert_eq!(
        step(&mut cpu, &mut ram)[2..],
        [Ev::W(0x1ff, 0xbe), Ev::W(0x1fe, 0xef)]
    );
    assert_eq!(step(&mut cpu, &mut ram).len(), 5);
    assert_eq!(step(&mut cpu, &mut ram).len(), 6);
    assert_eq!(ram.mem[0x1fa..0x1fe], [0x07, 0x80, 0x34, 0x12]);
}

#[test]
fn modes_flags_and_transfers() {
    // XCE to native, REP #$30, TAX copies 16 bits; SEP #$10 clears XH; XCE back forces S page 1.
    let (mut cpu, mut ram) = setup(
        &[0x18, 0xfb, 0xc2, 0x30, 0xaa, 0xe2, 0x10, 0x38, 0xfb],
        false,
        0,
    );
    cpu.a = 0x1234;
    cpu.s = 0x01ff;
    for _ in 0..3 {
        step(&mut cpu, &mut ram);
    }
    assert!(!cpu.e && cpu.p & (M | XF) == 0);
    step(&mut cpu, &mut ram);
    assert_eq!(cpu.x, 0x1234);
    step(&mut cpu, &mut ram);
    assert_eq!(cpu.x, 0x34);
    step(&mut cpu, &mut ram);
    step(&mut cpu, &mut ram);
    assert!(cpu.e && cpu.p & (M | XF) == M | XF);
    // REP cannot clear M/X in emulation mode.
    let (mut cpu, mut ram) = setup(&[0xc2, 0x30], false, 0);
    step(&mut cpu, &mut ram);
    assert_eq!(cpu.p & (M | XF), M | XF);
    // XBA: 3 cycles, flags from the new low byte.
    let (mut cpu, mut ram) = setup(&[0xeb], true, M);
    cpu.a = 0x8000;
    assert_eq!(step(&mut cpu, &mut ram).len(), 3);
    assert_eq!((cpu.a, cpu.p & (N | Z)), (0x0080, N));
}

#[test]
fn block_moves() {
    // MVN $7E,$01 moving 3 bytes: one byte (7 cycles) per step.
    let (mut cpu, mut ram) = setup(&[0x54, 0x7e, 0x01], true, 0);
    ram.mem[0x011000..0x011003].copy_from_slice(&[1, 2, 3]);
    cpu.a = 2;
    cpu.x = 0x1000;
    cpu.y = 0x2000;
    for _ in 0..3 {
        assert_eq!(step(&mut cpu, &mut ram).len(), 7);
    }
    assert_eq!(
        (cpu.pc, cpu.a, cpu.x, cpu.y, cpu.dbr),
        (0x8003, 0xffff, 0x1003, 0x2003, 0x7e)
    );
    assert_eq!(ram.mem[0x7e2000..0x7e2003], [1, 2, 3]);
    // MVP decrements.
    let (mut cpu, mut ram) = setup(&[0x44, 0x00, 0x00], true, 0);
    ram.mem[0x10] = 9;
    cpu.x = 0x10;
    cpu.y = 0x20;
    step(&mut cpu, &mut ram);
    assert_eq!(
        (ram.mem[0x20], cpu.x, cpu.y, cpu.pc),
        (9, 0x0f, 0x1f, 0x8003)
    );
}

fn vectors(ram: &mut Ram) {
    for (i, v) in (0xffe4u32..0x10000).step_by(2).enumerate() {
        ram.mem[v as usize] = i as u8; // vector at $FFE4 + 2i points to $9000 + i
        ram.mem[v as usize + 1] = 0x90;
    }
}

#[test]
fn interrupts_and_vectors() {
    // (native, how, vector address)
    let cases: &[(bool, &str, u32)] = &[
        (true, "nmi", 0xffea),
        (true, "irq", 0xffee),
        (true, "brk", 0xffe6),
        (true, "cop", 0xffe4),
        (false, "nmi", 0xfffa),
        (false, "irq", 0xfffe),
        (false, "brk", 0xfffe),
        (false, "cop", 0xfff4),
    ];
    for &(native, how, vec) in cases {
        let code: &[u8] = match how {
            "brk" => &[0x00, 0x00],
            "cop" => &[0x02, 0x00],
            _ => &[0xea],
        };
        let (mut cpu, mut ram) = setup(code, native, D | C);
        vectors(&mut ram);
        cpu.pc = 0x058000;
        ram.mem[0x058000..0x058000 + code.len()].copy_from_slice(code);
        match how {
            "nmi" => cpu.nmi(),
            "irq" => cpu.set_irq(true),
            _ => {}
        }
        let ev = step(&mut cpu, &mut ram);
        let want_pc = 0x9000 + (vec - 0xffe4) / 2;
        assert_eq!(cpu.pc, want_pc, "{how} native={native}");
        assert_eq!(cpu.p & (I | D), I, "{how}: I set, D clear");
        let pushes = if native { 4 } else { 3 };
        assert_eq!(ev.len(), 2 + pushes + 2, "{how} native={native}");
        let (pc_hi, pc_lo) = if how == "brk" || how == "cop" {
            (0x80, 0x02)
        } else {
            (0x80, 0x00)
        };
        if native {
            assert_eq!(ram.mem[0x01fc..0x0200], [D | C, pc_lo, pc_hi, 0x05]);
        } else {
            let b = if how == "brk" || how == "cop" { XF } else { 0 };
            assert_eq!(ram.mem[0x01fd..0x0200], [D | C | M | b, pc_lo, pc_hi]);
        }
    }
}

#[test]
fn interrupt_lines_wai_stp_rti() {
    // IRQ is masked by I; NMI is an edge taken once.
    let (mut cpu, mut ram) = setup(&[0xea, 0xea, 0xea], true, I);
    vectors(&mut ram);
    ram.mem[0x9003] = 0x40; // NMI handler: RTI
    cpu.set_irq(true);
    step(&mut cpu, &mut ram);
    assert_eq!(cpu.pc, 0x8001);
    cpu.nmi();
    step(&mut cpu, &mut ram);
    assert_eq!(cpu.pc, 0x9003);
    assert_eq!(
        step(&mut cpu, &mut ram).len(),
        7,
        "native RTI pulls P, PC and PBR"
    );
    assert_eq!((cpu.pc, cpu.p & I), (0x8001, I));
    step(&mut cpu, &mut ram);
    assert_eq!(cpu.pc, 0x8002);
    // WAI: waits with one idle per step; an IRQ with I set wakes it without being taken.
    let (mut cpu, mut ram) = setup(&[0xcb, 0xea], true, I);
    assert_eq!(step(&mut cpu, &mut ram).len(), 3);
    assert!(cpu.waiting());
    assert_eq!(step(&mut cpu, &mut ram), [Ev::I]);
    cpu.set_irq(true);
    step(&mut cpu, &mut ram);
    assert!(!cpu.waiting());
    assert_eq!(cpu.pc, 0x8002);
    // STP stops for good; reset restarts through $FFFC.
    let (mut cpu, mut ram) = setup(&[0xdb], true, 0);
    ram.mem[0xfffc..0xfffe].copy_from_slice(&[0x34, 0x12]);
    step(&mut cpu, &mut ram);
    cpu.nmi();
    assert_eq!(step(&mut cpu, &mut ram), [Ev::I]);
    assert!(cpu.stopped());
    cpu.reset(&mut ram);
    assert!(!cpu.waiting() && cpu.e && cpu.p & (M | XF | I) == M | XF | I);
    assert_eq!(cpu.pc, 0x1234);
}

#[test]
fn speed_smoke() {
    // A tight loop on flat RAM: DEX/BNE plus loads and stores; checks nothing allocates per step.
    let (mut cpu, mut ram) = setup(
        &[
            0xa2, 0x00, 0xb5, 0x10, 0x95, 0x20, 0xca, 0xd0, 0xf9, 0x80, 0xf5,
        ],
        true,
        M | XF,
    );
    for _ in 0..10_000 {
        cpu.step(&mut ram);
        ram.log.clear();
    }
    assert!(cpu.pc >= 0x8000 && cpu.pc < 0x800b);
}
