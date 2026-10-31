//! The PPU bridge: $2100-$213F with their hardware semantics (write-twice latches, the VRAM, OAM and
//! CGRAM ports, the Mode 7 multiplier, the counter latch) decoded into ppu-core's raw register row
//! and memory, rendered one line at a time through `ppu_core::LineRenderer`.
//!
//! Differences from hardware, kept small on purpose:
//! - A line renders whole at one point (see `Snes`), so a write in the middle of a line's pixels
//!   takes effect from the next line rather than the next pixel.
//! - CGRAM and OAM accept writes at any time (hardware routes active-display writes elsewhere).
//! - Interlace, pseudo-hires and overscan (239 lines) are stored but not rendered: the picture is
//!   the 224-line one ppu-core draws.
use ppu_core::{LineRenderer, Memory, Obj, RegBg, RegM7, RegRow, WIDTH};

pub struct Ppu {
    pub mem: Memory,
    /// Raw OAM: 512-byte low table + 32-byte high table, decoded into `mem.oam` on every write.
    oam: [u8; 544],
    /// The current register state; scrolls are applied per line from the fields below.
    row: RegRow,
    renderer: LineRenderer,
    hofs: [u16; 4],
    vofs: [u16; 4],
    m7hofs: u16,
    m7vofs: u16,
    bgofs_latch: u8,
    m7_latch: u8,
    /// OAMADD as written (9-bit word address) and the internal byte address (10 bits).
    oamadd: u16,
    oam_addr: u16,
    oam_latch: u8,
    vmain: u8,
    vaddr: u16,
    vram_latch: u16,
    cg_addr: u8,
    cg_high: bool,
    cg_latch: u8,
    mpy: i32,
    /// $2137 latched counters and their read flip-flops.
    ophct: u16,
    opvct: u16,
    oph_hi: bool,
    opv_hi: bool,
    counter_latched: bool,
    pub ppu1_mdr: u8,
    pub ppu2_mdr: u8,
    pub range_over: bool,
    pub time_over: bool,
    mosaic_size: u8,
    mosaic_top: u16,
    /// Field bit for STAT78 ($213F bit 7), toggled every frame.
    pub field: bool,
}

fn bg_default() -> RegBg {
    RegBg {
        scroll_x: 0,
        scroll_y: 0,
        visible: true,
        mode: 0,
        layer: 0,
        tile_size: 8,
        map_base: 0,
        screen_size: 0,
        char_base: 0,
        offset_map_base: 0,
        offset_screen_size: 0,
        offset_scroll_x: 0,
        offset_scroll_y: 0,
        offset_tile_size: 8,
        bpp: 0,
        direct_color: false,
        mosaic: 1,
        mosaic_top: 0,
    }
}

impl Default for Ppu {
    fn default() -> Self {
        let mut row = RegRow {
            mode: 0,
            brightness: 0,
            bg: std::array::from_fn(|_| bg_default()),
            m7: RegM7 { a: 0, b: 0, c: 0, d: 0, cx: 0, cy: 0, repeat: 0, flip_x: false, flip_y: false },
            bg3_priority: false,
            tm: 0,
            ts: 0,
            wh0: 0,
            wh1: 0,
            wh2: 0,
            wh3: 0,
            w12sel: 0,
            w34sel: 0,
            wobjsel: 0,
            wbglog: 0,
            wobjlog: 0,
            tmw: 0,
            tsw: 0,
            cgwsel: 0,
            cgadsub: 0,
            coldata: 0,
            mosaic_size: 0,
            mosaic_enable: [false; 4],
            setini: 0,
            force_blank: true,
            cgram: Vec::new(),
        };
        row.derive_layers();
        let mut mem = Memory::new();
        for o in mem.oam.iter_mut() {
            o.on = true;
        }
        Ppu {
            mem,
            oam: [0; 544],
            row,
            renderer: LineRenderer::new(),
            hofs: [0; 4],
            vofs: [0; 4],
            m7hofs: 0,
            m7vofs: 0,
            bgofs_latch: 0,
            m7_latch: 0,
            oamadd: 0,
            oam_addr: 0,
            oam_latch: 0,
            vmain: 0,
            vaddr: 0,
            vram_latch: 0,
            cg_addr: 0,
            cg_high: false,
            cg_latch: 0,
            mpy: 0,
            ophct: 0,
            opvct: 0,
            oph_hi: false,
            opv_hi: false,
            counter_latched: false,
            ppu1_mdr: 0,
            ppu2_mdr: 0,
            range_over: false,
            time_over: false,
            mosaic_size: 0,
            mosaic_top: 0,
            field: false,
        }
    }
}

/// 13-bit signed (Mode 7 centre and scroll) kept in an i16.
fn i13(v: u16) -> i16 {
    (((v & 0x1fff) ^ 0x1000) as i16) - 0x1000
}

impl Ppu {
    #[cfg(test)]
    pub fn row_coldata(&self) -> u16 {
        self.row.coldata
    }
    #[cfg(test)]
    pub fn row_wh0(&self) -> u8 {
        self.row.wh0
    }

    pub fn force_blank(&self) -> bool {
        self.row.force_blank
    }

    /// The VRAM word address after VMAIN's remapping (bits 2-3).
    fn vram_remap(&self) -> usize {
        let a = self.vaddr;
        let r = match (self.vmain >> 2) & 3 {
            0 => a,
            1 => (a & 0xff00) | ((a & 0x001f) << 3) | ((a >> 5) & 7),
            2 => (a & 0xfe00) | ((a & 0x003f) << 3) | ((a >> 6) & 7),
            _ => (a & 0xfc00) | ((a & 0x007f) << 3) | ((a >> 7) & 7),
        };
        (r & 0x7fff) as usize
    }

    fn vram_step(&mut self) {
        let step = [1, 32, 128, 128][(self.vmain & 3) as usize];
        self.vaddr = self.vaddr.wrapping_add(step);
    }

    fn decode_obj(&mut self, i: usize) {
        let b = &self.oam[i * 4..i * 4 + 4];
        let hi = (self.oam[0x200 + i / 4] >> ((i & 3) * 2)) & 3;
        let x9 = b[0] as u16 | ((hi as u16 & 1) << 8);
        self.mem.oam[i] = Obj {
            x: if x9 & 0x100 != 0 { x9 as i16 - 0x200 } else { x9 as i16 },
            y: b[1],
            tile: b[2] as u16 | ((b[3] as u16 & 1) << 8),
            pal: (b[3] >> 1) & 7,
            prio: (b[3] >> 4) & 3,
            large: hi & 2 != 0,
            flip_x: b[3] & 0x40 != 0,
            flip_y: b[3] & 0x80 != 0,
            on: true,
        };
    }

    fn oam_store(&mut self, byte: usize, v: u8) {
        self.oam[byte] = v;
        if byte < 0x200 {
            self.decode_obj(byte / 4);
        } else {
            for k in 0..4 {
                self.decode_obj((byte - 0x200) * 4 + k);
            }
        }
    }

    /// VBlank start: the internal OAM address reloads from OAMADD (unless force-blanked).
    pub fn vblank(&mut self) {
        if !self.row.force_blank {
            self.oam_addr = (self.oamadd & 0x1ff) << 1;
        }
    }

    /// Frame start (end of VBlank): the sprite overflow flags clear.
    pub fn frame_start(&mut self) {
        if !self.row.force_blank {
            self.range_over = false;
            self.time_over = false;
        }
        self.field = !self.field;
    }

    /// $21xx write (`reg` = low byte). `active` is true while the beam draws visible lines, when
    /// VRAM refuses writes unless the screen is force-blanked.
    pub fn write(&mut self, reg: u8, v: u8, active: bool) {
        let row = &mut self.row;
        match reg {
            0x00 => {
                // Leaving force blank mid-VBlank does not reload OAMADD; entering it keeps the address.
                row.force_blank = v & 0x80 != 0;
                row.brightness = v & 0x0f;
            }
            0x01 => {
                self.mem.obsel.char_base = ((v as u16) & 3) << 13;
                self.mem.obsel.name_select = (v >> 3) & 3;
                self.mem.obsel.size_sel = v >> 5;
            }
            0x02 => {
                self.oamadd = (self.oamadd & 0x100) | v as u16;
                self.oam_addr = (self.oamadd & 0x1ff) << 1;
                self.mem.oam_addr = self.oamadd & 0x1ff;
            }
            0x03 => {
                self.oamadd = (self.oamadd & 0xff) | ((v as u16 & 1) << 8);
                self.oam_addr = (self.oamadd & 0x1ff) << 1;
                self.mem.priority_rotate = v & 0x80 != 0;
                self.mem.oam_addr = self.oamadd & 0x1ff;
            }
            0x04 => {
                let a = self.oam_addr as usize;
                if a >= 0x200 {
                    self.oam_store(0x200 + (a & 0x1f), v);
                } else if a & 1 == 0 {
                    self.oam_latch = v;
                } else {
                    let lo = self.oam_latch;
                    self.oam[a - 1] = lo;
                    self.oam_store(a, v);
                }
                self.oam_addr = (self.oam_addr + 1) & 0x3ff;
            }
            0x05 => {
                row.mode = v & 7;
                row.bg3_priority = v & 8 != 0;
                for i in 0..4 {
                    row.bg[i].tile_size = if v & (0x10 << i) != 0 { 16 } else { 8 };
                }
            }
            0x06 => {
                row.mosaic_size = v >> 4;
                for i in 0..4 {
                    row.mosaic_enable[i] = v & (1 << i) != 0;
                }
            }
            0x07..=0x0a => {
                let b = &mut row.bg[(reg - 7) as usize];
                b.map_base = ((v as u16) & 0x7c) << 8;
                b.screen_size = v & 3;
            }
            0x0b => {
                row.bg[0].char_base = ((v as u16) & 7) << 12;
                row.bg[1].char_base = ((v as u16 >> 4) & 7) << 12;
            }
            0x0c => {
                row.bg[2].char_base = ((v as u16) & 7) << 12;
                row.bg[3].char_base = ((v as u16 >> 4) & 7) << 12;
            }
            0x0d..=0x14 => {
                let i = ((reg - 0x0d) / 2) as usize;
                if reg & 1 == 1 {
                    // BGnHOFS: (cur << 8) | (prev & ~7) | (old HOFS bits 8-10 & 7)
                    self.hofs[i] = ((v as u16) << 8 | (self.bgofs_latch as u16 & !7) | ((self.hofs[i] >> 8) & 7)) & 0x3ff;
                } else {
                    self.vofs[i] = ((v as u16) << 8 | self.bgofs_latch as u16) & 0x3ff;
                }
                self.bgofs_latch = v;
                if reg == 0x0d {
                    self.m7hofs = (v as u16) << 8 | self.m7_latch as u16;
                    self.m7_latch = v;
                } else if reg == 0x0e {
                    self.m7vofs = (v as u16) << 8 | self.m7_latch as u16;
                    self.m7_latch = v;
                }
            }
            0x15 => self.vmain = v,
            0x16 => {
                self.vaddr = (self.vaddr & 0xff00) | v as u16;
                self.vram_latch = self.mem.vram[self.vram_remap()];
            }
            0x17 => {
                self.vaddr = (self.vaddr & 0x00ff) | (v as u16) << 8;
                self.vram_latch = self.mem.vram[self.vram_remap()];
            }
            0x18 | 0x19 => {
                let high = reg == 0x19;
                if !active || row.force_blank {
                    let a = self.vram_remap();
                    let w = &mut self.mem.vram[a];
                    *w = if high { (*w & 0x00ff) | (v as u16) << 8 } else { (*w & 0xff00) | v as u16 };
                }
                if high == (self.vmain & 0x80 != 0) {
                    self.vram_step();
                }
            }
            0x1a => {
                row.m7.flip_x = v & 1 != 0;
                row.m7.flip_y = v & 2 != 0;
                row.m7.repeat = v >> 6;
            }
            0x1b..=0x20 => {
                let w = ((v as u16) << 8 | self.m7_latch as u16) as i16;
                self.m7_latch = v;
                let m7 = &mut row.m7;
                match reg {
                    0x1b => m7.a = w,
                    0x1c => m7.b = w,
                    0x1d => m7.c = w,
                    0x1e => m7.d = w,
                    0x1f => m7.cx = i13(w as u16),
                    _ => m7.cy = i13(w as u16),
                }
                // The multiplier: M7A (16-bit signed) x the last byte written to M7B (8-bit signed).
                self.mpy = row.m7.a as i32 * ((row.m7.b >> 8) as i8) as i32;
            }
            0x21 => {
                self.cg_addr = v;
                self.cg_high = false;
            }
            0x22 => {
                if !self.cg_high {
                    self.cg_latch = v;
                } else {
                    self.mem.cgram[self.cg_addr as usize] = ((v as u16 & 0x7f) << 8) | self.cg_latch as u16;
                    self.cg_addr = self.cg_addr.wrapping_add(1);
                }
                self.cg_high = !self.cg_high;
            }
            0x23 => row.w12sel = v,
            0x24 => row.w34sel = v,
            0x25 => row.wobjsel = v,
            0x26 => row.wh0 = v,
            0x27 => row.wh1 = v,
            0x28 => row.wh2 = v,
            0x29 => row.wh3 = v,
            0x2a => row.wbglog = v,
            0x2b => row.wobjlog = v & 0x0f,
            0x2c => row.tm = v & 0x1f,
            0x2d => row.ts = v & 0x1f,
            0x2e => row.tmw = v & 0x1f,
            0x2f => row.tsw = v & 0x1f,
            0x30 => row.cgwsel = v,
            0x31 => row.cgadsub = v,
            0x32 => {
                let c = (v & 0x1f) as u16;
                if v & 0x20 != 0 {
                    row.coldata = (row.coldata & !0x001f) | c;
                }
                if v & 0x40 != 0 {
                    row.coldata = (row.coldata & !0x03e0) | c << 5;
                }
                if v & 0x80 != 0 {
                    row.coldata = (row.coldata & !0x7c00) | c << 10;
                }
            }
            0x33 => row.setini = v,
            _ => {}
        }
    }

    /// $2137 (or WRIO bit 7 falling): latch the H/V counters.
    pub fn latch_counters(&mut self, hdot: u16, line: u16) {
        self.ophct = hdot;
        self.opvct = line;
        self.counter_latched = true;
    }

    /// $21xx read (`reg` 0x00-0x3f). Write-only registers return PPU1's open bus.
    pub fn read(&mut self, reg: u8, hdot: u16, line: u16, wrio_latch: bool) -> u8 {
        match reg {
            0x34..=0x36 => {
                self.ppu1_mdr = (self.mpy >> ((reg - 0x34) * 8)) as u8;
                self.ppu1_mdr
            }
            0x37 => {
                if wrio_latch {
                    self.latch_counters(hdot, line);
                }
                self.ppu1_mdr // SLHV reads open bus
            }
            0x38 => {
                let a = self.oam_addr as usize;
                let v = if a >= 0x200 { self.oam[0x200 + (a & 0x1f)] } else { self.oam[a] };
                self.oam_addr = (self.oam_addr + 1) & 0x3ff;
                self.ppu1_mdr = v;
                v
            }
            0x39 | 0x3a => {
                let high = reg == 0x3a;
                let v = if high { (self.vram_latch >> 8) as u8 } else { self.vram_latch as u8 };
                if high == (self.vmain & 0x80 != 0) {
                    self.vram_latch = self.mem.vram[self.vram_remap()];
                    self.vram_step();
                }
                self.ppu1_mdr = v;
                v
            }
            0x3b => {
                let c = self.mem.cgram[self.cg_addr as usize];
                let v = if self.cg_high {
                    self.cg_addr = self.cg_addr.wrapping_add(1);
                    ((c >> 8) as u8 & 0x7f) | (self.ppu2_mdr & 0x80)
                } else {
                    c as u8
                };
                self.cg_high = !self.cg_high;
                self.ppu2_mdr = v;
                v
            }
            0x3c => {
                let v = if self.oph_hi {
                    ((self.ophct >> 8) as u8 & 1) | (self.ppu2_mdr & 0xfe)
                } else {
                    self.ophct as u8
                };
                self.oph_hi = !self.oph_hi;
                self.ppu2_mdr = v;
                v
            }
            0x3d => {
                let v = if self.opv_hi {
                    ((self.opvct >> 8) as u8 & 1) | (self.ppu2_mdr & 0xfe)
                } else {
                    self.opvct as u8
                };
                self.opv_hi = !self.opv_hi;
                self.ppu2_mdr = v;
                v
            }
            0x3e => {
                let v = (self.time_over as u8) << 7 | (self.range_over as u8) << 6 | (self.ppu1_mdr & 0x10) | 1;
                self.ppu1_mdr = v;
                v
            }
            0x3f => {
                let v = (self.field as u8) << 7
                    | (self.counter_latched as u8) << 6
                    | (self.ppu2_mdr & 0x20)
                    | 3; // NTSC, PPU2 version 3
                self.counter_latched = false;
                self.oph_hi = false;
                self.opv_hi = false;
                self.ppu2_mdr = v;
                v
            }
            _ => self.ppu1_mdr,
        }
    }

    /// Render screen row `y` (hardware line y + 1) into `out` (256 RGBA pixels) from the registers
    /// and memory as they are now.
    pub fn render_line(&mut self, y: usize, out: &mut [u8]) {
        let row = &mut self.row;
        for i in 0..4 {
            row.bg[i].scroll_x = self.hofs[i] as i16;
            row.bg[i].scroll_y = self.vofs[i] as i16;
        }
        if row.mode == 7 {
            row.bg[0].scroll_x = i13(self.m7hofs);
            row.bg[0].scroll_y = i13(self.m7vofs);
        }
        if y == 0 || row.mosaic_size != self.mosaic_size {
            self.mosaic_top = y as u16;
        }
        self.mosaic_size = row.mosaic_size;
        row.derive_layers();
        for b in &mut row.bg {
            b.mosaic_top = self.mosaic_top;
        }
        let bin = self.renderer.render(row, &self.mem, y, &mut out[..WIDTH * 4]);
        self.range_over |= bin.range_over;
        self.time_over |= bin.time_over;
    }
}
