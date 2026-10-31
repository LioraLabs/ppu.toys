use super::*;

/// Run hand-assembled `code` from $0200 for `n` instructions, IPL region unmapped.
fn run(code: &[u8], n: usize) -> Apu {
    let mut apu = Apu::new();
    apu.control = 0;
    apu.ram[0x200..0x200 + code.len()].copy_from_slice(code);
    apu.pc = 0x200;
    steps(&mut apu, n);
    apu
}

fn steps(apu: &mut Apu, n: usize) -> u32 {
    let mut total = 0;
    for _ in 0..n {
        let c = apu.exec();
        apu.tick(c);
        total += c;
    }
    total
}

#[test]
fn adc_sbc_cmp_flags() {
    // mov a,#$7f ; clrc ; adc a,#$01
    let apu = run(&[0xe8, 0x7f, 0x60, 0x88, 0x01], 3);
    assert_eq!(apu.a, 0x80);
    assert_eq!(apu.psw & (N | V | H | Z | C), N | V | H);
    // mov a,#$10 ; setc ; sbc a,#$01 -> $0f, no borrow (C set), half borrow (H clear)
    let apu = run(&[0xe8, 0x10, 0x80, 0xa8, 0x01], 3);
    assert_eq!(apu.a, 0x0f);
    assert_eq!(apu.psw & (C | H | Z), C);
    // mov a,#5 ; cmp a,#5
    let apu = run(&[0xe8, 0x05, 0x68, 0x05], 2);
    assert_eq!(apu.psw & (C | Z | N), C | Z);
}

#[test]
fn memory_alu_and_moves() {
    // mov $10,#$f0 ; mov a,#$0f ; or $10,#$01 ; mov x,#$10 ; mov a,(x) ; mov $20,$10
    let apu = run(
        &[
            0x8f, 0xf0, 0x10, 0xe8, 0x0f, 0x18, 0x01, 0x10, 0xcd, 0x10, 0xe6, 0xfa, 0x10, 0x20,
        ],
        6,
    );
    assert_eq!(apu.ram[0x10], 0xf1);
    assert_eq!(apu.a, 0xf1);
    assert_eq!(apu.ram[0x20], 0xf1);
    // mov y,#2 ; mov a,#$aa ; mov !$0300+y,a ; mov a,#0 ; mov a,!$0302
    let apu = run(
        &[
            0x8d, 0x02, 0xe8, 0xaa, 0xd6, 0x00, 0x03, 0xe8, 0x00, 0xe5, 0x02, 0x03,
        ],
        5,
    );
    assert_eq!(apu.a, 0xaa);
    // indirect: $30/$31 = $0400 ; mov y,#1 ; mov a,#$5a ; mov [$30]+y,a
    let mut apu = run(
        &[
            0x8f, 0x00, 0x30, 0x8f, 0x04, 0x31, 0x8d, 0x01, 0xe8, 0x5a, 0xd7, 0x30,
        ],
        6,
    );
    assert_eq!(apu.ram[0x401], 0x5a);
    // mov x,#0 ; mov a,[$30+x] reads $0400
    apu.ram[0x400] = 0x77;
    apu.ram[0x210..0x214].copy_from_slice(&[0xcd, 0x00, 0xe7, 0x30]);
    apu.pc = 0x210;
    steps(&mut apu, 2);
    assert_eq!(apu.a, 0x77);
}

#[test]
fn direct_page_follows_p() {
    // setp ; mov $10,#$42 ; clrp ; mov a,$10
    let apu = run(&[0x40, 0x8f, 0x42, 0x10, 0x20, 0xe4, 0x10], 4);
    assert_eq!(apu.ram[0x110], 0x42);
    assert_eq!(apu.ram[0x010], 0);
    assert_eq!(apu.a, 0);
    // 16-bit dp word at $ff wraps inside the page: movw ya,$ff reads $ff and $00
    let mut apu = Apu::new();
    apu.control = 0;
    apu.ram[0xff] = 0x34;
    apu.ram[0x00] = 0x12;
    apu.ram[0x200..0x202].copy_from_slice(&[0xba, 0xff]);
    apu.pc = 0x200;
    steps(&mut apu, 1);
    // $FF is a timer counter register (reads 0), so only the wrap to $00 shows.
    assert_eq!((apu.y, apu.a), (0x12, 0x00));
}

#[test]
fn word_ops() {
    // movw: $10 = $1234 ; mov a,#$ff ; mov y,#$00 ; addw ya,$10 -> $1333
    let apu = run(
        &[
            0x8f, 0x34, 0x10, 0x8f, 0x12, 0x11, 0xe8, 0xff, 0x8d, 0x00, 0x7a, 0x10,
        ],
        5,
    );
    assert_eq!(apu.ya(), 0x1333);
    assert_eq!(apu.psw & (C | Z), 0);
    // subw ya,$10 from $1000 -> $fdcc, borrow
    let apu = run(
        &[
            0x8f, 0x34, 0x10, 0x8f, 0x12, 0x11, 0xe8, 0x00, 0x8d, 0x10, 0x9a, 0x10,
        ],
        5,
    );
    assert_eq!(apu.ya(), 0x1000u16.wrapping_sub(0x1234));
    assert_eq!(apu.psw & (C | N), N);
    // incw $10 at $00ff -> $0100 ; cmpw ya,$10 with ya=$0100 -> Z, C
    let apu = run(
        &[
            0x8f, 0xff, 0x10, 0x3a, 0x10, 0xe8, 0x00, 0x8d, 0x01, 0x5a, 0x10,
        ],
        5,
    );
    assert_eq!((apu.ram[0x10], apu.ram[0x11]), (0x00, 0x01));
    assert_eq!(apu.psw & (C | Z), C | Z);
}

#[test]
fn mul_div() {
    // mov a,#200 ; mov y,#3 ; mul ya
    let apu = run(&[0xe8, 200, 0x8d, 3, 0xcf], 3);
    assert_eq!(apu.ya(), 600);
    // ya = 1000 ; mov x,#7 ; div ya,x -> 142 r 6
    let apu = run(&[0xe8, 0xe8, 0x8d, 0x03, 0xcd, 7, 0x9e], 4);
    assert_eq!((apu.a, apu.y), (142, 6));
    assert_eq!(apu.psw & V, 0);
    // overflow: ya = $1000, x = 1 -> quotient $1000 doesn't fit: V set
    let apu = run(&[0xe8, 0x00, 0x8d, 0x10, 0xcd, 1, 0x9e], 4);
    assert_ne!(apu.psw & V, 0);
    // Y >= 2X branch from the documented algorithm.
    assert_eq!(apu.a, (255 - (0x1000 - 512) / 255) as u8);
    assert_eq!(apu.y, (1 + (0x1000 - 512) % 255) as u8);
}

#[test]
fn decimal_adjust() {
    // mov a,#$19 ; clrc ; adc a,#$28 ; daa -> $47
    let apu = run(&[0xe8, 0x19, 0x60, 0x88, 0x28, 0xdf], 4);
    assert_eq!(apu.a, 0x47);
    // mov a,#$47 ; setc ; sbc a,#$28 ; das -> $19
    let apu = run(&[0xe8, 0x47, 0x80, 0xa8, 0x28, 0xbe], 4);
    assert_eq!(apu.a, 0x19);
    // 99 + 1 -> 00 carry
    let apu = run(&[0xe8, 0x99, 0x60, 0x88, 0x01, 0xdf], 4);
    assert_eq!(apu.a, 0x00);
    assert_ne!(apu.psw & C, 0);
}

#[test]
fn shifts_and_xcn() {
    // mov a,#$81 ; asl a ; rol a ; xcn a
    let apu = run(&[0xe8, 0x81, 0x1c, 0x3c, 0x9f], 4);
    assert_eq!(apu.a, 0x05u8.rotate_left(4));
    // mov $10,#$01 ; setc ; ror $10 -> $80, C=1
    let apu = run(&[0x8f, 0x01, 0x10, 0x80, 0x6b, 0x10], 3);
    assert_eq!(apu.ram[0x10], 0x80);
    assert_ne!(apu.psw & C, 0);
}

#[test]
fn bit_ops() {
    // set1 $10.3 ; clr1 $10.3 ; set1 $10.7 ; bbs $10.7,+2 ; mov a,#1 ; mov a,#2
    let apu = run(
        &[
            0x62, 0x10, 0x72, 0x10, 0xe2, 0x10, 0xe3, 0x10, 0x02, 0xe8, 0x01, 0xe8, 0x02,
        ],
        5,
    );
    assert_eq!(apu.ram[0x10], 0x80);
    assert_eq!(apu.a, 2);
    // mov1 c,$0010.7 ; not1 $0010.0 ; mov1 $0011.2,c ; and1 c,/$0010.0
    let mem_bit = |addr: u16, bit: u16| (addr | bit << 13).to_le_bytes();
    let [a, b] = mem_bit(0x10, 7);
    let [c, d] = mem_bit(0x10, 0);
    let [e, f] = mem_bit(0x11, 2);
    let mut apu = Apu::new();
    apu.control = 0;
    apu.ram[0x10] = 0x80;
    apu.ram[0x200..0x20c].copy_from_slice(&[0xaa, a, b, 0xea, c, d, 0xca, e, f, 0x6a, c, d]);
    apu.pc = 0x200;
    steps(&mut apu, 3);
    assert_ne!(apu.psw & C, 0);
    assert_eq!(apu.ram[0x10], 0x81);
    assert_eq!(apu.ram[0x11], 0x04);
    steps(&mut apu, 1);
    assert_eq!(apu.psw & C, 0);
    // tset1 / tclr1 with A = $0f against $f0
    let mut apu = run(&[0xe8, 0x0f, 0x0e, 0x00, 0x03], 2);
    assert_eq!(apu.ram[0x300], 0x0f);
    apu.ram[0x210..0x213].copy_from_slice(&[0x4e, 0x00, 0x03]);
    apu.pc = 0x210;
    steps(&mut apu, 1);
    assert_eq!(apu.ram[0x300], 0x00);
    assert_ne!(apu.psw & Z, 0);
}

#[test]
fn calls_stack_and_branch_cycles() {
    // call $0300 ; (at $0300: mov a,#9 ; ret)
    let mut apu = Apu::new();
    apu.control = 0;
    apu.ram[0x200..0x203].copy_from_slice(&[0x3f, 0x00, 0x03]);
    apu.ram[0x300..0x303].copy_from_slice(&[0xe8, 0x09, 0x6f]);
    apu.pc = 0x200;
    assert_eq!(steps(&mut apu, 3), 8 + 2 + 5);
    assert_eq!((apu.pc, apu.a, apu.sp), (0x203, 9, 0xef));
    // tcall 1 jumps through $ffdc
    let mut apu = Apu::new();
    apu.control = 0;
    apu.ram[0xffdc] = 0x00;
    apu.ram[0xffdd] = 0x04;
    apu.ram[0x200] = 0x11;
    apu.pc = 0x200;
    steps(&mut apu, 1);
    assert_eq!(apu.pc, 0x400);
    // push a / pop x ; bne taken costs 4, not taken 2; dbnz y loops
    let apu = run(&[0xe8, 0x33, 0x2d, 0xce], 3);
    assert_eq!(apu.x, 0x33);
    let mut apu = run(&[0xe8, 0x01, 0xd0, 0x00], 1);
    assert_eq!(steps(&mut apu, 1), 4);
    let mut apu = run(&[0xe8, 0x00, 0xd0, 0x00], 1);
    assert_eq!(steps(&mut apu, 1), 2);
    // mov y,#3 ; dbnz y,-2 (loops on itself) ; mov a,#7
    let apu = run(&[0x8d, 0x03, 0xfe, 0xfe, 0xe8, 0x07], 5);
    assert_eq!((apu.y, apu.a), (0, 7));
}

#[test]
fn timers_tick_at_documented_rates() {
    // Timer 0 at target 32 ticks at 8 kHz / 32 = 250 Hz; timer 2 at target 4 at 16 kHz.
    let mut apu = Apu::new();
    apu.control = 0;
    apu.write(0xfa, 32);
    apu.write(0xfc, 4);
    apu.write(0xf1, 0x05);
    apu.tick(128 * 32 * 3 + 100);
    assert_eq!(apu.read(0xfd), 3);
    assert_eq!(apu.read(0xfd), 0, "counters clear on read");
    // 12388 cycles / (16*4) = 193 ticks, 4-bit counter
    assert_eq!(apu.read(0xff), (12388 / 64 % 16) as u8);
    // Target 0 means 256.
    let mut apu = Apu::new();
    apu.write(0xfb, 0);
    apu.write(0xf1, 0x02);
    apu.tick(128 * 255);
    assert_eq!(apu.read(0xfe), 0);
    apu.tick(128);
    assert_eq!(apu.read(0xfe), 1);
}

#[test]
fn control_clears_ports() {
    let mut apu = Apu::new();
    for p in 0..4 {
        apu.cpu_write_port(p, 0x11 * (p + 1));
    }
    apu.write(0xf1, 0x10);
    assert_eq!(
        [
            apu.read(0xf4),
            apu.read(0xf5),
            apu.read(0xf6),
            apu.read(0xf7)
        ],
        [0, 0, 0x33, 0x44]
    );
    apu.write(0xf5, 0x99);
    assert_eq!(apu.cpu_read_port(1), 0x99);
}

#[test]
fn one_second_is_32000_samples() {
    let mut apu = Apu::new();
    apu.run_until(21_477_272);
    let mut out = Vec::new();
    apu.take_samples(&mut out);
    assert!((out.len() / 2).abs_diff(32000) <= 1, "{}", out.len() / 2);
}

/// The 65816 side of the upload, the way the cartridge runtime does it: wait for $BBAA,
/// kick $CC, send bytes with an incrementing index, then kick with port 1 = 0 to jump.
struct Host {
    apu: Apu,
    clock: u64,
}

impl Host {
    fn wait(&mut self, port: u8, want: u8) {
        for _ in 0..100_000 {
            self.clock += 200;
            self.apu.run_until(self.clock);
            if self.apu.cpu_read_port(port) == want {
                return;
            }
        }
        panic!("port {port} never became {want:#04x}");
    }
    fn write(&mut self, port: u8, v: u8) {
        self.clock += 12;
        self.apu.run_until(self.clock);
        self.apu.cpu_write_port(port, v);
    }
    fn upload(&mut self, addr: u16, data: &[u8], entry: u16) {
        self.wait(0, 0xaa);
        self.wait(1, 0xbb);
        self.write(1, 1);
        self.write(2, addr as u8);
        self.write(3, (addr >> 8) as u8);
        self.write(0, 0xcc);
        self.wait(0, 0xcc);
        let mut index = 0u8;
        for &b in data {
            self.write(1, b);
            self.write(0, index);
            self.wait(0, index);
            index = index.wrapping_add(1);
        }
        let kick = index.wrapping_add(1);
        self.write(1, 0);
        self.write(2, entry as u8);
        self.write(3, (entry >> 8) as u8);
        self.write(0, kick);
        self.wait(0, kick);
    }
}

#[test]
fn upload_through_ports_then_drive_the_dsp() {
    // A mailbox driver: unmute, then for each new value on port 0 write DSP[port 2] = port 3,
    // report voice 0's ENVX on port 1 and echo port 0.
    let driver: &[u8] = &[
        0x8f, 0x6c, 0xf2, // mov $f2,#$6c
        0x8f, 0x00, 0xf3, // mov $f3,#$00   FLG: unmute, echo writes on
        0x8f, 0x00, 0x10, // mov $10,#0     last command
        0xe4, 0xf4, //       loop: mov a,$f4
        0x64, 0x10, //       cmp a,$10
        0xf0, 0xfa, //       beq loop
        0xc4, 0x10, //       mov $10,a
        0xfa, 0xf6, 0xf2, // mov $f2,$f6
        0xfa, 0xf7, 0xf3, // mov $f3,$f7
        0x8f, 0x08, 0xf2, // mov $f2,#$08   ENVX(0)
        0xfa, 0xf3, 0xf5, // mov $f5,$f3
        0xc4, 0xf4, //       mov $f4,a
        0x2f, 0xe8, //       bra loop
    ];
    // Spread the program over two blocks' worth of bytes so the index wraps past 255.
    let mut image = vec![0u8; 0x300];
    image[..driver.len()].copy_from_slice(driver);
    let mut host = Host {
        apu: Apu::new(),
        clock: 0,
    };
    host.upload(0x0200, &image, 0x0200);
    assert_eq!(&host.apu.ram[0x200..0x200 + driver.len()], driver);

    let mut command = 0x10u8;
    let mut dsp = |host: &mut Host, reg: u8, v: u8| {
        command = command.wrapping_add(1);
        host.write(2, reg);
        host.write(3, v);
        host.write(0, command);
        host.wait(0, command);
    };
    dsp(&mut host, 0x0c, 0x55);
    assert_eq!(host.apu.dsp.read(0x0c), 0x55);
    assert_eq!(host.apu.dsp.read(0x6c), 0x00);
    // Voice 0: direct gain $7f, key on; ENVX comes back through port 1.
    dsp(&mut host, 0x07, 0x7f);
    dsp(&mut host, 0x4c, 0x01);
    for _ in 0..4 {
        dsp(&mut host, 0x0d, 0);
    }
    assert_eq!(host.apu.cpu_read_port(1), host.apu.dsp.read(0x08));
    assert_ne!(
        host.apu.cpu_read_port(1),
        0,
        "voice 0's envelope should be up"
    );
}

#[test]
fn reupload_by_jumping_back_to_the_boot_region() {
    // Uploaded code: mov $f4,#$5a ; jmp !$ffc0 (driver hands control back to the boot routine)
    let code = [0x8f, 0x5a, 0xf4, 0x5f, 0xc0, 0xff];
    let mut host = Host {
        apu: Apu::new(),
        clock: 0,
    };
    host.upload(0x0400, &code, 0x0400);
    host.upload(0x0500, &[1, 2, 3], 0x0500);
    assert_eq!(&host.apu.ram[0x500..0x503], &[1, 2, 3]);
}
