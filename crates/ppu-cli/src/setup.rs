//! `ppu setup`: register ppu:// so the Studio's Connect button reaches this ppu with no terminal.
//! Run again any time (the install scripts do): it writes the same registration. Linux (a
//! .desktop entry), macOS (a small .app in ~/Applications) and Windows (HKCU\Software\Classes\ppu
//! running the windows-subsystem ppu-link.exe, src/bin/ppu-link.rs).

use anyhow::Result;
use std::io::Write;

/// Register ppu:// for this ppu binary, saying on `out` what it did.
pub fn setup(out: &mut dyn Write) -> Result<()> {
    platform::register(out)
}

#[cfg(all(unix, not(target_os = "macos")))]
mod platform {
    use anyhow::{bail, ensure, Context, Result};
    use std::io::Write;
    use std::path::PathBuf;
    use std::process::Command;

    const ENTRY: &str = "ppu-link.desktop";

    /// A .desktop entry (Terminal=false, hidden from menus) for x-scheme-handler/ppu, made the
    /// default handler with xdg-mime. A PPU_SITE set now is written into the entry, so links from
    /// that site's Studio connect to it.
    pub fn register(out: &mut dyn Write) -> Result<()> {
        let exe = std::env::current_exe()?.canonicalize()?;
        let data = std::env::var_os("XDG_DATA_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .context("HOME is not set")?;
        let dir = data.join("applications");
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("cannot create {}", dir.display()))?;
        let site = std::env::var("PPU_SITE")
            .ok()
            .filter(|s| !s.trim().is_empty());
        let exec = match &site {
            Some(site) => format!(
                "env {} {} %u",
                quote(&format!("PPU_SITE={site}")),
                quote(exe.to_str().context("ppu's path is not UTF-8")?)
            ),
            None => format!(
                "{} %u",
                quote(exe.to_str().context("ppu's path is not UTF-8")?)
            ),
        };
        let entry = format!(
            "[Desktop Entry]\nType=Application\nName=ppu\nComment=Opens ppu.toys Connect links\n\
             Exec={exec}\nTerminal=false\nNoDisplay=true\nMimeType=x-scheme-handler/ppu;\n"
        );
        let path = dir.join(ENTRY);
        std::fs::write(&path, entry).with_context(|| format!("cannot write {}", path.display()))?;
        writeln!(out, "Wrote {}", path.display())?;
        // xdg-mime writes mimeapps.list there, and on a fresh machine fails (exit 0) without it
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .context("HOME is not set")?;
        std::fs::create_dir_all(&config)
            .with_context(|| format!("cannot create {}", config.display()))?;
        let status = Command::new("xdg-mime")
            .args(["default", ENTRY, "x-scheme-handler/ppu"])
            .status();
        match status {
            Ok(s) if s.success() => {}
            Ok(s) => bail!("xdg-mime default {ENTRY} x-scheme-handler/ppu failed ({s})"),
            Err(e) => {
                bail!("cannot run xdg-mime ({e}): install xdg-utils, then run ppu setup again")
            }
        }
        // best effort: some desktops read MimeType= only from this cache
        let _ = Command::new("update-desktop-database").arg(&dir).status();
        let set = Command::new("xdg-mime")
            .args(["query", "default", "x-scheme-handler/ppu"])
            .output()?;
        ensure!(
            String::from_utf8_lossy(&set.stdout).trim() == ENTRY,
            "xdg-mime did not make {ENTRY} the ppu:// handler (it answers {:?})",
            String::from_utf8_lossy(&set.stdout).trim()
        );
        writeln!(
            out,
            "ppu:// links open {}{}: the Studio's Connect button starts ppu connect.",
            exe.display(),
            site.map(|s| format!(" for {s}")).unwrap_or_default()
        )?;
        Ok(())
    }

    /// One Exec argument, quoted as the desktop entry spec reads it.
    fn quote(arg: &str) -> String {
        if !arg.contains(|c: char| c.is_whitespace() || "\"'\\`$%;&|<>(){}[]*?#~".contains(c)) {
            return arg.to_string();
        }
        let mut q = String::from("\"");
        for c in arg.chars() {
            match c {
                '"' | '`' | '$' | '\\' => {
                    // the spec's escape, then the .desktop string escape doubles the backslash
                    q.push_str("\\\\");
                    q.push(c);
                }
                '%' => q.push_str("%%"),
                c => q.push(c),
            }
        }
        q.push('"');
        q
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn exec_arguments_quote_as_the_spec_reads_them() {
            assert_eq!(super::quote("/usr/bin/ppu"), "/usr/bin/ppu");
            assert_eq!(super::quote("/home/a b/ppu"), "\"/home/a b/ppu\"");
            assert_eq!(super::quote("/x/100%$y"), "\"/x/100%%\\\\$y\"");
        }
    }
}

#[cfg(windows)]
mod platform {
    use anyhow::{bail, ensure, Context, Result};
    use std::io::Write;
    use std::process::Command;

    const KEY: &str = r"HKCU\Software\Classes\ppu";

    /// HKCU\Software\Classes\ppu, for this user only (no admin), opening links with the
    /// windows-subsystem ppu-link.exe beside this ppu.exe so a click shows no console. A PPU_SITE
    /// set now is written into the command, so links from that site's Studio connect to it.
    pub fn register(out: &mut dyn Write) -> Result<()> {
        let exe = std::env::current_exe()?;
        let link = exe.with_file_name("ppu-link.exe");
        ensure!(
            link.is_file(),
            "{} is missing: it comes in the same zip as ppu.exe; put it beside ppu.exe and run ppu setup again",
            link.display()
        );
        let site = std::env::var("PPU_SITE")
            .ok()
            .filter(|s| !s.trim().is_empty());
        let site_arg = match &site {
            Some(site) => {
                ensure!(!site.contains(['"', ' ']), "PPU_SITE {site:?} is not a site address");
                format!(" --site={site}")
            }
            None => String::new(),
        };
        let command = format!("\"{}\"{site_arg} \"%1\"", link.display());
        let command_key = format!(r"{KEY}\shell\open\command");
        reg(&[KEY, "/ve", "/d", "URL:ppu.toys Connect link"])?;
        reg(&[KEY, "/v", "URL Protocol", "/d", ""])?;
        reg(&[&command_key, "/ve", "/d", &command])?;
        writeln!(out, "Wrote {command_key}")?;
        let set = Command::new("reg")
            .args(["query", &command_key, "/ve"])
            .output()?;
        ensure!(
            String::from_utf8_lossy(&set.stdout).contains(&command),
            "the registry does not hold the ppu:// command (reg query says {:?})",
            String::from_utf8_lossy(&set.stdout).trim()
        );
        writeln!(
            out,
            "ppu:// links open {}{}: the Studio's Connect button starts ppu connect.",
            exe.display(),
            site.map(|s| format!(" for {s}")).unwrap_or_default()
        )?;
        Ok(())
    }

    /// `reg add <args> /f`, failing with reg's own words.
    fn reg(args: &[&str]) -> Result<()> {
        let done = Command::new("reg")
            .arg("add")
            .args(args)
            .arg("/f")
            .output()
            .context("cannot run reg.exe")?;
        if !done.status.success() {
            bail!(
                "reg add {} failed: {}",
                args[0],
                String::from_utf8_lossy(&done.stderr).trim()
            );
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use anyhow::{bail, Context, Result};
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    const APP: &str = "ppu-link.app";
    const LSREGISTER: &str = "/System/Library/Frameworks/CoreServices.framework/Frameworks/\
                              LaunchServices.framework/Support/lsregister";

    /// An AppleScript applet in ~/Applications (macOS 26 ignores the URL schemes of apps
    /// elsewhere) that claims ppu:// and hands each link to this ppu in the background. osacompile
    /// and codesign ship with macOS, so nothing needs Xcode, and a bundle made here carries no
    /// quarantine for Gatekeeper to ask about. A PPU_SITE set now is written into the applet.
    pub fn register(out: &mut dyn Write) -> Result<()> {
        // not canonicalized: Homebrew's bin/ppu symlink outlives an upgrade, its Cellar path does not
        let exe = std::env::current_exe()?;
        let exe = exe.to_str().context("ppu's path is not UTF-8")?;
        let site = std::env::var("PPU_SITE")
            .ok()
            .filter(|s| !s.trim().is_empty());
        let apps = PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?)
            .join("Applications");
        std::fs::create_dir_all(&apps)
            .with_context(|| format!("cannot create {}", apps.display()))?;
        let app = apps.join(APP);
        // built beside the old one, then swapped in, so a failed run leaves the old link working
        let new = apps.join(".ppu-link.new.app");
        let _ = std::fs::remove_dir_all(&new);
        let made = build(&new, &applet(exe, site.as_deref()));
        if let Err(e) = made {
            let _ = std::fs::remove_dir_all(&new);
            return Err(e);
        }
        if app.exists() {
            run(Command::new(LSREGISTER).arg("-u").arg(&app))?;
            std::fs::remove_dir_all(&app)
                .with_context(|| format!("cannot replace {}", app.display()))?;
        }
        std::fs::rename(&new, &app)
            .with_context(|| format!("cannot move the applet to {}", app.display()))?;
        run(Command::new(LSREGISTER).arg("-f").arg(&app))?;
        writeln!(out, "Wrote {}", app.display())?;
        writeln!(
            out,
            "ppu:// links open {exe}{}: the Studio's Connect button starts ppu connect. The \
             browser asks once whether to open ppu-link.",
            site.map(|s| format!(" for {s}")).unwrap_or_default()
        )?;
        Ok(())
    }

    fn build(app: &Path, script: &str) -> Result<()> {
        let source = app.with_extension("applescript");
        std::fs::write(&source, script).with_context(|| {
            format!(
                "cannot write in {}: if it belongs to root, run sudo chown $USER {} and ppu setup \
                 again",
                app.parent().unwrap().display(),
                app.parent().unwrap().display()
            )
        })?;
        let compiled = run(Command::new("osacompile").arg("-o").arg(app).arg(&source));
        let _ = std::fs::remove_file(&source);
        compiled?;
        let plist = app.join("Contents/Info.plist");
        for edit in [
            &["CFBundleIdentifier", "-string", "toys.ppu.link"][..],
            &["LSUIElement", "-bool", "true"],
            &[
                "CFBundleURLTypes",
                "-json",
                r#"[{"CFBundleURLName":"ppu.toys Connect link","CFBundleURLSchemes":["ppu"]}]"#,
            ],
        ] {
            run(Command::new("plutil")
                .arg("-replace")
                .args(edit)
                .arg(&plist))?;
        }
        // the plist edits break osacompile's signature; an ad-hoc one is what LaunchServices needs
        run(Command::new("codesign")
            .args(["--force", "--sign", "-"])
            .arg(app))
    }

    /// The applet: run ppu with the link, detached, so the applet quits at once.
    fn applet(exe: &str, site: Option<&str>) -> String {
        let env = site
            .map(|s| format!("PPU_SITE={} ", sh(s)))
            .unwrap_or_default();
        let command = format!("{env}{} ", sh(exe));
        format!(
            "on open location theLink\n\tdo shell script {} & quoted form of theLink & \
             \" >/dev/null 2>&1 &\"\nend open location\n",
            text(&command)
        )
    }

    /// One sh word.
    fn sh(word: &str) -> String {
        format!("'{}'", word.replace('\'', r"'\''"))
    }

    /// An AppleScript string literal.
    fn text(s: &str) -> String {
        format!("\"{}\"", s.replace('\\', r"\\").replace('"', r#"\""#))
    }

    fn run(command: &mut Command) -> Result<()> {
        let shown = format!("{command:?}");
        let output = command
            .output()
            .with_context(|| format!("cannot run {shown}"))?;
        if !output.status.success() {
            bail!(
                "{shown} failed ({}): {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn the_applet_quotes_the_path_and_site_for_sh_inside_applescript() {
            assert_eq!(
                super::applet("/Users/a b/it's\"/ppu", Some("http://x:1")),
                "on open location theLink\n\tdo shell script \"PPU_SITE='http://x:1' \
                 '/Users/a b/it'\\\\''s\\\"/ppu' \" & quoted form of theLink & \
                 \" >/dev/null 2>&1 &\"\nend open location\n"
            );
        }
    }
}

#[cfg(not(any(unix, windows)))]
mod platform {
    use anyhow::{bail, Result};
    use std::io::Write;

    pub fn register(_out: &mut dyn Write) -> Result<()> {
        bail!("ppu setup registers ppu:// on Linux, macOS and Windows only; run the ppu connect command the Connect panel shows")
    }
}
