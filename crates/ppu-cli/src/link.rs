//! `ppu ppu://connect?project=<id>&folder=<path>&site=<origin>`: the Studio's Connect button. The
//! OS hands the link to ppu (registered by `ppu setup`) with no terminal, so everything ppu says
//! goes to the Connect panel's log, and the link runs the same pull and connect a terminal does.
//!
//! Any web page can fire a ppu:// link, so the link is a trust boundary: the project must be the
//! signed-in account's (the site refuses anyone else's), and the folder must be one the person
//! already connected to that project (it holds its `.ppu/link`), or a new or empty folder under
//! `~/ppu/`. Anywhere else takes one `ppu connect` typed in a terminal first; after that the
//! button reconnects it.

use crate::connect::{self, AlreadyRunning, Clashes, Link, LINK_FILE};
use crate::login::{self, Credentials};
use anyhow::{bail, ensure, Context, Result};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

/// What a ppu:// link asks for.
#[derive(Debug, PartialEq)]
pub struct Request {
    pub project: String,
    /// As written in the panel: absolute, or `~/...`.
    pub folder: String,
    /// The site whose Studio made the link; None in a link that names none.
    pub site: Option<String>,
}

/// Parse `ppu://connect?...` (and `ppu://connect/?...`, which some browsers deliver).
pub fn parse(url: &str) -> Result<Request> {
    let query = ["ppu://connect?", "ppu://connect/?"]
        .iter()
        .find_map(|p| url.strip_prefix(p))
        .with_context(|| format!("{url:?} is not a ppu://connect link"))?;
    let (mut project, mut folder, mut site) = (None, None, None);
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let value = decode(value).with_context(|| format!("{key} in {url:?} is not valid"))?;
        match key {
            // a bare id only: project_id would also take a URL ending in one
            "project" => {
                ensure!(
                    connect::project_id(&value).ok().as_deref() == Some(value.as_str()),
                    "{value:?} is not a project id"
                );
                project = Some(value)
            }
            "folder" => folder = Some(value),
            "site" => site = Some(value.trim_end_matches('/').to_string()),
            _ => bail!("{url:?} has {key:?}, which a ppu://connect link does not take"),
        }
    }
    Ok(Request {
        project: project.with_context(|| format!("{url:?} names no project"))?,
        folder: folder.with_context(|| format!("{url:?} names no folder"))?,
        site,
    })
}

/// Percent-decoding (`+` is a space, as browsers' form encoding writes it).
fn decode(text: &str) -> Result<String> {
    let mut bytes = Vec::with_capacity(text.len());
    let mut rest = text.as_bytes();
    while let Some((&b, tail)) = rest.split_first() {
        rest = tail;
        bytes.push(match b {
            b'+' => b' ',
            b'%' => {
                let hex = rest.get(..2).context("a % without two hex digits")?;
                rest = &rest[2..];
                u8::from_str_radix(std::str::from_utf8(hex)?, 16)?
            }
            b => b,
        });
    }
    Ok(String::from_utf8(bytes)?)
}

/// The folder a link may connect: one already linked to `project` anywhere, or a new or empty one
/// under `home/ppu/`. A folder holding another project, or anything else, is refused with what to
/// do instead.
pub fn folder(raw: &str, project: &str, home: &Path) -> Result<PathBuf> {
    let dir = match raw.strip_prefix("~/") {
        // part by part, so Windows shows C:\Users\a\ppu\x and not C:\Users\a\ppu/x
        Some(rest) => rest
            .split('/')
            .filter(|part| !part.is_empty())
            .fold(home.to_path_buf(), |dir, part| dir.join(part)),
        None => PathBuf::from(raw),
    };
    ensure!(
        dir.is_absolute(),
        "the folder {raw:?} is not a full path (like {}my-game)",
        login::PPU_HOME
    );
    ensure!(
        dir.components().all(|c| !matches!(c, Component::ParentDir)),
        "the folder {raw:?} holds .., which a link does not take"
    );
    if dir.join(LINK_FILE).exists() {
        let link = Link::read(&dir)?;
        ensure!(
            link.project == project,
            "{} is connected to project {}, not {project}: choose another folder",
            dir.display(),
            link.project
        );
        return Ok(dir);
    }
    // symlinks resolved on the part that exists, so ~/ppu/x -> /elsewhere is not under ~/ppu
    let real = |p: &Path| -> PathBuf {
        let mut tail = Vec::new();
        let mut at = p;
        loop {
            if let Ok(found) = at.canonicalize() {
                return tail.iter().rev().fold(found, |p, part| p.join(part));
            }
            match (at.parent(), at.file_name()) {
                (Some(parent), Some(name)) => {
                    tail.push(name.to_owned());
                    at = parent;
                }
                _ => return p.to_path_buf(),
            }
        }
    };
    let root = real(&home.join("ppu"));
    let under = real(&dir);
    ensure!(
        under.starts_with(&root) && under != root,
        "{} is not under {home} and not connected to this project yet. A link connects a new \
         folder under {home}, or one already connected: run ppu connect {project} {} in a terminal \
         once, and the Connect button reconnects it after that",
        dir.display(),
        dir.display(),
        home = login::PPU_HOME
    );
    Ok(dir)
}

/// Handle a ppu:// link: sign in if needed (in the browser), pull into the folder when it is new,
/// then connect, the same path `ppu connect` takes. A link for a project whose connect already
/// runs or is starting ends at once, saying so: people click again when nothing seems to happen.
pub fn run(url: &str, out: &mut dyn Write) -> Result<()> {
    let request = parse(url)?;
    let path = login::credentials_path()?;
    let signed_in = login::load(&path)?;
    let site = link_site(
        request.site.as_deref(),
        signed_in.as_ref(),
        login::site_from_env(),
    );
    if let Some(from) = &request.site {
        ensure!(
            *from == site,
            "the link came from {from}, but ppu connects to {site}; run {} to make links from {from} \
             work",
            login::with_site(from, "ppu setup")
        );
    }
    // one connect per project at a time (a terminal one too), from the click until connect ends
    let Some(_starting) = connect::hold(&connect::project_lock(&request.project)?)? else {
        writeln!(
            out,
            "ppu connect for {} is already starting or running.",
            request.project
        )?;
        return Ok(());
    };
    let creds = match signed_in {
        Some(c) if c.site == site => c,
        Some(c) => bail!(
            "this machine is signed in to {}, not {site}; run {}",
            c.site,
            login::with_site(&site, "ppu login")
        ),
        None => login::login_in_browser(&site, &path, out)?,
    };
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .context("HOME is not set")?;
    let started = start(&creds, &request, &home, out);
    match started {
        Err(e) if e.is::<AlreadyRunning>() => {
            writeln!(out, "{e}.")?;
            Ok(())
        }
        Err(e) => {
            // no terminal shows this: the panel's log does
            say(&creds, &request.project, &format!("Not connected: {e:#}"));
            Err(e)
        }
        ok => ok,
    }
}

fn start(creds: &Credentials, request: &Request, home: &Path, out: &mut dyn Write) -> Result<()> {
    let dir = folder(&request.folder, &request.project, home)?;
    say(
        creds,
        &request.project,
        &format!("Starting ppu connect in {} from the Studio.", dir.display()),
    );
    if !dir.join(LINK_FILE).exists() {
        let link = connect::pull(creds, &request.project, &dir, out)?;
        writeln!(
            out,
            "Pulled project {} (revision {}) into {}.",
            link.project,
            link.revision,
            dir.display()
        )?;
    }
    connect::connect(creds, &dir, Clashes::Panel, out)
}

/// The site a link connects to: the one this machine is signed in to when the link names it (the
/// person chose that site by signing in), else the one `ppu setup` baked in (PPU_SITE, or
/// ppu.toys). Any other site in the link is refused, since any page can fire a ppu:// link.
fn link_site(from: Option<&str>, signed_in: Option<&Credentials>, baked: String) -> String {
    match signed_in {
        Some(c) if from == Some(c.site.as_str()) => c.site.clone(),
        _ => baked,
    }
}

/// One line in the Connect panel's log, best effort.
fn say(creds: &Credentials, project: &str, text: &str) {
    let req = login::request_on(
        &login::agent(login::DEFAULT_READ_TIMEOUT),
        creds,
        "POST",
        &format!("/api/projects/{project}/changes"),
    );
    let _ = login::call(req, Some(serde_json::json!({"line": {"text": text}})));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_both_link_forms_and_refuses_strangers() {
        let want = Request {
            project: "abc123".into(),
            folder: "~/ppu/my game".into(),
            site: Some("http://localhost:3000".into()),
        };
        for url in [
            "ppu://connect?project=abc123&folder=~%2Fppu%2Fmy%20game&site=http%3A%2F%2Flocalhost%3A3000",
            "ppu://connect/?project=abc123&folder=~/ppu/my+game&site=http://localhost:3000/",
        ] {
            assert_eq!(parse(url).unwrap(), want, "{url}");
        }
        for url in [
            "ppu://pull?project=abc123&folder=/x",
            "ppu://connect?project=abc123",
            "ppu://connect?folder=/x",
            "ppu://connect?project=../etc&folder=/x",
            "ppu://connect?project=abc123&folder=/x&run=rm",
            "ppu://connect?project=abc123&folder=%zz",
        ] {
            assert!(parse(url).is_err(), "{url}");
        }
    }

    #[test]
    fn a_link_from_the_signed_in_site_connects_there_and_others_get_the_baked_site() {
        let staging = Credentials {
            site: "https://staging".into(),
            token: String::new(),
        };
        let baked = || "https://ppu.toys".to_string();
        assert_eq!(
            link_site(Some("https://staging"), Some(&staging), baked()),
            "https://staging"
        );
        assert_eq!(
            link_site(Some("https://evil"), Some(&staging), baked()),
            "https://ppu.toys"
        );
        assert_eq!(
            link_site(Some("https://staging"), None, baked()),
            "https://ppu.toys"
        );
        assert_eq!(link_site(None, Some(&staging), baked()), "https://ppu.toys");
    }

    #[test]
    fn folder_is_new_under_ppu_or_already_linked_to_the_project() {
        let home = tempfile::tempdir().unwrap();
        let home = home.path();
        assert_eq!(
            folder("~/ppu/game", "p1", home).unwrap(),
            home.join("ppu/game")
        );
        for raw in [
            "~/ppu",
            "~/elsewhere",
            "ppu/game",
            "~/ppu/../x",
            "/etc/game",
        ] {
            assert!(folder(raw, "p1", home).is_err(), "{raw}");
        }
        // a symlink inside ~/ppu that leads out is not under ~/ppu
        std::fs::create_dir_all(home.join("ppu")).unwrap();
        std::fs::create_dir_all(home.join("out")).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(home.join("out"), home.join("ppu/escape")).unwrap();
            assert!(folder("~/ppu/escape/game", "p1", home).is_err());
        }
        // a folder anywhere that is already linked to this project, and only this project
        let linked = home.join("work/game");
        Link {
            project: "p1".into(),
            ..Link::default()
        }
        .write(&linked)
        .unwrap();
        let raw = linked.to_str().unwrap();
        assert_eq!(folder(raw, "p1", home).unwrap(), linked);
        assert!(folder(raw, "p2", home).is_err());
    }
}
