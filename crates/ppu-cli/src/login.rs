//! `ppu login`, `ppu whoami`, `ppu logout`: the CLI signs in to a ppu.toys account once per
//! machine. Login is device-style: the site hands out a link and a short code, the person
//! approves it in a browser, and the CLI's poll picks up a token. No localhost server.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The site `ppu login` signs in to unless `PPU_SITE` names another (staging, a local server).
pub const DEFAULT_SITE: &str = "https://ppu.toys";

/// A signed-in machine: the site and the bearer token it issued.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct Credentials {
    pub site: String,
    pub token: String,
}

/// `cmd` run against `site`, written for this machine's shell: PowerShell has no `VAR=x cmd` form.
pub fn with_site(site: &str, cmd: &str) -> String {
    match cfg!(windows) {
        true => format!("$env:PPU_SITE=\"{site}\"; {cmd}"),
        false => format!("PPU_SITE={site} {cmd}"),
    }
}

/// Where ppu works by default, as the person writes it: Windows has no `~`.
pub const PPU_HOME: &str = if cfg!(windows) {
    "%USERPROFILE%\\ppu\\"
} else {
    "~/ppu/"
};

/// A leading `~/` (or `~\`) under the home folder: Windows PowerShell 5.1 passes `~` through.
/// `%USERPROFILE%\` too, which the Studio shows on Windows and PowerShell passes through.
pub fn expand_home(raw: &str) -> PathBuf {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    let profile = raw
        .get(..14)
        .filter(|p| {
            p.eq_ignore_ascii_case("%USERPROFILE%/") || p.eq_ignore_ascii_case("%USERPROFILE%\\")
        })
        .map(|_| &raw[14..]);
    match (
        raw.strip_prefix("~/")
            .or_else(|| raw.strip_prefix("~\\"))
            .or(profile),
        home,
    ) {
        (Some(rest), Some(home)) => rest
            .split(['/', '\\'])
            .filter(|part| !part.is_empty())
            .fold(PathBuf::from(home), |dir, part| dir.join(part)),
        _ => PathBuf::from(raw),
    }
}

/// The site to sign in to: `PPU_SITE`, or ppu.toys.
pub fn site_from_env() -> String {
    std::env::var("PPU_SITE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_SITE.into())
        .trim()
        .trim_end_matches('/')
        .to_string()
}

/// Where the token lives: `ppu/credentials.json` in the platform's user config directory
/// (`%APPDATA%`, `~/Library/Application Support`, `$XDG_CONFIG_HOME` or `~/.config`), or
/// in `PPU_CONFIG_DIR` when set.
pub fn credentials_path() -> Result<PathBuf> {
    let var = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let dir = if let Some(d) = var("PPU_CONFIG_DIR") {
        d
    } else if cfg!(windows) {
        var("APPDATA").context("APPDATA is not set")?.join("ppu")
    } else if cfg!(target_os = "macos") {
        var("HOME")
            .context("HOME is not set")?
            .join("Library/Application Support/ppu")
    } else if let Some(x) = var("XDG_CONFIG_HOME") {
        x.join("ppu")
    } else {
        var("HOME").context("HOME is not set")?.join(".config/ppu")
    };
    Ok(dir.join("credentials.json"))
}

pub fn load(path: &Path) -> Result<Option<Credentials>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(serde_json::from_str(&text).with_context(|| {
            format!(
                "{} is damaged; run ppu logout, then ppu login",
                path.display()
            )
        })?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("cannot read {}", path.display())),
    }
}

/// Write the token readable by its owner only, where the OS has such permissions; on
/// Windows the user's `%APPDATA%` is already private to them. Written beside the file and
/// renamed over it, so a crash leaves the old token or the new one, never half of either.
fn save(path: &Path, creds: &Credentials) -> Result<()> {
    let dir = path.parent().context("credentials path has no directory")?;
    std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let text = serde_json::to_string_pretty(creds)? + "\n";
    let tmp = path.with_extension("json.tmp");
    // a leftover from a crash may carry looser permissions; start the file afresh
    let _ = std::fs::remove_file(&tmp);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(&tmp)
        .and_then(|mut f| {
            f.write_all(text.as_bytes())?;
            f.sync_all()
        })
        .and_then(|()| std::fs::rename(&tmp, path))
        .with_context(|| format!("cannot write {}", path.display()))
}

/// What this machine calls itself on the approval page.
fn client_name() -> String {
    let host = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
        .map(|h| h.trim().to_string())
        .filter(|h| !h.is_empty());
    match host {
        Some(h) => format!("ppu CLI on {h}"),
        None => "ppu CLI".into(),
    }
}

/// How long an authed call waits for the site to answer; a long poll that holds longer
/// (`/changes` holds up to 25 s) passes its own to [`request`].
pub const DEFAULT_READ_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// One agent with timeouts, so a stalled connection fails instead of hanging; a long poll that
/// holds longer (`/changes` holds up to 25 s) passes its own read timeout. A long-running client
/// (`ppu connect`) keeps one, so its calls reuse connections and TLS sessions.
pub fn agent(read_timeout: Duration) -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(CONNECT_TIMEOUT)
        .timeout_read(read_timeout)
        .build()
}

/// A request to `path` (like `/api/projects`) on the signed-in site with the bearer set. Send it
/// with [`call`], which returns 4xx and 5xx answers as `(status, body)` for [`refusal`] to read.
pub fn request_on(
    agent: &ureq::Agent,
    creds: &Credentials,
    method: &str,
    path: &str,
) -> ureq::Request {
    agent
        .request(method, &format!("{}{path}", creds.site))
        .set("authorization", &format!("Bearer {}", creds.token))
}

/// A response's status and JSON body; 4xx and 5xx bodies are answers here, not errors.
pub fn call(
    req: ureq::Request,
    body: Option<serde_json::Value>,
) -> Result<(u16, serde_json::Value)> {
    let url = req.url().to_string();
    let res = match body {
        Some(b) => req
            .set("content-type", "application/json")
            .send_string(&b.to_string()),
        None => req.call(),
    };
    let res = match res {
        Ok(r) => r,
        Err(ureq::Error::Status(_, r)) => r,
        Err(e) => return Err(e).with_context(|| format!("cannot reach {url}")),
    };
    let status = res.status();
    let text = res.into_string()?;
    Ok((
        status,
        serde_json::from_str(&text).unwrap_or(serde_json::Value::Null),
    ))
}

/// The message a refusal carries (`{"error": "..."}`), or its status.
pub fn refusal(status: u16, v: &serde_json::Value) -> String {
    v["error"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| format!("HTTP {status}"))
}

/// Sign this machine in to `site`: print the link and code to `out`, poll until the person
/// approves, denies or the code expires, then save the token at `path`.
pub fn login(site: &str, path: &Path, out: &mut dyn Write) -> Result<Credentials> {
    sign_in(site, path, out, false)
}

/// [`login`] with no terminal to read the link from (a ppu:// link started this): the link
/// opens in the browser, where the person sees the same consent page and code.
pub fn login_in_browser(site: &str, path: &Path, out: &mut dyn Write) -> Result<Credentials> {
    sign_in(site, path, out, true)
}

fn sign_in(site: &str, path: &Path, out: &mut dyn Write, open: bool) -> Result<Credentials> {
    let http = agent(DEFAULT_READ_TIMEOUT);
    let (status, start) = call(
        http.post(&format!("{site}/api/cli/login")),
        Some(serde_json::json!({ "name": client_name() })),
    )?;
    if status != 200 {
        bail!(
            "{site} refused to start a sign-in: {}",
            refusal(status, &start)
        );
    }
    let field = |k: &str| {
        start[k]
            .as_str()
            .map(str::to_string)
            .with_context(|| format!("{site} answered without {k}"))
    };
    let device = field("device_code")?;
    let code = field("user_code")?;
    let link = field("verification_uri_complete")?;
    let mut interval = start["interval"].as_u64().unwrap_or(5).max(1);
    let deadline =
        Instant::now() + Duration::from_secs(start["expires_in"].as_u64().unwrap_or(600));
    writeln!(out, "Open this link to sign in:\n\n  {link}\n\nand check that it shows the code {code}.\nWaiting for approval…")?;
    out.flush()?;
    if open {
        // the page words itself for a Connect pressed in the Studio, not a terminal to go back to
        crate::connect::reveal(Path::new(&format!("{link}&from=connect")), false)?;
    }
    loop {
        std::thread::sleep(Duration::from_secs(interval));
        let polled = call(
            http.post(&format!("{site}/api/cli/login/token")),
            Some(serde_json::json!({ "device_code": device })),
        );
        // a dropped request or a 5xx is not an answer: ask again until the code expires
        let (status, v) = match polled {
            Err(_) if Instant::now() < deadline => continue,
            Ok((status, _)) if status >= 500 && Instant::now() < deadline => continue,
            other => other?,
        };
        if status == 200 {
            let token = v["access_token"]
                .as_str()
                .context("the sign-in answer carried no token")?;
            let creds = Credentials {
                site: site.to_string(),
                token: token.to_string(),
            };
            // signing in again replaces this machine's token: save the new one, then revoke the
            // old one, best effort, so a failed save never leaves the machine signed out
            let old = load(path).ok().flatten();
            save(path, &creds)?;
            if let Some(old) = old.filter(|o| o.token != creds.token) {
                let _ = call(
                    request_on(
                        &agent(Duration::from_secs(5)),
                        &old,
                        "DELETE",
                        "/api/cli/token",
                    ),
                    None,
                );
            }
            return Ok(creds);
        }
        match v["error"].as_str() {
            Some("authorization_pending") if Instant::now() < deadline => {}
            Some("slow_down") => interval += 5,
            Some("access_denied") => bail!("the sign-in was denied in the browser"),
            Some("authorization_pending" | "expired_token") => {
                bail!("the code expired before it was approved; run ppu login again")
            }
            // the site restarted, or the answer that carried the token was dropped
            Some("invalid_grant") => bail!("the sign-in was lost; run ppu login again"),
            _ => bail!("{site} refused the sign-in: {}", refusal(status, &v)),
        }
    }
}

/// The signed-in account's handle, asked of the site the token came from.
pub fn whoami(creds: &Credentials) -> Result<String> {
    let (status, v) = call(
        request_on(&agent(DEFAULT_READ_TIMEOUT), creds, "GET", "/api/me"),
        None,
    )?;
    match status {
        200 => v["handle"]
            .as_str()
            .map(str::to_string)
            .context("the site answered without a handle"),
        401 => bail!("this machine's sign-in was revoked; run ppu login"),
        _ => bail!("{} refused: {}", creds.site, refusal(status, &v)),
    }
}

/// Revoke the token on its site (best effort: an unreachable or already revoked token is
/// still forgotten) and delete the file. False when nothing was signed in.
pub fn logout(path: &Path, out: &mut dyn Write) -> Result<bool> {
    let Some(creds) = load(path).ok().flatten() else {
        if path.exists() {
            std::fs::remove_file(path)?;
            return Ok(true);
        }
        return Ok(false);
    };
    let revoked = call(
        request_on(
            &agent(DEFAULT_READ_TIMEOUT),
            &creds,
            "DELETE",
            "/api/cli/token",
        ),
        None,
    );
    match revoked {
        Ok((204 | 401, _)) => {}
        Ok((status, v)) => writeln!(
            out,
            "{} did not revoke the token ({}); forgetting it anyway",
            creds.site,
            refusal(status, &v)
        )?,
        Err(e) => writeln!(out, "{e:#}; forgetting the token anyway")?,
    }
    std::fs::remove_file(path).with_context(|| format!("cannot delete {}", path.display()))?;
    Ok(true)
}

#[cfg(test)]
mod home_tests {
    #[test]
    fn expand_home_takes_both_slashes_and_leaves_the_rest() {
        let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
        let want = home.join("ppu").join("adventure");
        assert_eq!(super::expand_home("~/ppu/adventure"), want);
        assert_eq!(super::expand_home("~\\ppu\\adventure"), want);
        assert_eq!(super::expand_home("%USERPROFILE%\\ppu\\adventure"), want);
        assert_eq!(super::expand_home("%userprofile%/ppu/adventure"), want);
        assert_eq!(
            super::expand_home("/tmp/x"),
            std::path::PathBuf::from("/tmp/x")
        );
        assert_eq!(super::expand_home("a~/b"), std::path::PathBuf::from("a~/b"));
    }
}
