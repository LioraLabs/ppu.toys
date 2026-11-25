//! `ppu build`: the folder built into a SNES cartridge by the signed-in site's compiler, the same
//! build the Studio's Export to ROM runs. The compiler is the site's; this sends the packed folder,
//! polls the job and hands back the image.

use crate::login::{self, Credentials};
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

/// How often a pending job is asked about, and for how long, before giving up on it.
const POLL_EVERY: Duration = Duration::from_secs(1);
const GIVE_UP_AFTER: Duration = Duration::from_secs(20 * 60);
/// A line while a build runs, so an agent's shell (and a person) sees it is alive.
const SAY_EVERY: Duration = Duration::from_secs(30);

/// Where `ppu build` puts an image by default: `build/` inside the folder, which ignores itself
/// (git, and `ppu connect`, which only syncs what ppu.json names).
pub const BUILD_DIR: &str = "build";

/// `YYYYMMDD-HHMMSS` in UTC for `secs` since the Unix epoch: the default image's name, so each
/// build keeps its own file.
pub fn stamp(secs: u64) -> String {
    let (days, rem) = ((secs / 86400) as i64, secs % 86400);
    // days to a civil date (Howard Hinnant's algorithm)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}", rem / 3600, rem / 60 % 60, rem % 60)
}

/// A diagnostic's file as the agent can open it from where the command ran: the folder's own
/// files under `dir` (unless `dir` is `.`); the engine's (`<engine>/…`, `@ppu/…`) as the compiler names them.
pub(crate) fn shown(dir: &Path, file: &str) -> String {
    if dir == Path::new(".") || file.starts_with('<') || file.starts_with('@') {
        file.to_string()
    } else {
        dir.join(file).display().to_string()
    }
}

/// Build `dir` on the site `creds` signed in to. Prints each compiler diagnostic to `out` as
/// `file:line: severity[code]: message` (with an indented `docs:` pointer line), and returns the .sfc
/// image, or fails naming why it did not build.
pub fn build(creds: &Credentials, dir: &Path, out: &mut dyn Write) -> Result<Vec<u8>> {
    let mut body: serde_json::Value = serde_json::from_str(&crate::pack(dir)?)?;
    body["async"] = true.into();
    // the upload can be several MB of art: give it longer than a plain call
    let http = login::agent(Duration::from_secs(120));
    writeln!(out, "Building {} on {}…", dir.display(), creds.site)?;
    out.flush()?;
    let (mut status, mut v) = login::call(
        login::request_on(&http, creds, "POST", "/api/rom"),
        Some(body),
    )?;
    let started = Instant::now();
    let deadline = started + GIVE_UP_AFTER;
    let mut said = started;
    if status == 202 {
        writeln!(out, "Queued as {}{}; waiting (up to {} minutes).", creds.site, v["job"].as_str().unwrap_or("?"), GIVE_UP_AFTER.as_secs() / 60)?;
        out.flush()?;
    }
    while status == 202 {
        let job = v["job"]
            .as_str()
            .context("the site answered a pending build without its job")?
            .to_string();
        if Instant::now() > deadline {
            bail!(
                "the build is still running after {} minutes; try again later",
                GIVE_UP_AFTER.as_secs() / 60
            );
        }
        std::thread::sleep(POLL_EVERY);
        if said.elapsed() >= SAY_EVERY {
            said = Instant::now();
            writeln!(out, "Still building ({} s)…", started.elapsed().as_secs())?;
            out.flush()?;
        }
        (status, v) = login::call(login::request_on(&http, creds, "GET", &job), None)?;
    }
    match status {
        200 => {}
        401 => bail!("this machine's sign-in was revoked; run ppu login"),
        404 => bail!("the build expired before it was fetched; run ppu build again"),
        _ => bail!(
            "{} refused the build: {}",
            creds.site,
            login::refusal(status, &v)
        ),
    }
    let mut errors = 0;
    for d in v["diagnostics"].as_array().into_iter().flatten() {
        let severity = d["severity"].as_str().unwrap_or("error");
        errors += usize::from(severity == "error");
        writeln!(
            out,
            "{}:{}: {severity}[{}]: {}",
            shown(dir, d["file"].as_str().unwrap_or("?")),
            d["line"],
            d["code"].as_str().unwrap_or(""),
            d["message"].as_str().unwrap_or("")
        )?;
        writeln!(out, "{}", ppu_rules::docs::pointer(d["code"].as_str().unwrap_or("")))?;
    }
    if v["ok"] != true {
        match v["error"].as_str() {
            Some(e) => bail!("the cartridge did not build: {e}"),
            None => bail!("the cartridge did not build: {errors} error(s) above"),
        }
    }
    let rom = v["rom"]
        .as_str()
        .context("the site answered a build without its image")?;
    BASE64.decode(rom).context("the site sent a damaged image")
}
