//! ppu-cart as a libretro core (`cargo build --release -p ppu-cart --example ppu_cart_libretro` ->
//! `libppu_cart_libretro.so`), so test rigs that already drive emulator cores through the libretro API can play
//! ROMs in it unchanged: XRGB8888 video, joypads 1 and 2, the work RAM as RETRO_MEMORY_SYSTEM_RAM.
//! No audio, no save states.
//!
//! Profiling: `st2_on`, `st2_pcticks` (master clocks per 24-bit PC) and `st2_clearpc` follow the
//! interface of the profiling-core tools: while `st2_on` is non-zero every frame's
//! `Snes::take_pc_ticks` is added into `st2_pcticks`.
#![allow(non_upper_case_globals, static_mut_refs, clippy::missing_safety_doc)]
use ppu_cart::Snes;
use std::ffi::{c_char, c_uint, c_void};

type EnvFn = extern "C" fn(c_uint, *mut c_void) -> bool;
type VideoFn = extern "C" fn(*const c_void, c_uint, c_uint, usize);
type PollFn = extern "C" fn();
type InputFn = extern "C" fn(c_uint, c_uint, c_uint, c_uint) -> i16;

static mut ENV: Option<EnvFn> = None;
static mut VIDEO: Option<VideoFn> = None;
static mut POLL: Option<PollFn> = None;
static mut INPUT: Option<InputFn> = None;
static mut SNES: Option<Box<Snes>> = None;
static mut XRGB: Vec<u32> = Vec::new();

#[no_mangle]
pub static mut st2_on: i32 = 0;
#[no_mangle]
pub static mut st2_pcticks: [u32; 1 << 24] = [0; 1 << 24];
#[no_mangle]
pub unsafe extern "C" fn st2_clearpc() {
    st2_pcticks.fill(0);
}

#[repr(C)]
pub struct SystemInfo {
    name: *const c_char,
    version: *const c_char,
    extensions: *const c_char,
    need_fullpath: bool,
    block_extract: bool,
}
#[repr(C)]
pub struct AvInfo {
    base_width: c_uint,
    base_height: c_uint,
    max_width: c_uint,
    max_height: c_uint,
    aspect: f32,
    fps: f64,
    sample_rate: f64,
}
#[repr(C)]
pub struct GameInfo {
    path: *const c_char,
    data: *const u8,
    size: usize,
    meta: *const c_char,
}

#[no_mangle]
pub extern "C" fn retro_api_version() -> c_uint {
    1
}
#[no_mangle]
pub unsafe extern "C" fn retro_get_system_info(i: *mut SystemInfo) {
    *i = SystemInfo { name: c"ppu-cart".as_ptr(), version: c"0.1".as_ptr(), extensions: c"sfc|smc".as_ptr(), need_fullpath: false, block_extract: false };
}
#[no_mangle]
pub unsafe extern "C" fn retro_get_system_av_info(i: *mut AvInfo) {
    *i = AvInfo { base_width: 256, base_height: 224, max_width: 256, max_height: 224, aspect: 4.0 / 3.0, fps: 21_477_272.0 / 357_366.0, sample_rate: 32040.0 };
}
#[no_mangle]
pub unsafe extern "C" fn retro_set_environment(f: EnvFn) {
    ENV = Some(f);
}
#[no_mangle]
pub unsafe extern "C" fn retro_set_video_refresh(f: VideoFn) {
    VIDEO = Some(f);
}
#[no_mangle]
pub extern "C" fn retro_set_audio_sample(_: *const c_void) {}
#[no_mangle]
pub extern "C" fn retro_set_audio_sample_batch(_: *const c_void) {}
#[no_mangle]
pub unsafe extern "C" fn retro_set_input_poll(f: PollFn) {
    POLL = Some(f);
}
#[no_mangle]
pub unsafe extern "C" fn retro_set_input_state(f: InputFn) {
    INPUT = Some(f);
}
#[no_mangle]
pub extern "C" fn retro_set_controller_port_device(_: c_uint, _: c_uint) {}
#[no_mangle]
pub extern "C" fn retro_init() {}
#[no_mangle]
pub unsafe extern "C" fn retro_deinit() {
    SNES = None;
}
#[no_mangle]
pub unsafe extern "C" fn retro_load_game(g: *const GameInfo) -> bool {
    if g.is_null() || (*g).data.is_null() {
        return false;
    }
    let rom = std::slice::from_raw_parts((*g).data, (*g).size);
    let mut fmt: u32 = 1; // RETRO_PIXEL_FORMAT_XRGB8888
    if let Some(env) = ENV {
        env(10, &mut fmt as *mut u32 as *mut c_void);
    }
    match Snes::new(rom) {
        Ok(s) => {
            SNES = Some(Box::new(s));
            XRGB = vec![0; 256 * 224];
            true
        }
        Err(_) => false,
    }
}
#[no_mangle]
pub extern "C" fn retro_load_game_special(_: c_uint, _: *const c_void, _: usize) -> bool {
    false
}
#[no_mangle]
pub unsafe extern "C" fn retro_unload_game() {
    SNES = None;
}
#[no_mangle]
pub extern "C" fn retro_reset() {}
#[no_mangle]
pub extern "C" fn retro_get_region() -> c_uint {
    0
}
#[no_mangle]
pub unsafe extern "C" fn retro_run() {
    let Some(s) = SNES.as_mut() else { return };
    if let Some(p) = POLL {
        p();
    }
    if let Some(input) = INPUT {
        for port in 0..2 {
            let mut b = 0u16;
            for id in 0..12 {
                if input(port, 1, 0, id) != 0 {
                    b |= 1 << (15 - id);
                }
            }
            s.set_pad(port as usize, b);
        }
    }
    s.profile = st2_on != 0;
    s.run_frame();
    if s.profile {
        for (pc, t) in s.take_pc_ticks() {
            let c = &mut st2_pcticks[(pc & 0xff_ffff) as usize];
            *c = c.saturating_add(t as u32);
        }
    }
    for (o, p) in XRGB.iter_mut().zip(s.framebuffer().chunks_exact(4)) {
        *o = (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32;
    }
    if let Some(v) = VIDEO {
        v(XRGB.as_ptr() as *const c_void, 256, 224, 1024);
    }
}
#[no_mangle]
pub extern "C" fn retro_serialize_size() -> usize {
    0
}
#[no_mangle]
pub extern "C" fn retro_serialize(_: *mut c_void, _: usize) -> bool {
    false
}
#[no_mangle]
pub extern "C" fn retro_unserialize(_: *const c_void, _: usize) -> bool {
    false
}
#[no_mangle]
pub extern "C" fn retro_cheat_reset() {}
#[no_mangle]
pub extern "C" fn retro_cheat_set(_: c_uint, _: bool, _: *const c_char) {}
#[no_mangle]
pub unsafe extern "C" fn retro_get_memory_data(id: c_uint) -> *mut c_void {
    match (id, SNES.as_mut()) {
        (2, Some(s)) => s.bus.wram.as_mut_ptr() as *mut c_void,
        (0, Some(s)) if !s.bus.sram.is_empty() => s.bus.sram.as_mut_ptr() as *mut c_void,
        _ => std::ptr::null_mut(),
    }
}
#[no_mangle]
pub unsafe extern "C" fn retro_get_memory_size(id: c_uint) -> usize {
    match (id, SNES.as_ref()) {
        (2, Some(s)) => s.bus.wram.len(),
        (0, Some(s)) => s.bus.sram.len(),
        _ => 0,
    }
}
