//! `ppu art job <dir>`: runs the jobs an editor plugin staged in `dir`, so a plugin starts `ppu`
//! with one constant command line (no shell, no `timeout` binary, no `&`) and polls files.
//!
//! A job is `<name>.args` (one `ppu` argument per line, UTF-8; the runner runs this same
//! executable with them) and optionally `<name>.in` (its stdin; none means empty). The plugin
//! writes `.in` first and `.args` last, so a job is never claimed half-staged. The runner claims
//! a job by renaming `.args` to `.taken`; the one whose rename wins runs it, and the other skips.
//! Results land as `<name>.out`, `<name>.err` (written under `.part` names and renamed) and then
//! `<name>.code` (the exit code), last: `.code` is the one signal that the job is done, and
//! whoever sees it can read `.out` and `.err`. A job past `--timeout` is killed: code 124.

use anyhow::{bail, ensure, Context, Result};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const DEFAULT_TIMEOUT: u64 = 20;
/// The exit code of a job that outlived its timeout (what coreutils `timeout` reports too).
const TIMED_OUT: i32 = 124;

/// `ppu art job <dir> [--detach] [--timeout SECONDS]`: the exit code.
pub fn run(args: Vec<String>) -> i32 {
    match run_inner(args) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("ppu: {e:#}");
            2
        }
    }
}

fn run_inner(args: Vec<String>) -> Result<()> {
    let (mut detach, mut timeout, mut dir) = (false, DEFAULT_TIMEOUT, None);
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--detach" => detach = true,
            "--timeout" => {
                let v = it.next().context("--timeout requires a value")?;
                timeout =
                    v.parse().ok().filter(|n| *n > 0).with_context(|| {
                        format!("--timeout wants whole seconds above 0, got {v:?}")
                    })?;
            }
            _ if dir.is_none() && !a.starts_with("--") => dir = Some(PathBuf::from(a)),
            _ => bail!("usage: ppu art job <dir> [--detach] [--timeout SECONDS]"),
        }
    }
    let dir = dir.context("usage: ppu art job <dir> [--detach] [--timeout SECONDS]")?;
    ensure!(dir.is_dir(), "{} is not a directory", dir.display());
    if detach {
        return spawn_detached(&dir, timeout);
    }
    let mut jobs: Vec<String> = std::fs::read_dir(&dir)
        .with_context(|| format!("cannot read {}", dir.display()))?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter_map(|n| n.strip_suffix(".args").map(String::from))
        .collect();
    jobs.sort();
    // A job that cannot finish (its files swept under it, say) must not strand the others.
    let mut failed = None;
    for name in jobs {
        if let Err(e) = run_job(&dir, &name, Duration::from_secs(timeout)) {
            failed = Some(e);
        }
    }
    failed.map_or(Ok(()), Err)
}

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// This runner again, without `--detach`, in the background: stdio closed, not waited for.
fn spawn_detached(dir: &Path, timeout: u64) -> Result<()> {
    let mut c = Command::new(std::env::current_exe().context("cannot find the ppu executable")?);
    c.args(["art", "job"])
        .arg(dir)
        .args(["--timeout", &timeout.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW only: DETACHED_PROCESS would void it and leave no console for the
        // jobs' ppu children to inherit, so each would open a visible one. The runner keeps a
        // hidden console, and outlives the caller anyway.
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c.spawn().context("cannot start the job runner")?;
    Ok(())
}

fn side(dir: &Path, name: &str, ext: &str) -> PathBuf {
    dir.join(format!("{name}.{ext}"))
}

/// `contents` to `<name>.<ext>` by writing `<ext>.part` and renaming it.
fn publish(dir: &Path, name: &str, ext: &str, contents: &[u8]) -> Result<()> {
    let part = side(dir, name, &format!("{ext}.part"));
    std::fs::write(&part, contents).with_context(|| format!("cannot write {}", part.display()))?;
    land(dir, name, ext)
}

fn land(dir: &Path, name: &str, ext: &str) -> Result<()> {
    let (part, done) = (
        side(dir, name, &format!("{ext}.part")),
        side(dir, name, ext),
    );
    std::fs::rename(&part, &done).with_context(|| format!("cannot write {}", done.display()))
}

/// One job: claim, run, publish `.out`, `.err`, then `.code`.
fn run_job(dir: &Path, name: &str, timeout: Duration) -> Result<()> {
    let (args_file, taken) = (side(dir, name, "args"), side(dir, name, "taken"));
    // rename replaces an existing file on unix: a job already taken is left alone first.
    if taken.exists() || std::fs::rename(&args_file, &taken).is_err() {
        return Ok(());
    }
    let (out, err) = (side(dir, name, "out.part"), side(dir, name, "err.part"));
    let mut err_text = None;
    let code = match start(dir, name, &taken, &out, &err) {
        Ok(mut child) => wait(&mut child, timeout).unwrap_or_else(|e| {
            err_text = Some(format!("{e:#}"));
            2
        }),
        Err(e) => {
            err_text = Some(format!("{e:#}"));
            2
        }
    };
    if code == TIMED_OUT {
        err_text = Some(format!("ppu did not finish in {} s", timeout.as_secs()));
    }
    if let Some(text) = err_text {
        std::fs::write(&err, text).with_context(|| format!("cannot write {}", err.display()))?;
        if !out.exists() {
            File::create(&out)?;
        }
    }
    land(dir, name, "out")?;
    land(dir, name, "err")?;
    publish(dir, name, "code", code.to_string().as_bytes())?;
    let _ = std::fs::remove_file(taken);
    Ok(())
}

fn start(
    dir: &Path,
    name: &str,
    taken: &Path,
    out: &Path,
    err: &Path,
) -> Result<std::process::Child> {
    let text = std::fs::read_to_string(taken)
        .with_context(|| format!("cannot read {} as UTF-8 text", taken.display()))?;
    let stdin = match File::open(side(dir, name, "in")) {
        Ok(f) => Stdio::from(f),
        Err(_) => Stdio::null(),
    };
    let mut c = Command::new(std::env::current_exe().context("cannot find the ppu executable")?);
    c.args(text.lines())
        .stdin(stdin)
        .stdout(File::create(out)?)
        .stderr(File::create(err)?);
    // A runner started some other way may have no console: never let a job open one.
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut c, CREATE_NO_WINDOW);
    c.spawn().context("cannot start ppu")
}

/// The child's exit code, or [`TIMED_OUT`] after killing it at `timeout`.
fn wait(child: &mut std::process::Child, timeout: Duration) -> Result<i32> {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            // A signal leaves no code: report 1, so a poller still sees a failure.
            return Ok(status.code().unwrap_or(1));
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(TIMED_OUT);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
