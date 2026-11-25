//! ppu-link.exe: the ppu:// handler `ppu setup` registers on Windows. A windows-subsystem program,
//! so a browser click opens no console; it starts the console ppu.exe beside it with no window
//! (ppu.exe stays a console program because a shell does not wait for a windows-subsystem one).
//! `ppu-link [--site=<url>] <ppu:// link>`: the site setup baked in reaches ppu as PPU_SITE.
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let Ok(me) = std::env::current_exe() else {
        std::process::exit(1)
    };
    let mut cmd = std::process::Command::new(me.with_file_name("ppu.exe"));
    if let Some(site) = args
        .first()
        .and_then(|a| a.to_str())
        .and_then(|a| a.strip_prefix("--site="))
    {
        cmd.env("PPU_SITE", site);
        args.remove(0);
    }
    // no window to show an error in: ppu.exe reports to the Connect panel's log itself
    let spawned = cmd.args(args).creation_flags(CREATE_NO_WINDOW).spawn();
    std::process::exit(if spawned.is_ok() { 0 } else { 1 });
}

#[cfg(not(windows))]
fn main() {
    eprintln!("ppu-link is the Windows ppu:// handler; run ppu instead");
    std::process::exit(2);
}
