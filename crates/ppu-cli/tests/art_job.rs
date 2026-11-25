//! `ppu art job <dir>`: claim, timeout, atomic results and detach, through the built binary.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn ppu() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ppu"))
}

/// Stage a job: `.in` first (if any), `.args` last, as a plugin does.
fn stage(dir: &Path, name: &str, args: &[&str], stdin: Option<&[u8]>) {
    if let Some(b) = stdin {
        std::fs::write(dir.join(format!("{name}.in")), b).unwrap();
    }
    std::fs::write(dir.join(format!("{name}.args")), args.join("\n")).unwrap();
}

fn files(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    v.sort();
    v
}

fn read(dir: &Path, name: &str, ext: &str) -> String {
    std::fs::read_to_string(dir.join(format!("{name}.{ext}"))).unwrap()
}

fn wait_for(path: &Path, secs: u64) {
    let end = Instant::now() + Duration::from_secs(secs);
    while !path.exists() {
        assert!(Instant::now() < end, "{} never appeared", path.display());
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_job_runs_with_its_args_and_stdin_and_leaves_exactly_out_err_code() {
    let d = tempfile::tempdir().unwrap();
    let dir = d.path();
    // the usage refusal of `art`, as JSON: the runner passes the args through (`\r\n` too)
    std::fs::write(dir.join("a.args"), "art\r\ncheck\r\n--json\r\n").unwrap();
    stage(
        dir,
        "b",
        &[
            "art", "check", "x.png", "--stdin", "--width", "1", "--height", "1",
        ],
        Some(b"\x00"),
    );
    let out = ppu().args(["art", "job"]).arg(dir).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        files(dir),
        ["a.code", "a.err", "a.out", "b.code", "b.err", "b.in", "b.out"],
        "no .args, .taken or .part is left behind (.in stays: the plugin sweeps it)"
    );
    let v: serde_json::Value = serde_json::from_str(&read(dir, "a", "out")).unwrap();
    assert_eq!(
        (read(dir, "a", "code").as_str(), v["reason"].clone()),
        ("2", "usage".into())
    );
    assert_eq!(read(dir, "b", "code"), "2");
    assert!(
        read(dir, "b", "err").contains("x.png"),
        "{}",
        read(dir, "b", "err")
    );
}

#[test]
fn a_job_already_taken_is_left_alone() {
    let d = tempfile::tempdir().unwrap();
    let dir = d.path();
    stage(dir, "a", &["art", "check", "--json"], None);
    std::fs::write(dir.join("a.taken"), "someone else").unwrap();
    assert!(ppu()
        .args(["art", "job"])
        .arg(dir)
        .status()
        .unwrap()
        .success());
    assert_eq!(files(dir), ["a.args", "a.taken"]);
    assert_eq!(read(dir, "a", "taken"), "someone else");
}

/// A project of its own for job `n` (so jobs never share a manifest) and the job that makes art in it.
fn art_new_job(dir: &Path, n: usize) -> PathBuf {
    let proj = dir.join(format!("p{n}"));
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(
        proj.join("ppu.json"),
        r#"{"title":"t","files":[],"sources":[]}"#,
    )
    .unwrap();
    let png = proj.join("a.png");
    stage(
        &dir.join("jobs"),
        &format!("j{n:02}"),
        &[
            "art",
            "new",
            png.to_str().unwrap(),
            "--kind",
            "bg",
            "--size",
            "8x8",
            "--new-palette",
            "pal",
        ],
        None,
    );
    png
}

#[test]
fn two_runners_racing_never_run_one_job_twice() {
    let d = tempfile::tempdir().unwrap();
    let jobs = d.path().join("jobs");
    std::fs::create_dir(&jobs).unwrap();
    let pngs: Vec<PathBuf> = (0..24).map(|n| art_new_job(d.path(), n)).collect();
    // A second run of a job would find its PNG already there and exit 2.
    let mut runners: Vec<_> = (0..3)
        .map(|_| {
            ppu()
                .args(["art", "job"])
                .arg(&jobs)
                .stdout(Stdio::null())
                .spawn()
                .unwrap()
        })
        .collect();
    for r in &mut runners {
        assert!(r.wait().unwrap().success());
    }
    for (n, png) in pngs.iter().enumerate() {
        let name = format!("j{n:02}");
        assert_eq!(
            read(&jobs, &name, "code"),
            "0",
            "{name}: {}",
            read(&jobs, &name, "err")
        );
        assert!(png.exists());
    }
    assert_eq!(files(&jobs).len(), 24 * 3, "{:?}", files(&jobs));
}

#[test]
fn code_is_the_last_file_and_no_half_written_result_is_ever_visible() {
    let d = tempfile::tempdir().unwrap();
    let jobs = d.path().join("jobs");
    std::fs::create_dir(&jobs).unwrap();
    for n in 0..8 {
        art_new_job(d.path(), n);
    }
    let mut runner = ppu().args(["art", "job"]).arg(&jobs).spawn().unwrap();
    // Poll while it runs: whenever a `.code` is there, so are its `.out` and `.err`, and the
    // `.part` of that job is gone; no `.out` or `.err` is ever seen as a `.part` rename away.
    let mut seen = 0;
    while runner.try_wait().unwrap().is_none() || seen < 8 {
        let listing = files(&jobs);
        for c in listing.iter().filter(|f| f.ends_with(".code")) {
            let name = c.trim_end_matches(".code");
            let now = files(&jobs);
            for ext in ["out", "err"] {
                assert!(
                    now.contains(&format!("{name}.{ext}")),
                    "{name}.code without .{ext}: {now:?}"
                );
            }
            assert!(!now.contains(&format!("{name}.code.part")), "{now:?}");
        }
        seen = listing.iter().filter(|f| f.ends_with(".code")).count();
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[cfg(unix)]
fn fifo(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    assert!(Command::new("mkfifo").arg(&p).status().unwrap().success());
    p
}

/// A job that blocks until its stdin closes: `.in` is a FIFO, and the test holds the write end.
#[cfg(unix)]
fn hanging_job(dir: &Path) -> std::thread::JoinHandle<std::fs::File> {
    let fifo = fifo(dir, "h.in");
    std::fs::write(
        dir.join("h.args"),
        "art\ncheck\nx.png\n--stdin\n--width\n1\n--height\n1",
    )
    .unwrap();
    std::thread::spawn(move || std::fs::OpenOptions::new().write(true).open(fifo).unwrap())
}

#[cfg(unix)]
#[test]
fn a_job_past_the_timeout_is_killed_and_reports_124() {
    let d = tempfile::tempdir().unwrap();
    let dir = d.path();
    let writer = hanging_job(dir);
    let t = Instant::now();
    let st = ppu()
        .args(["art", "job"])
        .arg(dir)
        .args(["--timeout", "1"])
        .status()
        .unwrap();
    assert!(st.success());
    assert!(t.elapsed() < Duration::from_secs(10), "{:?}", t.elapsed());
    assert_eq!(read(dir, "h", "code"), "124");
    assert_eq!(read(dir, "h", "err"), "ppu did not finish in 1 s");
    assert_eq!(files(dir), ["h.code", "h.err", "h.in", "h.out"]);
    drop(writer.join().unwrap());
}

#[cfg(unix)]
#[test]
fn detach_returns_at_once_and_the_job_still_finishes() {
    let d = tempfile::tempdir().unwrap();
    let dir = d.path();
    let writer = hanging_job(dir);
    let t = Instant::now();
    let st = ppu()
        .args(["art", "job"])
        .arg(dir)
        .arg("--detach")
        .status()
        .unwrap();
    assert!(st.success());
    assert!(t.elapsed() < Duration::from_secs(5), "{:?}", t.elapsed());
    let held = writer.join().unwrap(); // the detached runner opened the FIFO and the job runs
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !dir.join("h.code").exists(),
        "the job finished before its stdin closed"
    );
    drop(held);
    wait_for(&dir.join("h.code"), 20);
    assert_eq!(read(dir, "h", "code"), "2"); // x.png is in no project: the check's own refusal
}

#[test]
fn a_bad_command_line_is_refused() {
    let d = tempfile::tempdir().unwrap();
    for args in [
        vec!["art", "job"],
        vec!["art", "job", "--timeout", "0", "."],
        vec!["art", "job", "/no/such/dir"],
    ] {
        let o = ppu().args(&args).current_dir(d.path()).output().unwrap();
        assert_eq!(o.status.code(), Some(2), "{args:?}");
    }
}
