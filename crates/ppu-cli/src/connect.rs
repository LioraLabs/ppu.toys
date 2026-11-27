//! `ppu connect`: a folder on disk is a client of one project on the site, the way a browser
//! tab is. Pull writes the project in the `ppu pack`/`unpack` layout and links the folder in
//! `.ppu/link`; connect then turns each settled folder save into a revision-checked project
//! save, while a listen on the project's change stream tells open Studio tabs a folder is
//! connected and brings each revision saved elsewhere (the Studio) into the folder, entry by
//! entry against the link's base. An entry both sides changed is the person's call: Keep
//! folder's or Take site's, asked in the Studio's Connect panel or settled at start by `--keep`.
//! A folder copy the site's replaces goes to `.ppu-backup/`. Everything connect prints also goes
//! to the panel's log over the change stream, and a Disconnect there ends connect: one path,
//! whether a terminal or a ppu:// link started it.
//!
//! Art lives in the folder as the artist's files: an image source is an indexed PNG
//! (`assets/<name>.png`, or wherever a tileset in the folder looks for it), a palette source a `.gpl` (`palettes/<name>.gpl`), both listed in `ppu.json`;
//! a song is a `.song.json` (`music/<name>.song.json`). The site stores only payloads. [`land`]
//! is the one place a payload becomes those files (a pull, a revision brought in, an old folder's
//! `.bin`s), and a payload only lands as a file that imports back to exactly its bytes: anything
//! else stays a `.ppu/sources` payload, with a note.

use crate::folder::{back_up, own_palette};
use crate::login::{self, Credentials};
use crate::{fnv, write_temp};
use anyhow::{bail, ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ppu_core::render::render_png;
/// Folder is mine, the site theirs: [`compare`] is where base, folder and site hashes meet.
pub use ppu_core::sync::{compare, hashes, Compared};
use ppu_core::Rule;
use ppu_core::{
    convert_png_source, gpl, row_size_for, ConvertOptions, PaletteSource, SourceKind, SourceMeta,
    SourcePayload,
};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::Path;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

/// The link: which project the folder belongs to and what it held at the last sync. It lives
/// inside `.ppu/` because the pack layout already owns that directory (`.ppu/sources/`).
pub const LINK_FILE: &str = ".ppu/link";
pub use crate::BACKUP_DIR;
/// The compare key for the file order (Lua runs files in this order).
const ORDER: &str = "order files";
/// How often connect looks at the folder; a change counts once it holds still for one look.
const POLL: Duration = Duration::from_millis(400);
/// The change stream holds a listen up to 25 s; the read timeout must outlast it.
const LISTEN_WAIT_SECS: u64 = 25;
const LISTEN_TIMEOUT: Duration = Duration::from_secs(35);
/// A save or fetch that fails in transit is tried again, waiting this long and doubling up to MAX.
const RETRY_MIN: Duration = Duration::from_secs(1);
const RETRY_MAX: Duration = Duration::from_secs(30);
/// The body fields a folder owns; anything else the site keeps (tags, forkedFrom) rides along.
const FOLDER_FIELDS: [&str; 6] = [
    "title",
    "description",
    "files",
    "sources",
    "targetHz",
    "romSize",
];

/// `.ppu/link`: the project id, the revision last synced, and a hash per file and source as of
/// that revision (the base a later per-file merge compares both sides against), the file order
/// then, and the hashes of a save sent but not answered yet.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Link {
    pub project: String,
    /// The site the project lives on (`site = <url>`); empty in links written before it was recorded.
    pub site: String,
    pub revision: i64,
    /// `file <name>`, `source <name>` and `meta <field>` keys, see [`hashes`].
    pub hashes: BTreeMap<String, String>,
    /// The file order at that revision (`order <name> = <index>` lines); empty in older links.
    pub order: Vec<String>,
    /// [`hashes`] of the save in flight (`pending <key>` lines): a site revision holding exactly
    /// these is this folder's own save whose answer was lost, not a change made elsewhere.
    pub pending: BTreeMap<String, String>,
}

impl Link {
    pub fn read(dir: &Path) -> Result<Link> {
        let path = dir.join(LINK_FILE);
        let text = match std::fs::read_to_string(&path) {
            // A pull writes the link last: a project with no link is a pull that stopped midway.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && dir.join(crate::MANIFEST).exists() => bail!(
                "{} holds a project but is not linked to one (no {LINK_FILE}). A pull into it that \
                 stopped midway: delete the folder and pull again. A folder made by ppu new is \
                 not on the site: keep it, and pull a site project into a new folder instead",
                dir.display()
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => bail!(
                "{} is not linked to a project (no {LINK_FILE}); run ppu connect <project link> <dir>",
                dir.display()
            ),
            r => r.with_context(|| format!("cannot read {}", path.display()))?,
        };
        let mut link = Link::default();
        let (mut project, mut revision) = (false, false);
        let mut order = Vec::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let bad = || format!("{} line {}: {line:?}", path.display(), n + 1);
            // Names may hold `=`; values (ids, numbers, hex hashes) never do.
            let (key, value) = line
                .rsplit_once('=')
                .with_context(|| bad() + " is not key = value")?;
            let (key, value) = (key.trim(), value.trim());
            match key {
                "project" => (link.project, project) = (value.to_string(), true),
                "site" => link.site = value.to_string(),
                "revision" => {
                    link.revision = value.parse().with_context(|| bad() + " needs a number")?;
                    revision = true;
                }
                _ if key.starts_with("order ") => order.push((
                    value
                        .parse::<usize>()
                        .with_context(|| bad() + " needs a number")?,
                    key["order ".len()..].to_string(),
                )),
                _ if key.starts_with("pending ") => {
                    link.pending
                        .insert(key["pending ".len()..].to_string(), value.to_string());
                }
                _ if ["file ", "source ", "meta "]
                    .iter()
                    .any(|p| key.starts_with(p)) =>
                {
                    link.hashes.insert(key.to_string(), value.to_string());
                }
                // A key from a newer ppu (or a hand note) is not this version's business.
                _ => {}
            }
        }
        ensure!(
            project && revision && !link.project.is_empty(),
            "{} needs a project and a revision line",
            path.display()
        );
        order.sort();
        link.order = order.into_iter().map(|(_, name)| name).collect();
        Ok(link)
    }

    /// Written to a temporary file of its own and renamed over the old one, so a crash never leaves
    /// half a link.
    pub fn write(&self, dir: &Path) -> Result<()> {
        let mut text = String::from(
            "# ppu connect: this folder is a client of a ppu.toys project.\n\
             # The revision it last synced and each file's hash as of that revision.\n",
        );
        if !self.site.is_empty() {
            text += &format!("site = {}\n", self.site);
        }
        text += &format!("project = {}\nrevision = {}\n", self.project, self.revision);
        let lines = self.hashes.iter().map(|(k, h)| (k.clone(), h.clone()));
        let order =
            (self.order.iter().enumerate()).map(|(i, n)| (format!("order {n}"), i.to_string()));
        let pending = (self.pending.iter()).map(|(k, h)| (format!("pending {k}"), h.clone()));
        for (key, value) in lines.chain(order).chain(pending) {
            ensure!(
                !key.contains('\n'),
                "a name with a line break cannot be linked: {key:?}"
            );
            text += &format!("{key} = {value}\n");
        }
        let path = dir.join(LINK_FILE);
        let temp = write_temp(dir, "link", text.as_bytes())?;
        std::fs::rename(&temp, &path).map_err(|e| {
            let _ = std::fs::remove_file(&temp);
            anyhow::Error::new(e).context(format!("cannot write {}", path.display()))
        })
    }
}

/// The file names of a body, in its order.
fn file_order(body: &Value) -> Vec<String> {
    let files = body["files"].as_array().into_iter().flatten();
    files
        .map(|f| f["name"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// The project id in a project link: a bare id, `…?project=<id>`, or a URL ending in the id.
pub fn project_id(link: &str) -> Result<String> {
    let link = link.trim();
    let id = match link.split_once("project=") {
        Some((_, rest)) => rest.split(['&', '#']).next().unwrap_or_default(),
        None => link
            .split(['?', '#'])
            .next()
            .unwrap_or_default()
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or_default(),
    };
    ensure!(
        !id.is_empty()
            && id.len() <= 64
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
        "{link:?} is not a project link or id"
    );
    Ok(id.to_string())
}

/// On a 401: a `ppu login` in another terminal since this connect started replaced (and revoked)
/// the token it holds. Take the saved one when it differs; false when there is nothing new.
fn signed_in_again(creds: &mut Credentials) -> bool {
    let saved = login::credentials_path()
        .ok()
        .and_then(|path| login::load(&path).ok().flatten());
    match saved {
        Some(saved) if saved.site == creds.site && saved.token != creds.token => {
            *creds = saved;
            true
        }
        _ => false,
    }
}

/// [`login::call`], sent again with the machine's new token when the one in hand was revoked by a
/// sign-in since ([`signed_in_again`]).
fn send(
    agent: &ureq::Agent,
    creds: &mut Credentials,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> Result<(u16, Value)> {
    loop {
        let answer = login::call(login::request_on(agent, creds, method, path), body.clone())?;
        if answer.0 != 401 || !signed_in_again(creds) {
            return Ok(answer);
        }
    }
}

/// A project of the signed-in account: its revision and body.
pub fn fetch(agent: &ureq::Agent, creds: &mut Credentials, id: &str) -> Result<(i64, Value)> {
    let (status, v) = send(agent, creds, "GET", &format!("/api/projects/{id}"), None)?;
    match status {
        200 => Ok((
            v["revision"]
                .as_i64()
                .context("the site answered without a revision")?,
            v["body"].clone(),
        )),
        _ => Err(refused(creds, id, status, &v)),
    }
}

fn refused(creds: &Credentials, id: &str, status: u16, v: &Value) -> anyhow::Error {
    match status {
        401 => anyhow::anyhow!(
            "this machine is signed out of {}; run ppu login",
            creds.site
        ),
        404 => anyhow::anyhow!(
            "no project {id} on {}: it was deleted, or it belongs to another account (ppu whoami)",
            creds.site
        ),
        _ => anyhow::anyhow!("{} refused: {}", creds.site, login::refusal(status, v)),
    }
}

/// Write a site body into an EMPTY `dir` in the pack layout, with the editor scaffolding `ppu new`
/// writes where it is missing, art as PNG and `.gpl` files ([`convert_bins`]), and link the folder
/// at `revision`. Never for a folder that holds a project; [`bring_in`] updates an existing folder.
fn write_new_folder(
    dir: &Path,
    site: &str,
    project: &str,
    revision: i64,
    body: &Value,
    out: &mut dyn Write,
) -> Result<Link> {
    let mut file = body.clone();
    file["version"] = "ppu.toys/1".into();
    crate::unpack(&file.to_string(), dir)?;
    // Art becomes PNGs and .gpl files; the link below still records the site's hashes, so the
    // first sync sends only what the conversion added (palette sources, options.palette).
    convert_bins(dir, None, out)?;
    let names = hashes(body);
    if names
        .keys()
        .any(|k| k.starts_with("file ") && k.ends_with(".tmj"))
        && !dir.join("project.tiled-project").exists()
    {
        crate::author::write_tiled_project(dir)?;
    }
    let link = Link {
        project: project.to_string(),
        site: site.to_string(),
        revision,
        hashes: names,
        order: file_order(body),
        pending: BTreeMap::new(),
    };
    crate::author::write_ignore(dir)?;
    link.write(dir)?;
    Ok(link)
}

/// `ppu connect <project link> <dir>`: pull a project into a new (or empty) folder, announcing
/// on `out` the palettes the pull created.
pub fn pull(
    creds: &Credentials,
    project_link: &str,
    dir: &Path,
    out: &mut dyn Write,
) -> Result<Link> {
    let id = project_id(project_link)?;
    let occupied = dir
        .read_dir()
        .map(|mut d| d.next().is_some())
        .unwrap_or(false);
    ensure!(
        !occupied,
        "{} is not empty; pull into a new folder, or run ppu connect inside a linked one",
        dir.display()
    );
    let agent = login::agent(login::DEFAULT_READ_TIMEOUT);
    let (revision, body) = fetch(&agent, &mut creds.clone(), &id)?;
    write_new_folder(dir, &creds.site, &id, revision, &body, out)
}

/// The folder as a project body (what `ppu pack` makes of it) with the hashes of the bytes the
/// bring-in's re-check holds the folder to: ppu.json and the PNGs its sources name, taken
/// before the pack so nothing the person does after that (a prompt may wait) goes unseen.
struct Packed {
    body: Value,
    manifest: Option<String>,
    assets: BTreeMap<String, String>,
}

fn folder_body(dir: &Path) -> Result<Packed> {
    let hash = |name: &str| std::fs::read(dir.join(name)).ok().map(|b| fnv(&[&b]));
    let manifest = hash(crate::MANIFEST);
    let mut assets = BTreeMap::new();
    if let Ok(m) = crate::read_manifest(dir) {
        for s in &m.sources {
            for path in [&s.file, &s.priority_file].into_iter().flatten() {
                let path = path.replace('\\', "/");
                if let Some(h) = hash(&path) {
                    assets.insert(path, h);
                }
            }
        }
    }
    let body = serde_json::from_str(&crate::pack(dir)?)?;
    Ok(Packed {
        body,
        manifest,
        assets,
    })
}

/// The site's body with the folder's fields in place: what a save sends.
fn merge(site: &Value, folder: &Value) -> Value {
    let mut body = site.clone();
    if !body.is_object() {
        body = Value::Object(Default::default());
    }
    let map = body.as_object_mut().expect("an object");
    for key in FOLDER_FIELDS {
        match folder.get(key) {
            Some(v) => map.insert(key.into(), v.clone()),
            None => map.remove(key),
        };
    }
    map.remove("expectedRevision");
    body
}

/// Every byte `ppu pack` would read, hashed: the manifest and each file and source it names.
/// Cheap next to pack itself, which converts PNG sources.
fn fingerprint(dir: &Path) -> String {
    let read = |name: &str| std::fs::read(dir.join(name.replace('\\', "/"))).unwrap_or_default();
    let mut parts = vec![read(crate::MANIFEST)];
    if let Ok(m) = crate::read_manifest(dir) {
        let sources = m
            .sources
            .iter()
            .flat_map(|s| [&s.payload, &s.file, &s.priority_file]);
        for name in m.files.iter().chain(sources.flatten()) {
            parts.push(name.as_bytes().to_vec());
            parts.push(read(name));
        }
    }
    fnv(&parts.iter().map(Vec::as_slice).collect::<Vec<_>>())
}

/// True when a site body and a folder body hold the same project: files, sources and the
/// manifest fields, compared the way a folder sees them (so `builtinId: null` vs absent is equal).
fn same_content(site: &Value, folder: &Value) -> bool {
    let order = |b: &Value| {
        b["files"]
            .as_array()
            .map(|f| f.iter().map(|f| f["name"].clone()).collect::<Vec<_>>())
    };
    // File order is part of the project (pack keeps it), the hashes alone are not.
    hashes(site) == hashes(folder)
        && order(site) == order(folder)
        && ["title", "description", "targetHz", "romSize"]
            .iter()
            .all(|k| {
                site.get(*k).filter(|v| !v.is_null()) == folder.get(*k).filter(|v| !v.is_null())
            })
}

/// Which copy wins an entry the folder and the site both changed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Keep {
    Folder,
    Site,
}

/// Asks the person which copy of a clashing entry wins, given the entry (as [`describe`] names
/// it) and the question.
pub type Ask = Box<dyn FnMut(&str, &str) -> Result<Keep> + Send>;

/// How connect settles entries both sides changed.
pub enum Clashes {
    /// `--keep folder|site`: one answer for every clash connect finds when it starts; the
    /// clashes of later, live revisions go to the inner policy (the panel's question or a refusal),
    /// so a Studio edit made after the answer was given is never settled without the person.
    AtStart(Keep, Box<Clashes>),
    /// Ask the person about each entry in the Studio's Connect panel, over the change stream.
    Panel,
    /// Ask about each entry: takes the entry and the question, returns the choice.
    Ask(Ask),
    /// Refuse, naming the clashes, with nothing written.
    Refuse,
}

/// The person pressed Disconnect in the Studio's Connect panel: connect ends, cleanly.
#[derive(Debug)]
pub struct Disconnected;

impl std::fmt::Display for Disconnected {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "disconnected in the Studio")
    }
}

impl std::error::Error for Disconnected {}

/// Another `ppu connect` already runs in this folder (from a terminal or a ppu:// link): a second
/// one would sync the same files twice, so it does not start.
#[derive(Debug)]
pub struct AlreadyRunning;

impl std::fmt::Display for AlreadyRunning {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "ppu connect is already running in this folder")
    }
}

impl std::error::Error for AlreadyRunning {}

/// The lock one project's `ppu connect` holds, from a terminal or a ppu:// link, so a project never
/// has two (each folder has its own lock too, `.ppu/connect.lock`).
pub fn project_lock(project: &str) -> Result<std::path::PathBuf> {
    Ok(login::credentials_path()?.with_file_name(format!("connect-{project}.lock")))
}

/// An exclusive lock on `path` (created if missing), held while the file lives; the OS drops it
/// when the process ends, however it ends. None while another process (or handle) holds it.
pub fn hold(path: &Path) -> Result<Option<std::fs::File>> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
        .with_context(|| format!("cannot open {}", path.display()))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(e)) => {
            Err(e).with_context(|| format!("cannot lock {}", path.display()))
        }
    }
}

/// A folder entry changed between compare and the write that would replace it: nothing past this
/// point was written and the link did not move, so the next look compares again.
#[derive(Debug)]
struct FolderMoved(String);

impl std::fmt::Display for FolderMoved {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{} changed in this folder while the site's revision was being written",
            self.0
        )
    }
}

impl std::error::Error for FolderMoved {}

/// The read-to-write re-check: `name` still holds what compare judged (`was`, None: absent), or
/// already holds what is about to be written there.
fn unchanged(dir: &Path, name: &str, was: Option<&String>, becomes: Option<&String>) -> Result<()> {
    let now = std::fs::read(dir.join(name)).ok().map(|b| fnv(&[&b]));
    if now.as_ref() == was || now.is_some() && now.as_ref() == becomes {
        Ok(())
    } else {
        Err(FolderMoved(name.to_string()).into())
    }
}

/// An entry as the person knows it: `main.luau`, `the source ship`, `the title`.
fn describe(key: &str) -> String {
    match key.split_once(' ') {
        _ if key == ORDER => "the file order".into(),
        Some(("file", name)) => name.into(),
        Some(("source", name)) => format!("the source {name}"),
        Some(("meta", "targetHz")) => "the frame rate (targetHz)".into(),
        Some(("meta", "romSize")) => "the ROM size (romSize)".into(),
        Some(("meta", field)) => format!("the {field}"),
        _ => key.into(),
    }
}

/// The clashes the site's copy wins, by the person's choice; the rest the folder keeps.
fn settle(
    clashes: &[String],
    revision: i64,
    how: &mut Clashes,
    out: &mut dyn Write,
) -> Result<BTreeSet<String>> {
    match how {
        _ if clashes.is_empty() => Ok(BTreeSet::new()),
        Clashes::AtStart(Keep::Folder, _) => Ok(BTreeSet::new()),
        Clashes::AtStart(Keep::Site, _) => Ok(clashes.iter().cloned().collect()),
        // connect turns Panel into the panel's Ask; anywhere else there is no stream to ask on
        Clashes::Refuse | Clashes::Panel => {
            bail!(
            "this folder and the site's revision {revision} both changed {}. Nothing was written. \
             Run ppu connect to choose for each in the Studio's Connect panel, or settle them all: \
             --keep folder saves the folder's copies over the site's; --keep site writes the \
             site's into this folder and keeps the folder's in {BACKUP_DIR}/{revision}/",
            clashes.iter().map(|k| describe(k)).collect::<Vec<_>>().join(", ")
        )
        }
        Clashes::Ask(ask) => {
            let mut site = BTreeSet::new();
            for key in clashes {
                let question = format!(
                    "This folder and the site's revision {revision} both changed {}. Keep \
                     folder's, or take site's (the folder's copy is kept in {BACKUP_DIR}/{revision}/)?",
                    describe(key)
                );
                writeln!(out, "{question} Answer in the Studio's Connect panel.")?;
                out.flush()?;
                if ask(&describe(key), &question)? == Keep::Site {
                    site.insert(key.clone());
                }
            }
            Ok(site)
        }
    }
}

/// `file main.luau` -> `main.luau`.
fn entry_name(key: &str) -> &str {
    key.split_once(' ').map_or(key, |(_, name)| name)
}

/// The entries that differ between two hash sets, by name; the manifest when only it changed.
fn changed<'a>(
    old: &'a BTreeMap<String, String>,
    new: &'a BTreeMap<String, String>,
) -> Vec<&'a str> {
    let mut names: Vec<&str> = new
        .iter()
        .filter(|(k, h)| old.get(*k) != Some(*h))
        .map(|(k, _)| k.as_str())
        .chain(
            old.keys()
                .filter(|k| !new.contains_key(*k))
                .map(String::as_str),
        )
        .map(entry_name)
        .collect();
    if names.is_empty() {
        names.push(crate::MANIFEST);
    }
    names
}

/// The entry names a folder lists after taking `taken` from the site, in the site's order with
/// the folder's own additions after (`site_order`), or the folder's with the site's additions after.
fn arrange(
    site: &[String],
    folder: &[String],
    taken: impl Fn(&str) -> bool,
    site_order: bool,
) -> Vec<String> {
    // A site name stays when taken or still in the folder; a folder-only name unless the site dropped it.
    let stays = |n: &String| match site.contains(n) {
        true => taken(n) || folder.contains(n),
        false => !taken(n) && folder.contains(n),
    };
    let (first, then) = if site_order {
        (site, folder)
    } else {
        (folder, site)
    };
    let mut out: Vec<String> = first.iter().filter(|n| stays(n)).cloned().collect();
    for n in then {
        if stays(n) && !out.contains(n) {
            out.push(n.clone());
        }
    }
    out
}

/// Bring a site revision into a linked folder: write only the entries the site changed and the
/// folder did not (files, sources, the file order and the manifest's title, description, targetHz
/// and romSize), remove the ones it dropped, keep everything else (PNG `file:` sources, the
/// folder's own unsaved edits), and link the folder at `revision`. An entry both sides changed is
/// settled by `clashes`; the folder's copy of one the site wins goes to `.ppu-backup/<revision>/`
/// first. Refuses, writing nothing, when the site sent a shape or name that cannot be written or
/// `clashes` refuses. Every overwrite and removal first re-reads the path: a folder edit made
/// since the compare stops the bring-in ([`FolderMoved`]) before the link moves. Site-changed art
/// and palettes land as PNG and `.gpl` files ([`land`]); any other source, or one that cannot land,
/// is a fresh `.ppu/sources/N.bin` payload. Dropped files go
/// last, after the manifest and link, so an interruption leaves a stray file rather than a folder
/// that will not pack.
pub fn bring_in(
    dir: &Path,
    link: &Link,
    revision: i64,
    site: &Value,
    clashes: &mut Clashes,
    out: &mut dyn Write,
) -> Result<Link> {
    let packed = folder_body(dir)
        .context("this folder does not pack, so the site's changes cannot be compared with it")?;
    bring_in_folder(dir, link, revision, site, &packed, clashes, out)
}

fn bring_in_folder(
    dir: &Path,
    link: &Link,
    revision: i64,
    site: &Value,
    packed: &Packed,
    clashes: &mut Clashes,
    out: &mut dyn Write,
) -> Result<Link> {
    let folder = &packed.body;
    let list = |k: &str| site[k].as_array().cloned().unwrap_or_default();
    let text = |v: &Value| v.as_str().unwrap_or_default().to_string();
    let (site_files, site_sources) = (list("files"), list("sources"));

    // Check the names and shapes the site sent before comparing or writing anything.
    for f in &site_files {
        let name = text(&f["name"]);
        ensure!(
            crate::valid_site_name(&name),
            "the site sent an unsafe file name {name:?}; nothing was written"
        );
        ensure!(
            f["source"].is_string(),
            "the site's file {name:?} has no source; nothing was written"
        );
    }
    let names: Vec<String> = site_files.iter().map(|f| text(&f["name"])).collect();
    if let Some(name) = crate::case_twin(names.iter().map(String::as_str)) {
        bail!("the site sent two files whose names differ only in case ({name:?}); nothing was written");
    }
    for s in &site_sources {
        let name = text(&s["name"]);
        ensure!(
            !name.contains(['\n', '\r']),
            "the site sent a source name with a line break {name:?}; nothing was written"
        );
    }
    let target_hz = &site["targetHz"];
    ensure!(
        target_hz.is_null() || target_hz.as_u64().is_some_and(|v| u8::try_from(v).is_ok()),
        "the site's targetHz {target_hz} is not a whole number from 0 to 255; nothing was written"
    );

    // Compare's three inputs. Everything from here to compare prepares them; it decides.
    let mut site_hashes = hashes(site);
    // A file only the site lists, whose path already holds other bytes in the folder (an agent's
    // file not yet in ppu.json), meets compare as a folder entry, so it clashes. Bytes equal to
    // the site's copy are no clash: the file is taken and listed.
    let mut folder_hashes = hashes(folder);
    for f in &site_files {
        let name = text(&f["name"]);
        let key = format!("file {name}");
        if !link.hashes.contains_key(&key) && !folder_hashes.contains_key(&key) {
            if let Ok(bytes) = std::fs::read(dir.join(&name)) {
                let hash = fnv(&[&bytes]);
                if site_hashes.get(&key) != Some(&hash) {
                    folder_hashes.insert(key, hash);
                }
            }
        }
    }
    // The folder's files as compare judged them: each write re-checks its path against these.
    let disk = folder_hashes.clone();
    // A save whose answer was lost is the folder's own: a site holding exactly it is the base.
    let own = !link.pending.is_empty() && link.pending == site_hashes;
    let mut base = if own {
        link.pending.clone()
    } else {
        link.hashes.clone()
    };
    // A link written before manifest fields were hashed has no `meta` base: the folder's value is
    // the base, so the site's change is taken rather than reported as a clash.
    if !base.keys().any(|k| k.starts_with("meta ")) {
        for (k, v) in folder_hashes.iter().filter(|(k, _)| k.starts_with("meta ")) {
            base.insert(k.clone(), v.clone());
        }
    }
    // Line endings alone are no edit (a Windows editor's CRLF): a folder file equal to the base
    // or the site once read with LF counts as that copy. For judging only; saves send the bytes.
    for f in folder["files"].as_array().into_iter().flatten() {
        let source = text(&f["source"]);
        if source.contains("\r\n") {
            let key = format!("file {}", text(&f["name"]));
            let lf = fnv(&[source.replace("\r\n", "\n").as_bytes()]);
            if base.get(&key) == Some(&lf) || site_hashes.get(&key) == Some(&lf) {
                folder_hashes.insert(key, lf);
            }
        }
    }
    // The file order, judged over the names all three hold, so adding or dropping a file on one
    // side never reads as a reorder. A link from before the order was kept: the folder's is the base.
    let (folder_order, site_order) = (file_order(folder), file_order(site));
    // A lost save's answer: the site holds the folder's own sent order, which is the base.
    let base_order = match (&link.order, own) {
        (_, true) => site_order.clone(),
        (o, false) if !o.is_empty() => o.clone(),
        _ => folder_order.clone(),
    };
    let shared =
        |n: &String| base_order.contains(n) && folder_order.contains(n) && site_order.contains(n);
    let relative = |order: &[String]| {
        let names: Vec<&[u8]> = order
            .iter()
            .filter(|n| shared(n))
            .map(|n| n.as_bytes())
            .collect();
        fnv(&names)
    };
    base.insert(ORDER.into(), relative(&base_order));
    folder_hashes.insert(ORDER.into(), relative(&folder_order));
    site_hashes.insert(ORDER.into(), relative(&site_order));

    let Compared {
        mut take,
        clashes: both,
    } = compare(&base, &folder_hashes, &site_hashes);
    let theirs = settle(&both, revision, clashes, &mut *out)?;
    take.extend(theirs.iter().cloned());

    let mut payloads = BTreeMap::new();
    for s in &site_sources {
        let name = text(&s["name"]);
        if take.contains(&format!("source {name}")) {
            let payload = s["payload"].as_str().with_context(|| {
                format!("the site's source {name:?} has no payload; nothing was written")
            })?;
            let bytes = BASE64
                .decode(payload)
                .with_context(|| format!("the site's source {name:?} has an invalid payload"))?;
            ensure!(
                s["kind"].is_string(),
                "the site's source {name:?} has no kind; nothing was written"
            );
            payloads.insert(name, bytes);
        }
    }

    // The manifest is rebuilt from only if it still holds the bytes compare judged.
    let manifest_hash = packed.manifest.as_ref().context("cannot read ppu.json")?;
    unchanged(dir, crate::MANIFEST, Some(manifest_hash), None)?;
    let mut manifest = crate::read_manifest(dir)?;

    // The folder's copy of each clash the site wins, before anything replaces it.
    let backup = format!("{BACKUP_DIR}/{revision}");
    let mut notes = Vec::new();
    for key in &theirs {
        let kept = match key.split_once(' ') {
            Some(("file", name)) => {
                unchanged(dir, name, disk.get(key), None)?;
                back_up(dir, &backup, name)?
            }
            // A source, the order or a manifest field lives in ppu.json (and the source's files).
            _ => {
                let source = key.strip_prefix("source ");
                for s in manifest
                    .sources
                    .iter()
                    .filter(|s| Some(s.name.as_str()) == source)
                {
                    for path in [&s.payload, &s.file, &s.priority_file]
                        .into_iter()
                        .flatten()
                    {
                        back_up(dir, &backup, &path.replace('\\', "/"))?;
                    }
                }
                back_up(dir, &backup, crate::MANIFEST)?
            }
        };
        if kept {
            notes.push(format!(
                "Kept this folder's {} in {backup}/.",
                describe(key)
            ));
        }
    }

    let folder_files: Vec<String> = manifest
        .files
        .iter()
        .map(|n| n.replace('\\', "/"))
        .collect();
    let file_taken = |n: &str| take.contains(&format!("file {n}"));
    let site_names: Vec<String> = site_files.iter().map(|f| text(&f["name"])).collect();
    let mut removals = Vec::new();
    for name in folder_files
        .iter()
        .filter(|n| file_taken(n) && !site_names.contains(n))
        // A rename that changes only case: on macOS and Windows the old path IS the new file.
        // shortcut: on Linux the old-case file stays behind, unlisted; harmless, remove by hand.
        .filter(|n| {
            !site_names
                .iter()
                .any(|s| s.to_lowercase() == n.to_lowercase())
        })
    {
        ensure!(
            crate::valid_file_name(name),
            "unsafe file name {name:?} in ppu.json"
        );
        unchanged(dir, name, disk.get(&format!("file {name}")), None)?;
        removals.push(name.clone());
    }
    for f in &site_files {
        let name = text(&f["name"]);
        if file_taken(&name) {
            let key = format!("file {name}");
            unchanged(dir, &name, disk.get(&key), site_hashes.get(&key))?;
            let path = dir.join(&name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("cannot create {}", parent.display()))?;
            }
            std::fs::write(&path, text(&f["source"]))
                .with_context(|| format!("cannot write {name}"))?;
        }
    }
    manifest.files = arrange(&site_names, &folder_files, file_taken, take.contains(ORDER));

    let source_taken = |n: &str| take.contains(&format!("source {n}"));
    let site_source_names: Vec<String> = site_sources.iter().map(|s| text(&s["name"])).collect();
    // Taken art and palettes land as the artist's files (a PNG already holding the source is
    // rendered over in place), or stay raw payloads with a note.
    let cands: Vec<Cand> = site_sources
        .iter()
        .filter(|s| source_taken(&text(&s["name"])) && lands_as_file(&text(&s["kind"])))
        .map(|s| {
            let (name, kind) = (text(&s["name"]), text(&s["kind"]));
            let at = (manifest.sources.iter().find(|o| o.name == name))
                .filter(|o| o.kind == kind && o.priority_file.is_none())
                .and_then(|o| o.file.as_ref())
                .map(|f| f.replace('\\', "/"))
                .filter(|f| crate::valid_file_name(f));
            let entry = bare_entry(&name, &kind, s["options"].clone(), s["meta"].clone());
            Cand {
                entry,
                payload: payloads[&name].clone(),
                at,
            }
        })
        .collect();
    let mut landed = land(dir, &manifest.sources, cands);
    let mut synced = Vec::new();
    let folder_order: Vec<String> = manifest.sources.iter().map(|s| s.name.clone()).collect();
    let mut folder_sources: BTreeMap<String, crate::ManifestSource> =
        std::mem::take(&mut manifest.sources)
            .into_iter()
            .map(|s| (s.name.clone(), s))
            .collect();
    let order = arrange(&site_source_names, &folder_order, source_taken, true);
    // A taken source leaves its PNGs behind: they must still be the bytes compare judged.
    for s in folder_sources.values().filter(|s| source_taken(&s.name)) {
        for path in [&s.file, &s.priority_file].into_iter().flatten() {
            let path = path.replace('\\', "/");
            unchanged(dir, &path, packed.assets.get(&path), None)?;
        }
    }
    // Payloads the folder keeps using, and every name a new payload must not take: a changed
    // source gets a fresh `.bin`, so a crash before the manifest never mixes old and new.
    let used: BTreeSet<String> = folder_sources
        .values()
        .filter(|s| !source_taken(&s.name))
        .filter_map(|s| s.payload.clone())
        .collect();
    let mut avoid: BTreeSet<String> = folder_sources
        .values()
        .filter_map(|s| s.payload.clone())
        .collect();
    for name in &order {
        if !source_taken(name) {
            manifest.sources.extend(folder_sources.remove(name));
            continue;
        }
        let s = &site_sources[site_source_names
            .iter()
            .position(|n| n == name)
            .expect("taken from the site")];
        // A WAV cannot be written back from BRR, and a song file is the person's text: the
        // folder's stays while it converts to the site's.
        if let Some(own) = folder_sources.remove(name) {
            let same = matches!(own.kind.as_str(), "sample" | "song")
                && own.file.is_some()
                && crate::author::source_bytes(dir, &own).is_ok_and(|(b, _)| b == payloads[name]);
            if same {
                manifest.sources.push(own);
                continue;
            }
            folder_sources.insert(name.clone(), own);
        }
        let why = match landed.by_name.remove(name) {
            Some(Landing::File(p)) => {
                // The folder's own copy is kept before the site's lands over it.
                if std::fs::read(dir.join(&p.path)).is_ok_and(|old| old != p.bytes)
                    && back_up(dir, &backup, &p.path)?
                {
                    notes.push(format!(
                        "{} replaced by the site's version; the old copy is in {backup}/{}.",
                        p.path, p.path
                    ));
                }
                put(dir, &p.path, &p.bytes)?;
                if p.entry.kind == "palette" {
                    synced.push(p.path.clone());
                }
                manifest.sources.push(p.entry);
                continue;
            }
            Some(Landing::Raw(why)) => why,
            None => String::new(),
        };
        if let Some(png) = folder_sources.get(name).and_then(|s| s.file.as_ref()) {
            let raw = if why.is_empty() {
                String::new()
            } else {
                format!(" It stays raw data: {why}.")
            };
            notes.push(format!(
                "{name} now comes from the site; {png} is no longer used.{raw}"
            ));
        } else if !why.is_empty() {
            notes.push(format!("{name} stays raw data: {why}."));
        }
        let path = (0..)
            .map(|i| format!(".ppu/sources/{i}.bin"))
            .find(|p| !avoid.contains(p) && !dir.join(p).exists())
            .expect("a free payload name");
        avoid.insert(path.clone());
        std::fs::create_dir_all(dir.join(".ppu/sources"))?;
        std::fs::write(dir.join(&path), &payloads[name])
            .with_context(|| format!("cannot write {path}"))?;
        manifest.sources.push(crate::ManifestSource {
            payload: Some(path),
            ..bare_entry(
                name,
                &text(&s["kind"]),
                s["options"].clone(),
                s["meta"].clone(),
            )
        });
    }
    for p in &landed.created {
        put(dir, &p.path, &p.bytes)?;
        manifest.sources.push(p.entry.clone());
    }
    notes.extend(announcements(&landed));
    // Taken sources (changed or dropped) lose the old payload file when connect owns it; a PNG stays.
    let mut stale = Vec::new();
    for old in folder_sources.values() {
        if let Some(path) = own_payload(old).filter(|p| !used.contains(p)) {
            stale.push(path);
        }
    }

    let field = |k: &str| take.contains(&format!("meta {k}"));
    if field("title") {
        manifest.title = text(&site["title"]);
    }
    if field("description") {
        manifest.description = text(&site["description"]);
    }
    if field("targetHz") {
        manifest.target_hz = site["targetHz"].as_u64().and_then(|v| u8::try_from(v).ok());
    }
    if field("romSize") {
        manifest.rom_size = site.get("romSize").filter(|v| !v.is_null()).cloned();
    }
    let temp = write_temp(
        dir,
        crate::MANIFEST,
        format!("{}\n", serde_json::to_string_pretty(&manifest)?).as_bytes(),
    )?;
    if let Err(e) = unchanged(dir, crate::MANIFEST, Some(manifest_hash), None) {
        let _ = std::fs::remove_file(&temp);
        return Err(e);
    }
    std::fs::rename(&temp, dir.join(crate::MANIFEST)).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        anyhow::Error::new(e).context("cannot write ppu.json")
    })?;

    let next = adopt(dir, link, revision, site)?;
    // A palette the site changed: the PLTE of every PNG naming it follows, so the folder still
    // packs. Each PNG is kept first; a failure leaves PLTE != .gpl, which a sync then refuses.
    for gpl in synced {
        let kept = &mut |rel: &str| {
            notes.push(format!(
                "{rel} now follows {gpl}; the old copy is in {backup}/{rel}."
            ))
        };
        if let Err(e) = follow_palette(dir, &gpl, &backup, &packed.assets, kept) {
            notes.push(format!(
                "The art naming {gpl} was not updated to it ({e:#}); ppu art palette sync {gpl} retries."
            ));
        }
    }
    for name in removals {
        // An edit landing after the last check keeps its file, now unlisted, rather than losing it.
        if unchanged(dir, &name, disk.get(&format!("file {name}")), None).is_err() {
            notes.push(format!(
                "{name} changed in this folder just as the site dropped it: kept, no longer in ppu.json."
            ));
            continue;
        }
        remove(&dir.join(name))?;
    }
    for path in stale {
        remove(&dir.join(path))?;
    }
    for note in notes {
        writeln!(out, "{note}")?;
    }
    Ok(next)
}

/// Rewrite the PLTE of every PNG naming the palette file `gpl` (`ppu art palette sync`), keeping
/// each PNG in `backup` first. A PNG the folder moved since `assets` was read stops the sync
/// before it is touched; `kept` hears each PNG's path just before its rewrite lands.
fn follow_palette(
    dir: &Path,
    gpl: &str,
    backup: &str,
    assets: &BTreeMap<String, String>,
    kept: &mut dyn FnMut(&str),
) -> Result<()> {
    let before = &mut |png: &Path| -> Result<()> {
        let rel = png
            .strip_prefix(dir)
            .unwrap_or(png)
            .to_string_lossy()
            .replace('\\', "/");
        if let Some(hash) = assets.get(&rel) {
            unchanged(dir, &rel, Some(hash), None)?;
        }
        // The first copy in a revision wins: it may be the one a site-wins clash kept.
        if !dir.join(backup).join(&rel).exists() {
            back_up(dir, backup, &rel)?;
        }
        FOLLOWED.with(|f| {
            f.borrow_mut()
                .push((rel.clone(), gpl.to_string(), backup.to_string()))
        });
        kept(&rel);
        Ok(())
    };
    crate::art::palette_sync_with(&dir.join(gpl), before).map(drop)
}

/// A source a folder holds as a file: an image (bg, sheet, obj, m7), a palette or a song.
fn lands_as_file(kind: &str) -> bool {
    matches!(kind, "palette" | "song") || is_image(kind)
}

/// An image source (bg, sheet, obj, m7): one a PNG holds.
fn is_image(kind: &str) -> bool {
    kind.parse::<SourceKind>()
        .is_ok_and(|k| row_size_for(k, 4).is_some())
}

/// A payload this connect owns: a `.bin` under `.ppu/sources/`, never the person's own file.
fn own_payload(s: &crate::ManifestSource) -> Option<String> {
    (s.payload.clone()).filter(|p| s.file.is_none() && p.starts_with(".ppu/sources/"))
}

/// A source to land in the folder: what the site (or an old `.bin`) holds of it.
struct Cand {
    /// Name, kind, options and meta; no payload or file yet.
    entry: crate::ManifestSource,
    payload: Vec<u8>,
    /// The file the source already lives in: rendered over in place.
    at: Option<String>,
}

/// A source written as a file: the manifest entry that lists it, and the file.
struct Placed {
    entry: crate::ManifestSource,
    path: String,
    bytes: Vec<u8>,
}

enum Landing {
    File(Placed),
    /// Stays a raw payload, and why (the person is told).
    Raw(String),
}

struct Landed {
    by_name: BTreeMap<String, Landing>,
    /// Palettes made for art that named none (or whose named palette was not its colours), in the
    /// order made.
    created: Vec<Placed>,
    /// One line per art that left the palette it named for its own colours or was pointed at a
    /// palette the folder holds, and per source whose meta the next sync refreshes.
    notes: Vec<String>,
}

/// `<folder>/<name>.<ext>` when `name` can be a plain file name on every system.
fn art_path(folder: &str, name: &str, ext: &str) -> Option<String> {
    let path = format!("{folder}/{name}.{ext}");
    (!name.is_empty() && !name.contains('/') && crate::valid_site_name(&path)).then_some(path)
}

/// Where a new file goes: `at`, or `wanted` (where a tileset looks for it), or
/// `<folder>/<name>.<ext>`, unless another file holds that name (bytes already identical there
/// are fine: an interrupted conversion run again).
#[allow(clippy::too_many_arguments)]
fn claim(
    dir: &Path,
    claimed: &mut BTreeSet<String>,
    at: Option<&str>,
    wanted: Option<&str>,
    folder: &str,
    name: &str,
    ext: &str,
    bytes: &[u8],
) -> Result<String, String> {
    let path = match at {
        Some(at) => at.to_string(),
        None => {
            let taken = |path: &str| {
                let on_disk = dir.join(path);
                claimed.contains(&path.to_lowercase())
                    || on_disk.exists() && std::fs::read(&on_disk).map_or(true, |b| b != bytes)
            };
            match wanted.filter(|w| !taken(w)) {
                Some(w) => w.to_string(),
                None => {
                    let path = art_path(folder, name, ext)
                        .ok_or_else(|| format!("its name cannot be a file name ({name:?})"))?;
                    if taken(&path) {
                        return Err(format!("{path} is already another file"));
                    }
                    path
                }
            }
        }
    };
    claimed.insert(path.to_lowercase());
    Ok(path)
}

/// The PNG path each tileset in the folder (a `.tsj`, or one inline in a `.tmj`) names, by its
/// file stem: the engine binds a tileset to the source of that name, and Tiled opens the path, so
/// art landing there shows in Tiled. Paths outside the folder or not plain file names are skipped.
fn tileset_images(dir: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let path = e.path();
            let Some(fname) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if fname.starts_with('.') || fname == "node_modules" {
                continue;
            }
            if path.is_dir() {
                todo.push(path);
                continue;
            }
            let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("");
            if !matches!(ext, "tsj" | "tmj") {
                continue;
            }
            let Ok(json) = std::fs::read(&path)
                .map_err(drop)
                .and_then(|b| serde_json::from_slice::<Value>(&b).map_err(drop))
            else {
                continue;
            };
            let images: Vec<&str> = match ext {
                "tsj" => json["image"].as_str().into_iter().collect(),
                _ => (json["tilesets"].as_array().into_iter().flatten())
                    .filter_map(|t| t["image"].as_str())
                    .collect(),
            };
            let Ok(rel_dir) = d.strip_prefix(dir) else {
                continue;
            };
            for image in images {
                // Resolve against the tileset's folder, folding `..`; refuse escaping the folder.
                let mut parts: Vec<String> = rel_dir
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect();
                let mut ok = true;
                for part in image.replace('\\', "/").split('/') {
                    match part {
                        "" | "." => {}
                        ".." => ok &= parts.pop().is_some(),
                        p => parts.push(p.to_string()),
                    }
                }
                let joined = parts.join("/");
                let stem = joined.rsplit('/').next().unwrap_or("");
                let Some(stem) = stem.strip_suffix(".png") else {
                    continue;
                };
                if ok && crate::valid_site_name(&joined) {
                    out.entry(stem.to_string()).or_insert(joined);
                }
            }
        }
    }
    out
}

/// Turn site payloads into folder files: the one place a payload becomes a PNG, `.gpl` or
/// `.song.json`.
/// `existing` is the folder's manifest (its palette sources are the ones art can name). Art
/// whose `options.palette` names a palette that reproduces its colours uses it; any other art
/// (none named, or the named one is unknown or not its colours) gets one: a palette the folder
/// already holds with the same rows, else a new `<name>_pal` (so sources whose whole palette lists
/// are identical share one file). A payload lands as a file only when the file imports back to
/// exactly the payload (what a sync would send), else it is `Raw`.
fn land(dir: &Path, existing: &[crate::ManifestSource], cands: Vec<Cand>) -> Landed {
    let batch: BTreeSet<&str> = cands.iter().map(|c| c.entry.name.as_str()).collect();
    let mut names: BTreeSet<String> = (existing.iter().map(|s| s.name.clone()))
        .chain(batch.iter().map(|n| n.to_string()))
        .collect();
    let mut claimed: BTreeSet<String> = existing
        .iter()
        .flat_map(|s| [&s.file, &s.priority_file, &s.payload])
        .flatten()
        .map(|p| p.replace('\\', "/").to_lowercase())
        .collect();
    // Every palette art can name: the folder's (read through ppu art) and this batch's.
    let mut known: Vec<(String, PaletteSource)> = existing
        .iter()
        .filter(|s| s.kind == "palette" && !batch.contains(s.name.as_str()))
        .filter_map(|s| {
            let (pal, _) = crate::art::load_palette(dir, existing, &s.name).ok()?;
            Some((s.name.clone(), pal))
        })
        .collect();
    // Every palette name the folder lists, landed or not.
    let palettes: BTreeSet<&str> = (existing.iter().map(|s| (&s.name, &s.kind)))
        .chain(cands.iter().map(|c| (&c.entry.name, &c.entry.kind)))
        .filter(|(_, k)| *k == "palette")
        .map(|(n, _)| n.as_str())
        .collect();
    let wanted = tileset_images(dir);
    let mut by_name = BTreeMap::new();
    let mut created = Vec::new();
    let mut notes = Vec::new();
    // What a sync sends for a file is the meta its import makes: name a source whose stored meta
    // (an older importer's report) the next sync will change.
    let refresh = |notes: &mut Vec<String>, name: &str, stored: &Value, fresh: &SourceMeta| {
        if serde_json::to_value(fresh).map(|v| v.to_string()).ok() != Some(stored.to_string()) {
            notes.push(format!(
                "{name}: the next sync refreshes its meta, which differs from what its file imports as now."
            ));
        }
    };

    for c in cands.iter().filter(|c| c.entry.kind == "palette") {
        let name = c.entry.name.clone();
        let landing = (|| -> Result<Placed, String> {
            let pal = match SourcePayload::decode(&c.payload) {
                Ok(SourcePayload::Palette(p)) => p,
                Ok(_) => return Err("its payload is not a palette".into()),
                Err(e) => return Err(format!("its payload does not decode: {e}")),
            };
            let text = gpl::write(&pal, &name);
            let (_, fresh) = gpl::convert_palette(&text)
                .ok()
                .filter(|(p, _)| p.encode() == c.payload)
                .ok_or("its .gpl would not import back to the same palette")?;
            let bytes = text.into_bytes();
            let path = claim(
                dir,
                &mut claimed,
                c.at.as_deref(),
                None,
                "palettes",
                &name,
                "gpl",
                &bytes,
            )?;
            known.push((name.clone(), pal));
            refresh(&mut notes, &name, &c.entry.meta, &fresh);
            Ok(Placed {
                entry: file_entry(&c.entry, &path, c.entry.options.clone()),
                path,
                bytes,
            })
        })();
        by_name.insert(name, landing.map_or_else(Landing::Raw, Landing::File));
    }

    for c in cands.iter().filter(|c| c.entry.kind == "song") {
        let name = c.entry.name.clone();
        let landing = (|| -> Result<Placed, String> {
            let psng = match SourcePayload::decode(&c.payload) {
                Ok(SourcePayload::Song(b)) => b,
                Ok(_) => return Err("its payload is not a song".into()),
                Err(e) => return Err(format!("its payload does not decode: {e}")),
            };
            let song = ppu_core::song::decode(&psng)
                .map_err(|e| format!("its song does not decode: {e}"))?;
            let text = ppu_core::song::to_json(&song);
            ppu_core::song::from_json(&text)
                .ok()
                .filter(|back| {
                    SourcePayload::Song(ppu_core::song::encode(back)).encode() == c.payload
                })
                .ok_or("its .song.json would not import back to the same bytes")?;
            let bytes = text.into_bytes();
            // Only a .song.json is written over: another song file (a .mid) is the person's,
            // so the site's song lands beside it as <name>.song.json and the manifest moves there.
            let at = c.at.as_deref().filter(|a| a.ends_with(".song.json"));
            let beside = (c.at.as_deref())
                .filter(|_| at.is_none())
                .and_then(|a| a.rsplit_once('.'))
                .map(|(stem, _)| format!("{stem}.song.json"));
            let path = claim(
                dir,
                &mut claimed,
                at,
                beside.as_deref(),
                "music",
                &name,
                "song.json",
                &bytes,
            )?;
            Ok(Placed {
                entry: file_entry(&c.entry, &path, c.entry.options.clone()),
                path,
                bytes,
            })
        })();
        by_name.insert(name, landing.map_or_else(Landing::Raw, Landing::File));
    }

    for c in cands.iter().filter(|c| is_image(&c.entry.kind)) {
        let name = c.entry.name.clone();
        let landing = (|| -> Result<Placed, String> {
            let kind: SourceKind = c.entry.kind.parse()?;
            let payload = SourcePayload::decode(&c.payload)
                .map_err(|e| format!("its payload does not decode: {e}"))?;
            let meta: SourceMeta = serde_json::from_value(c.entry.meta.clone())
                .map_err(|e| format!("its meta is not usable: {e}"))?;
            let options: ConvertOptions = serde_json::from_value(c.entry.options.clone())
                .map_err(|e| format!("its options are not usable: {e}"))?;
            let named = crate::art::palette_option(&c.entry).map_err(|e| format!("{e:#}"))?;
            // The PNG with `pal` and the meta it imports as, only if it imports back to the
            // site's payload.
            let png_with = |pal: &PaletteSource| -> Result<(Vec<u8>, SourceMeta), String> {
                let png = render_png(&payload, &meta, Some(pal))?;
                let (back, fresh) = convert_png_source(kind, &options, &png, None)
                    .map_err(|e| format!("its PNG would not import back: {e}"))?;
                if back.encode() != c.payload {
                    return Err("its PNG would not import back to the site's payload".into());
                }
                Ok((png, fresh))
            };
            // The named palette when it is the art's colours, else the art's own.
            let from_named = named
                .as_ref()
                .and_then(|n| known.iter().find(|(k, _)| k == n))
                .and_then(|(n, p)| Some((n.clone(), p.clone(), png_with(p).ok()?)));
            let (pal, palette_name, (png, fresh)) = match from_named {
                Some((n, p, png)) => (p, Some(n), png),
                None => {
                    let own = own_palette(&payload)?;
                    let png = png_with(&own)?;
                    let same = known.iter().find(|(_, k)| *k == own);
                    let reused = same.map(|(n, _)| n.clone());
                    (own, reused, png)
                }
            };
            let path = claim(
                dir,
                &mut claimed,
                c.at.as_deref(),
                wanted.get(&name).map(String::as_str),
                "assets",
                &name,
                "png",
                &png,
            )?;
            let palette_name = match palette_name {
                Some(n) => n,
                None => {
                    // The first name no source holds and no other file sits at.
                    let (n, gpl_path, bytes) = (1..1000)
                        .find_map(|i| {
                            let n = match i {
                                1 => format!("{name}_pal"),
                                i => format!("{name}_pal{i}"),
                            };
                            if names.contains(&n) {
                                return None;
                            }
                            let bytes = gpl::write(&pal, &n).into_bytes();
                            let at =
                                claim(dir, &mut claimed, None, None, "palettes", &n, "gpl", &bytes);
                            at.ok().map(|at| (n, at, bytes))
                        })
                        .ok_or_else(|| format!("no free palette name for {name}"))?;
                    let (_, meta) =
                        gpl::convert_palette(std::str::from_utf8(&bytes).expect("text"))?;
                    let meta = serde_json::to_value(meta).map_err(|e| e.to_string())?;
                    names.insert(n.clone());
                    known.push((n.clone(), pal));
                    created.push(Placed {
                        entry: crate::ManifestSource {
                            file: Some(gpl_path.clone()),
                            ..bare_entry(&n, "palette", serde_json::json!({}), meta)
                        },
                        path: gpl_path,
                        bytes,
                    });
                    n
                }
            };
            let mut options = c.entry.options.clone();
            if !options.is_object() {
                options = serde_json::json!({});
            }
            if let Some(old) = named.as_ref().filter(|old| **old != palette_name) {
                notes.push(if palettes.contains(old.as_str()) {
                    format!("{name}'s colours are no longer {old}'s: it now uses {palette_name}.")
                } else {
                    format!(
                        "{name} names {old}, which the folder holds no palette of: it now uses {palette_name}."
                    )
                });
            }
            // A palette this batch made is named in the "created" line instead.
            let made = created
                .iter()
                .any(|p: &Placed| p.entry.name == palette_name);
            if named.is_none() && !made {
                notes.push(format!(
                    "{name} uses {palette_name}, the folder's palette with its colours."
                ));
            }
            refresh(&mut notes, &name, &c.entry.meta, &fresh);
            options["palette"] = palette_name.into();
            Ok(Placed {
                entry: file_entry(&c.entry, &path, options),
                path,
                bytes: png,
            })
        })();
        by_name.insert(name, landing.map_or_else(Landing::Raw, Landing::File));
    }
    Landed {
        by_name,
        created,
        notes,
    }
}

/// A source entry that names no file or payload yet.
fn bare_entry(name: &str, kind: &str, options: Value, meta: Value) -> crate::ManifestSource {
    crate::ManifestSource {
        name: name.to_string(),
        kind: kind.to_string(),
        options,
        meta,
        payload: None,
        file: None,
        priority_file: None,
        extra: Default::default(),
    }
}

/// `entry` listed as the file `path`.
fn file_entry(entry: &crate::ManifestSource, path: &str, options: Value) -> crate::ManifestSource {
    crate::ManifestSource {
        file: Some(path.to_string()),
        ..bare_entry(&entry.name, &entry.kind, options, entry.meta.clone())
    }
}

/// `bytes` at `rel` in the folder, by rename: a crash leaves the old file or the new one.
fn put(dir: &Path, rel: &str, bytes: &[u8]) -> Result<()> {
    let to = dir.join(rel);
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    let temp = write_temp(dir, "art", bytes)?;
    std::fs::rename(&temp, &to).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        anyhow::Error::new(e).context(format!("cannot write {rel}"))
    })
}

/// What landing tells the person: its notes (palette switches and reuses, meta refreshes), then
/// "created 2 palettes:
/// a_pal, b_pal; nothing uses them until dma names them".
fn announcements(landed: &Landed) -> Vec<String> {
    let names: Vec<&str> = landed
        .created
        .iter()
        .map(|p| p.entry.name.as_str())
        .collect();
    let mut lines = landed.notes.clone();
    if !names.is_empty() {
        lines.push(format!(
            "created {} palette{}: {}; nothing uses them until dma names them.",
            names.len(),
            if names.len() == 1 { "" } else { "s" },
            names.join(", ")
        ));
    }
    lines
}

/// Convert every image and palette source the folder holds as a `.ppu/sources/*.bin` payload
/// to a PNG or `.gpl` file ([`land`]). A pull's own unpacked payloads are dropped (`backup` None:
/// the site holds them); an old folder's go to `backup` first. What cannot land stays a payload,
/// with a note. New files are written first and the manifest by rename after them, so a crash
/// leaves files nothing lists and the same run repeats; the link never moves.
fn convert_bins(dir: &Path, backup: Option<&str>, out: &mut dyn Write) -> Result<()> {
    let manifest_hash = std::fs::read(dir.join(crate::MANIFEST))
        .ok()
        .map(|b| fnv(&[&b]));
    let mut manifest = crate::read_manifest(dir)?;
    let mut cands = Vec::new();
    for s in manifest.sources.iter().filter(|s| lands_as_file(&s.kind)) {
        if let Some(path) = own_payload(s) {
            let payload = std::fs::read(dir.join(&path))
                .with_context(|| format!("cannot read {path} (source {})", s.name))?;
            let entry = bare_entry(&s.name, &s.kind, s.options.clone(), s.meta.clone());
            cands.push(Cand {
                entry,
                payload,
                at: None,
            });
        }
    }
    if cands.is_empty() {
        return Ok(());
    }
    let mut landed = land(dir, &manifest.sources, cands);
    let mut notes = Vec::new();
    let mut moved = Vec::new();
    for s in &mut manifest.sources {
        match landed.by_name.remove(&s.name) {
            Some(Landing::File(p)) => {
                put(dir, &p.path, &p.bytes)?;
                moved.extend(own_payload(s));
                *s = p.entry;
            }
            Some(Landing::Raw(why)) => notes.push(format!(
                "{} stays raw data in {}: {why}.",
                s.name,
                s.payload.as_deref().unwrap_or(".ppu/sources")
            )),
            None => {}
        }
    }
    for p in &landed.created {
        put(dir, &p.path, &p.bytes)?;
    }
    if moved.is_empty() && landed.created.is_empty() {
        // Connect start (`backup` set) runs this every time; a source that stays raw is named
        // by the pull or the start that converted its neighbours, not again on each start.
        // shortcut: an old folder whose art all stays raw never hears why at start; say it once
        // per link revision if that folder turns up.
        if backup.is_none() {
            for note in notes {
                writeln!(out, "{note}")?;
            }
        }
        return Ok(());
    }
    manifest
        .sources
        .extend(landed.created.iter().map(|p| p.entry.clone()));
    let temp = write_temp(
        dir,
        crate::MANIFEST,
        format!("{}\n", serde_json::to_string_pretty(&manifest)?).as_bytes(),
    )?;
    if let Err(e) = unchanged(dir, crate::MANIFEST, manifest_hash.as_ref(), None) {
        let _ = std::fs::remove_file(&temp);
        return Err(e);
    }
    std::fs::rename(&temp, dir.join(crate::MANIFEST)).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        anyhow::Error::new(e).context("cannot write ppu.json")
    })?;
    for path in moved {
        // The payload is the site's own (or, in an old folder, kept in the backup first).
        if let Some(backup) = backup {
            back_up(dir, backup, &path)?;
        }
        remove(&dir.join(path))?;
    }
    for note in notes {
        writeln!(out, "{note}")?;
    }
    for line in announcements(&landed) {
        writeln!(out, "{line}")?;
    }
    Ok(())
}

/// Move each image source's PNG to where a tileset in the folder looks for it (a folder connected
/// before art landed there kept `art/<name>.png`, which Tiled cannot find), when nothing sits at
/// that path yet. Only `ppu.json`'s `file` changes: the site never stores where a PNG lives. The
/// new copy and the manifest are written before the old file goes, so a crash leaves a spare
/// file, never a source without one.
fn follow_tilesets(dir: &Path, out: &mut dyn Write) -> Result<()> {
    let wanted = tileset_images(dir);
    if wanted.is_empty() {
        return Ok(());
    }
    let manifest_hash = std::fs::read(dir.join(crate::MANIFEST))
        .ok()
        .map(|b| fnv(&[&b]));
    let mut manifest = crate::read_manifest(dir)?;
    let listed: BTreeSet<String> = (manifest.sources.iter())
        .flat_map(|s| [&s.file, &s.priority_file])
        .flatten()
        .map(|p| p.replace('\\', "/").to_lowercase())
        .collect();
    let mut moves = Vec::new();
    for s in &mut manifest.sources {
        if !is_image(&s.kind) {
            continue;
        }
        let (Some(from), Some(to)) = (s.file.clone(), wanted.get(&s.name)) else {
            continue;
        };
        let from = from.replace('\\', "/");
        let free = !listed.contains(&to.to_lowercase()) && !dir.join(to).exists();
        if from.eq_ignore_ascii_case(to) || !free || !crate::valid_file_name(&from) {
            continue;
        }
        let Ok(bytes) = std::fs::read(dir.join(&from)) else {
            continue;
        };
        put(dir, to, &bytes)?;
        s.file = Some(to.clone());
        moves.push((from, to.clone()));
    }
    if moves.is_empty() {
        return Ok(());
    }
    let temp = write_temp(
        dir,
        crate::MANIFEST,
        format!("{}\n", serde_json::to_string_pretty(&manifest)?).as_bytes(),
    )?;
    if let Err(e) = unchanged(dir, crate::MANIFEST, manifest_hash.as_ref(), None) {
        let _ = std::fs::remove_file(&temp);
        for (_, to) in &moves {
            let _ = std::fs::remove_file(dir.join(to));
        }
        return Err(e);
    }
    std::fs::rename(&temp, dir.join(crate::MANIFEST)).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        anyhow::Error::new(e).context("cannot write ppu.json")
    })?;
    for (from, to) in moves {
        remove(&dir.join(&from))?;
        writeln!(out, "moved {from} to {to}, where a tileset looks for it.")?;
    }
    Ok(())
}

/// Why a sync must not send this folder, by PNG: art that fails `ppu art check`'s rules (a mixed
/// row, a PLTE that is no longer its `.gpl`, a budget), as the dashboard's refused state. Import
/// alone does not compare a PLTE with its palette, so this is what keeps a drifted PNG off the site.
fn art_refusals(dir: &Path) -> Result<BTreeMap<String, Value>> {
    let manifest = crate::read_manifest(dir)?;
    let mut out = BTreeMap::new();
    for s in &manifest.sources {
        let Some(file) = s.file.as_deref() else {
            continue;
        };
        if !is_image(&s.kind) {
            continue;
        }
        let problems = crate::art::findings_of(dir, &manifest, s)?;
        let Some((rule, _)) = problems.first() else {
            continue;
        };
        // each line names the file first; the row already does
        let why = problems
            .iter()
            .map(|(_, m)| m.split_once(": ").map_or(m.as_str(), |(_, rest)| rest))
            .collect::<Vec<_>>()
            .join("; ");
        let mut state = serde_json::json!({"is": "refused", "why": why});
        // a PLTE off its palette follows it (palette sync); colours that are not SNES art convert
        let palette = s.options["palette"].as_str().and_then(|name| {
            manifest
                .sources
                .iter()
                .find(|p| p.name == name && p.kind == "palette")?
                .file
                .clone()
        });
        match (rule, palette) {
            (Rule::Plte, Some(gpl)) => {
                state["fix"] = "palette-sync".into();
                state["palette"] = gpl.replace('\\', "/").into();
            }
            (Rule::MixedRows | Rule::PaletteBudget | Rule::Import | Rule::NotSnesArt, _) => {
                state["fix"] = "convert".into()
            }
            _ => {}
        }
        out.insert(file.replace('\\', "/"), state);
    }
    Ok(out)
}

fn remove(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(e).with_context(|| format!("cannot remove {}", path.display()))
        }
        _ => Ok(()),
    }
}

enum Heard {
    /// A revision the change stream reported; the connect loop drops the ones it saved itself.
    Moved(i64),
    /// The listen failed (true) or works again (false): presence is off while it fails.
    Lost(bool),
    /// Disconnect was pressed in the Studio.
    Stop,
    /// A dashboard button: a [`Command`], not checked yet.
    Command(Value),
    Fatal(anyhow::Error),
}

/// Keep a listen open on the project as a folder client, on its own thread and never waiting on
/// the connect loop, so a slow save or write never lets presence lapse.
/// shortcut: a listen in flight when connect returns runs on until its answer (up to
/// LISTEN_TIMEOUT), since ureq 2 cannot cancel a blocked read; `ppu connect` exits at once, which
/// closes the socket and ends the listen on the site. Upgrade if a long-lived host calls connect.
fn listen(
    mut creds: Credentials,
    id: String,
    mut after: i64,
    alive: Arc<()>,
    panel: Arc<Stream>,
    tx: mpsc::Sender<Heard>,
) {
    let agent = login::agent(LISTEN_TIMEOUT);
    let mut lost = false;
    // The connect loop holds the other reference; when it ends, so does the listen.
    while Arc::strong_count(&alive) > 1 {
        let seen = panel.heard.lock().unwrap().seq;
        let path = format!(
            "/api/projects/{id}/changes?after={after}&seen={seen}&folder=true&wait={LISTEN_WAIT_SECS}"
        );
        let answer = send(&agent, &mut creds, "GET", &path, None);
        let revision = match &answer {
            Ok((200, v)) => v["revision"].as_i64(),
            _ => None,
        };
        if let Ok((200, v)) = &answer {
            let stop = v["stop"] == true;
            if let Some(seq) = v["seq"].as_u64() {
                let mut heard = panel.heard.lock().unwrap();
                heard.seq = seq;
                heard.stop |= stop;
                heard.answers = v["answers"].as_object().cloned().unwrap_or_default();
                panel.news.notify_all();
            }
            // the site sends only the commands newer than `seen`
            for c in v["commands"].as_array().into_iter().flatten() {
                if tx.send(Heard::Command(c["command"].clone())).is_err() {
                    return;
                }
            }
            if stop {
                let _ = tx.send(Heard::Stop);
                return;
            }
        }
        if let Ok((status @ (401 | 404), v)) = &answer {
            let _ = tx.send(Heard::Fatal(refused(&creds, &id, *status, v)));
            return;
        }
        if lost == revision.is_some() {
            lost = !lost;
            if tx.send(Heard::Lost(lost)).is_err() {
                return;
            }
        }
        match revision {
            Some(revision) if revision != after => {
                if tx.send(Heard::Moved(revision)).is_err() {
                    return;
                }
                after = revision;
            }
            Some(_) => {}
            // A dropped connection, a server hiccup or a page that is not the site: retry shortly.
            None => std::thread::sleep(Duration::from_secs(2)),
        }
    }
}

thread_local! {
    /// The PNGs `follow_palette` rewrote on this thread (png, palette, backup), for the dashboard.
    static FOLLOWED: RefCell<Vec<(String, String, String)>> = const { RefCell::new(Vec::new()) };
}

fn now_ms() -> u64 {
    let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
    since.map_or(0, |d| d.as_millis() as u64)
}

/// The keys of the entries that differ between two hash sets.
fn changed_keys(old: &BTreeMap<String, String>, new: &BTreeMap<String, String>) -> Vec<String> {
    let differ = new
        .iter()
        .filter(|(k, h)| old.get(*k) != Some(*h))
        .map(|(k, _)| k);
    let gone = old.keys().filter(|k| !new.contains_key(*k));
    differ.chain(gone).cloned().collect()
}

/// `dir` as the person writes it: `~/ppu/x` under their home, `%USERPROFILE%\ppu\x` on Windows.
fn shown_dir(dir: &Path) -> String {
    let full = dir.display().to_string();
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE"));
    match home
        .ok()
        .and_then(|h| full.strip_prefix(&h).map(str::to_string))
    {
        Some(rest) if rest.starts_with(['/', '\\']) => {
            format!(
                "{}{rest}",
                if cfg!(windows) { "%USERPROFILE%" } else { "~" }
            )
        }
        _ => full,
    }
}

/// What the Connect panel's sync dashboard shows: one row per folder entry with its state. Connect
/// posts the whole snapshot whenever it changes, so a tab that opens later reads it at once.
/// Clashes are not here: the panel has them as the open clash questions, by entry name.
#[derive(Default)]
struct Board {
    /// When connect started (ms since the epoch): what every entry not synced since shows.
    start: u64,
    /// When each entry (link key) last synced.
    at: BTreeMap<String, u64>,
    /// Entries a save is sending or a site revision is writing.
    moving: BTreeMap<String, &'static str>,
    /// Where a revision being written keeps the folder copies it replaces.
    backup: Option<String>,
    /// Art the last compare refused, by path: its refused state.
    refused: BTreeMap<String, Value>,
    /// PNGs rewritten to follow a palette: (palette, backup, at, the bytes' hash then). A row
    /// follows until the PNG changes again.
    follows: BTreeMap<String, (String, String, u64, String)>,
    skills: Value,
    posted: String,
    /// Pause pressed in the Studio: nothing syncs until Resume.
    paused: bool,
    /// While paused, the entries (link keys) the folder changed: sent on Resume.
    waiting: BTreeSet<String>,
}

impl Board {
    fn snapshot(&mut self, dir: &Path, revision: i64) -> Value {
        let hash = |rel: &str| std::fs::read(dir.join(rel)).ok().map(|b| fnv(&[&b]));
        let at = now_ms();
        for (png, gpl, backup) in FOLLOWED.with(|f| std::mem::take(&mut *f.borrow_mut())) {
            if let Some(h) = hash(&png) {
                self.follows.insert(png, (gpl, backup, at, h));
            }
        }
        self.follows
            .retain(|png, f| hash(png).as_ref() == Some(&f.3));
        let state = |key: &str, path: &str| -> Value {
            if let Some(how) = self.moving.get(key) {
                return serde_json::json!({ "is": how });
            }
            if self.waiting.contains(key) {
                return serde_json::json!({ "is": "waiting" });
            }
            if let Some(refused) = self.refused.get(path) {
                return refused.clone();
            }
            if let Some((gpl, backup, at, _)) = self.follows.get(path) {
                return serde_json::json!({"is": "follows", "palette": gpl, "backup": backup, "at": at});
            }
            serde_json::json!({"is": "synced", "at": self.at.get(key).copied().unwrap_or(self.start)})
        };
        let mut files = Vec::new();
        // shortcut: a ppu.json that does not parse lists no rows until it does; its "Not saved"
        // line says why.
        let manifest = crate::read_manifest(dir).ok();
        for name in manifest.iter().flat_map(|m| &m.files) {
            let kind = match name.rsplit_once('.').map(|(_, ext)| ext) {
                Some("tmj") => "map",
                Some("tsj") => "tileset",
                _ => "code",
            };
            let key = format!("file {name}");
            let entry = describe(&key);
            files.push(serde_json::json!({"path": name, "entry": entry, "kind": kind, "state": state(&key, name)}));
        }
        for s in manifest.iter().flat_map(|m| &m.sources) {
            let path = s.file.as_deref().unwrap_or(&s.name).replace('\\', "/");
            let key = format!("source {}", s.name);
            let mut row = serde_json::json!({"path": path, "entry": describe(&key), "state": state(&key, &path)});
            match s.kind.as_str() {
                art @ ("bg" | "sheet" | "obj" | "m7") => {
                    row["kind"] = "art".into();
                    row["art"] = art.into();
                    // the add-art dialog Convert opens starts from them
                    row["options"] = s.options.clone();
                    // the palette file it names: Sync PNGs to palette lists the PNGs it rewrites
                    let gpl = s.options["palette"].as_str().and_then(|name| {
                        let m = manifest.as_ref()?;
                        m.sources
                            .iter()
                            .find(|p| p.name == name && p.kind == "palette")?
                            .file
                            .clone()
                    });
                    if let Some(gpl) = gpl {
                        row["palette"] = gpl.replace('\\', "/").into();
                    }
                }
                "palette" => row["kind"] = "palette".into(),
                "sample" | "song" => row["kind"] = "sound".into(),
                _ => row["kind"] = "project".into(),
            }
            files.push(row);
        }
        // ppu.json carries the meta fields and the file order: its row is theirs
        let meta = |k: &&String| k.starts_with("meta ") || k.as_str() == ORDER;
        let mut manifest_state = serde_json::json!({"is": "synced", "at": self.at.iter().filter(|(k, _)| meta(k)).map(|(_, &t)| t).max().unwrap_or(self.start)});
        if let Some((_, how)) = self.moving.iter().find(|(k, _)| meta(k)) {
            manifest_state = serde_json::json!({ "is": how });
        }
        files.push(serde_json::json!({"path": crate::MANIFEST, "kind": "project", "state": manifest_state}));
        let version = env!("CARGO_PKG_VERSION");
        let types = match std::fs::read_to_string(dir.join(crate::LUAU_DECLARATIONS_FILE)) {
            Ok(text) if text == ppu_core::LUAU_DECLARATIONS => version,
            Ok(_) => "older",
            Err(_) => "none",
        };
        serde_json::json!({
            "dir": shown_dir(dir), "revision": revision, "files": files, "backup": self.backup,
            "skills": self.skills, "types": {"version": types, "current": version},
            "paused": self.paused,
        })
    }
}

/// What the change stream last told this folder client: the newest message's seq, the panel's
/// answers by entry, and whether Disconnect was pressed. The listen thread fills it, a clash
/// question waits on it.
#[derive(Default)]
struct Stream {
    heard: std::sync::Mutex<PanelHeard>,
    news: std::sync::Condvar,
}

#[derive(Default)]
struct PanelHeard {
    seq: u64,
    answers: serde_json::Map<String, Value>,
    stop: bool,
}

/// POST one message to the project's change stream; the seq it got, None when it did not land.
fn post(agent: &ureq::Agent, creds: &mut Credentials, id: &str, message: Value) -> Option<u64> {
    match send(
        agent,
        creds,
        "POST",
        &format!("/api/projects/{id}/changes"),
        Some(message),
    ) {
        Ok((200, v)) => v["seq"].as_u64(),
        _ => None,
    }
}

/// The panel's question about one clash: post it, then wait for the answer the listen brings.
fn panel_ask(creds: Credentials, id: String, panel: Arc<Stream>) -> Ask {
    let agent = login::agent(login::DEFAULT_READ_TIMEOUT);
    let mut creds = creds;
    Box::new(move |entry: &str, question: &str| {
        let message = serde_json::json!({"clash": {"entry": entry, "question": question}});
        // shortcut: a site that cannot take the question (down, restarted mid-wait) leaves connect
        // waiting; Ctrl+C or a Disconnect ends it. Re-post on a seq that went backwards if it bites.
        let posted = loop {
            if let Some(seq) = post(&agent, &mut creds, &id, message.clone()) {
                break seq;
            }
            if panel.heard.lock().unwrap().stop {
                return Err(Disconnected.into());
            }
            std::thread::sleep(RETRY_MIN);
        };
        let mut heard = panel.heard.lock().unwrap();
        loop {
            if heard.stop {
                return Err(Disconnected.into());
            }
            // an answer counts only from a listen that heard this question: an older one for the
            // same entry answered an earlier clash
            if heard.seq >= posted {
                match heard.answers.get(entry).and_then(Value::as_str) {
                    Some("folder") => return Ok(Keep::Folder),
                    Some("site") => return Ok(Keep::Site),
                    _ => {}
                }
            }
            heard = panel.news.wait(heard).unwrap();
        }
    })
}

/// The panel's clash questions in place of [`Clashes::Panel`], inside `--keep`'s too.
fn panelled(how: Clashes, ask: &mut dyn FnMut() -> Ask) -> Clashes {
    match how {
        Clashes::Panel => Clashes::Ask(ask()),
        Clashes::AtStart(keep, then) => Clashes::AtStart(keep, Box::new(panelled(*then, ask))),
        other => other,
    }
}

/// What connect prints, line by line, also sent to the Connect panel's log by a thread of its
/// own, so a slow site never holds up the terminal or the sync.
struct Tee<'a> {
    out: &'a mut dyn Write,
    line: Vec<u8>,
    lines: mpsc::Sender<Value>,
}

impl Write for Tee<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.out.write(buf)?;
        for &b in &buf[..n] {
            if b == b'\n' {
                let text = String::from_utf8_lossy(&std::mem::take(&mut self.line)).into_owned();
                let _ = self.lines.send(serde_json::json!({"line": {"text": text}}));
            } else {
                self.line.push(b);
            }
        }
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.out.flush()
    }
}

/// Post each message (a line, the dashboard's snapshot) to the change stream, in order; ends when
/// connect drops the senders.
fn post_lines(mut creds: Credentials, id: String, messages: mpsc::Receiver<Value>) {
    let agent = login::agent(login::DEFAULT_READ_TIMEOUT);
    for message in messages {
        // shortcut: a message the site cannot take is dropped (the terminal still has the line, the
        // next snapshot replaces a lost one); queue and resend if the log must survive outages.
        post(&agent, &mut creds, &id, message);
    }
}

/// `ppu connect` inside a linked folder: bring in what the site saved since the folder last
/// synced (asking about entries both changed), save folder edits made while disconnected, then
/// keep both in step: each settled folder change is saved, each revision saved elsewhere is
/// written into the folder. Returns Ok when Disconnect is pressed in the Studio, else only with a
/// fatal error.
pub fn connect(
    creds: &Credentials,
    dir: &Path,
    clashes: Clashes,
    out: &mut dyn Write,
) -> Result<()> {
    match run(creds, dir, clashes, out) {
        Err(e) if e.is::<Disconnected>() => {
            writeln!(out, "Disconnected in the Studio.")?;
            Ok(())
        }
        r => r,
    }
}

fn run(creds: &Credentials, dir: &Path, clashes: Clashes, out: &mut dyn Write) -> Result<()> {
    let mut link = Link::read(dir)?;
    let _running = hold(&dir.join(".ppu/connect.lock"))?.ok_or(AlreadyRunning)?;
    if link.site.is_empty() {
        // A link from before sites were recorded belongs to the site this machine is signed in to.
        link.site = creds.site.clone();
        link.write(dir)?;
    }
    ensure!(
        link.site == creds.site,
        "{} is linked to a project on {}, but this machine is signed in to {}. To connect it, sign \
         in to that site and name it to connect too: {}, then {}",
        dir.display(),
        link.site,
        creds.site,
        login::with_site(&link.site, "ppu login"),
        login::with_site(&link.site, "ppu connect"),
    );
    let mut creds = creds.clone();
    let agent = login::agent(login::DEFAULT_READ_TIMEOUT);
    let (revision, site) = fetch(&agent, &mut creds, &link.project)?;
    // Listen first: Studio tabs see the folder connected while a clash waits for the person.
    let alive = Arc::new(());
    let panel = Arc::new(Stream::default());
    let (tx, rx) = mpsc::channel();
    {
        let creds = creds.clone();
        let (id, after, alive, panel) = (
            link.project.clone(),
            link.revision,
            alive.clone(),
            panel.clone(),
        );
        std::thread::spawn(move || listen(creds, id, after, alive, panel, tx));
    }
    let (lines, posting) = mpsc::channel();
    {
        let (creds, id) = (creds.clone(), link.project.clone());
        std::thread::spawn(move || post_lines(creds, id, posting));
    }
    let board_tx = lines.clone();
    let mut tee = Tee {
        out,
        line: Vec::new(),
        lines,
    };
    let out: &mut dyn Write = &mut tee;
    let skills = crate::write_skills(dir, false)?;
    let version = env!("CARGO_PKG_VERSION");
    match skills.written.is_empty() {
        true => writeln!(out, "Skills up to date (ppu {version}).")?,
        false => writeln!(out, "Skills refreshed for ppu {version}.")?,
    }
    for name in &skills.kept {
        writeln!(
            out,
            "Kept {name}: edited since ppu wrote it (ppu skills --reset replaces it)"
        )?;
    }
    let mut clashes = panelled(clashes, &mut || {
        panel_ask(creds.clone(), link.project.clone(), panel.clone())
    });
    let folder = folder_body(dir).context("this folder does not pack")?.body;
    if revision != link.revision {
        let next = if same_content(&site, &folder) {
            // A save whose answer never arrived (Ctrl+C, a dropped connection) left the site one
            // revision on with this folder's content.
            adopt(dir, &link, revision, &site)?
        } else {
            loop {
                match bring_in(dir, &link, revision, &site, &mut clashes, out) {
                    Err(e) if e.is::<FolderMoved>() => {
                        writeln!(out, "{e}; comparing again.")?;
                        std::thread::sleep(POLL);
                    }
                    r => break r?,
                }
            }
        };
        writeln!(
            out,
            "Brought in revision {revision}: {}.",
            changed(&link.hashes, &next.hashes).join(", ")
        )?;
        link = next;
    }
    // A folder from before art was files: its image and palette payloads become PNGs and .gpl
    // files now, after any revision was brought in, so a clash is never made by the conversion.
    // The link stays: what the conversion adds (palettes, options.palette) saves with the next sync.
    convert_bins(dir, Some(&format!("{BACKUP_DIR}/{}", link.revision)), out)?;
    follow_tilesets(dir, out)?;
    // `--keep` answered the clashes found at start, only.
    let clashes = match clashes {
        Clashes::AtStart(_, then) => *then,
        live => live,
    };
    let mut sync = Sync {
        agent,
        creds,
        dir,
        link,
        site,
        clashes,
        out,
        board: Board {
            start: now_ms(),
            skills: serde_json::json!({"version": version, "kept": skills.kept}),
            ..Board::default()
        },
        board_tx,
    };
    writeln!(
        sync.out,
        "Connected {} to project {} at revision {}. Saves in this folder reach the site and the \
         site's reach this folder; Ctrl+C stops.",
        dir.display(),
        sync.link.project,
        sync.link.revision
    )?;
    sync.out.flush()?;

    // shortcut: polls the files pack reads every POLL; a file-watcher crate would wake sooner and
    // idle cheaper, upgrade if large art makes the poll costly.
    let (mut seen, mut settling) = (fingerprint(dir), false);
    // `behind`: the site has a revision to bring in. `pending`: the folder may hold unsaved edits.
    // `due`: a failed fetch or save waits for this before trying again.
    let (mut behind, mut pending, mut due, mut backoff) = (false, true, None::<Instant>, RETRY_MIN);
    loop {
        sync.publish();
        match rx.recv_timeout(POLL) {
            Ok(Heard::Moved(r)) => behind |= r != sync.link.revision,
            Ok(Heard::Lost(true)) => writeln!(
                sync.out,
                "Cannot listen on the project's changes; Studio tabs will not show this folder as connected until it can. Retrying."
            )?,
            Ok(Heard::Lost(false)) => writeln!(sync.out, "Listening on the project's changes again.")?,
            Ok(Heard::Stop) => return Err(Disconnected.into()),
            Ok(Heard::Command(c)) => {
                let was = sync.board.paused;
                sync.command(c)?;
                if was && !sync.board.paused {
                    // Resume: the site's revisions and the folder's edits made meanwhile, as usual
                    (behind, pending, due) = (true, true, None);
                }
                continue;
            }
            Ok(Heard::Fatal(e)) => return Err(e),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => bail!("the change stream listen stopped"),
        }
        sync.out.flush()?;
        let waiting = due.is_some_and(|t| Instant::now() < t);
        fn retry_in(
            due: &mut Option<Instant>,
            backoff: &mut Duration,
            out: &mut dyn Write,
            what: String,
        ) -> Result<()> {
            writeln!(out, "{what}. Retrying in {} s.", backoff.as_secs())?;
            *due = Some(Instant::now() + *backoff);
            *backoff = (*backoff * 2).min(RETRY_MAX);
            Ok(())
        }
        if sync.board.paused {
            // nothing syncs either way; the rows show the folder's edits waiting for Resume
            let now = fingerprint(dir);
            if now != seen {
                seen = now;
                sync.board.waiting = match folder_body(dir) {
                    Ok(f) => changed_keys(&sync.link.hashes, &hashes(&f.body))
                        .into_iter()
                        .collect(),
                    Err(_) => sync.board.waiting.clone(),
                };
            }
            continue;
        }
        // Only a settled folder is compared with the site: not while an edit is still landing.
        if behind && !waiting && !settling {
            match sync.catch_up() {
                Ok(()) => {
                    behind = false;
                    backoff = RETRY_MIN;
                    // The folder now holds what connect wrote: it is not an edit to save.
                    (seen, settling, pending) = (fingerprint(dir), false, true);
                }
                Err(Pull::Transient(e)) => {
                    retry_in(
                        &mut due,
                        &mut backoff,
                        &mut *sync.out,
                        format!("Cannot fetch the site's revision: {e:#}"),
                    )?;
                    continue;
                }
                Err(Pull::Unpackable(e)) => {
                    retry_in(
                        &mut due,
                        &mut backoff,
                        &mut *sync.out,
                        format!("The site's revision waits: this folder does not pack ({e:#})"),
                    )?;
                    continue;
                }
                Err(Pull::Moved(e)) => {
                    writeln!(sync.out, "{e:#}; comparing again.")?;
                    settling = true;
                    continue;
                }
                Err(Pull::Fatal(e)) => return Err(e),
            }
        }
        let now = fingerprint(dir);
        if now != seen {
            (seen, settling) = (now, true);
            continue;
        }
        let edited = std::mem::take(&mut settling);
        if !(edited || pending && !waiting) {
            continue;
        }
        (pending, due) = (false, None);
        sync.board.refused.clear(); // this compare finds them again, or the folder no longer has them
        match folder_body(dir) {
            Ok(folder) if !same_content(&sync.site, &folder.body) => {
                // A palette edited here: the PNGs naming it follow, in this same save.
                match follow_edited_palettes(
                    sync.dir,
                    &sync.site,
                    &folder,
                    sync.link.revision,
                    &mut *sync.out,
                ) {
                    Ok(false) => {}
                    // The rewrite is an edit like any: the next settled pass, which finds the
                    // PNGs following, saves it, and a .gpl saved again meanwhile is followed too.
                    Ok(true) => {
                        (settling, pending) = (true, true);
                        continue;
                    }
                    Err(e) if e.is::<FolderMoved>() => {
                        writeln!(sync.out, "{e:#}; comparing again.")?;
                        settling = true;
                        continue;
                    }
                    Err(e) => {
                        writeln!(
                            sync.out,
                            "Not saved: {e:#}. Nothing was sent; the site keeps its last good revision."
                        )?;
                        continue;
                    }
                }
                // Art must pass the rules of `ppu art check` before anything is sent.
                // shortcut: check and pack each quantize a full-colour PNG, twice per sync; reuse
                // pack's import if large truecolour sources make saves slow.
                match art_refusals(dir) {
                    Ok(refused) if refused.is_empty() => {}
                    Ok(refused) => {
                        let why: Vec<_> = refused
                            .iter()
                            .filter_map(|(path, r)| Some(format!("{path}: {}", r["why"].as_str()?)))
                            .collect();
                        writeln!(
                            sync.out,
                            "Not saved: {}. Nothing was sent; the site keeps its last good revision.",
                            why.join("; ")
                        )?;
                        sync.board.refused = refused;
                        continue;
                    }
                    Err(e) => {
                        writeln!(sync.out, "Not saved: {e:#}")?;
                        continue;
                    }
                }
                match sync.save(merge(&sync.site, &folder.body))? {
                    Saved::Done => backoff = RETRY_MIN,
                    Saved::Transient(why) => {
                        pending = true;
                        retry_in(
                            &mut due,
                            &mut backoff,
                            &mut *sync.out,
                            format!("Not saved: {why}"),
                        )?;
                    }
                    // Bring the site's revision in first, then save what the folder adds to it.
                    Saved::Behind => (behind, pending) = (true, true),
                }
            }
            Ok(_) => {}
            Err(e) => writeln!(sync.out, "Not saved: {e:#}")?,
        }
        sync.out.flush()?;
    }
}

/// The palettes whose `.gpl` the folder changed since the site's copy: the PNGs naming each follow
/// it, one line per palette. True when any PNG was rewritten.
fn follow_edited_palettes(
    dir: &Path,
    site: &Value,
    folder: &Packed,
    revision: i64,
    out: &mut dyn Write,
) -> Result<bool> {
    let (then, now) = (hashes(site), hashes(&folder.body));
    let backup = format!("{BACKUP_DIR}/{revision}");
    let mut any = false;
    for s in crate::read_manifest(dir)?.sources {
        let key = format!("source {}", s.name);
        let Some(gpl) = s
            .file
            .map(|f| f.replace('\\', "/"))
            .filter(|_| s.kind == "palette")
        else {
            continue;
        };
        if then.get(&key) == now.get(&key) {
            continue;
        }
        let mut moved = Vec::new();
        follow_palette(dir, &gpl, &backup, &folder.assets, &mut |rel| {
            moved.push(rel.to_string())
        })?;
        if !moved.is_empty() {
            any = true;
            writeln!(
                out,
                "{} now follow {gpl}; the old copies are in {backup}/.",
                moved.join(", ")
            )?;
        }
    }
    Ok(any)
}

/// Link `dir` at `revision` holding `body`'s files.
fn adopt(dir: &Path, linked: &Link, revision: i64, body: &Value) -> Result<Link> {
    let link = Link {
        project: linked.project.clone(),
        site: linked.site.clone(),
        revision,
        hashes: hashes(body),
        order: file_order(body),
        pending: BTreeMap::new(),
    };
    link.write(dir).with_context(|| {
        format!("the site holds revision {revision}, but the folder's link could not be updated")
    })?;
    Ok(link)
}

enum Saved {
    /// The site holds the folder's content (or refused it for good, already reported).
    Done,
    /// It never reached the site: try again.
    Transient(String),
    /// The site has a revision this folder has not brought in.
    Behind,
}

enum Pull {
    Transient(anyhow::Error),
    /// The folder does not pack right now (an edit half done): wait for it, do not stop.
    Unpackable(anyhow::Error),
    /// A folder entry changed while the revision was being written: compare again once it settles.
    Moved(anyhow::Error),
    Fatal(anyhow::Error),
}

/// What connect keeps between steps: the link, and the site body it links to.
struct Sync<'a> {
    agent: ureq::Agent,
    creds: Credentials,
    dir: &'a Path,
    link: Link,
    site: Value,
    clashes: Clashes,
    out: &'a mut dyn Write,
    board: Board,
    /// The posting thread's queue: the dashboard's snapshot goes in order with the lines.
    board_tx: mpsc::Sender<Value>,
}

impl Sync<'_> {
    /// Post the dashboard's snapshot when it changed.
    fn publish(&mut self) {
        let snapshot = self.board.snapshot(self.dir, self.link.revision);
        let text = snapshot.to_string();
        if text != self.board.posted {
            let _ = self.board_tx.send(serde_json::json!({ "files": snapshot }));
            self.board.posted = text;
        }
    }

    /// Mark `keys` as `how` (sending, receiving) on the dashboard, or none moving.
    fn moving(&mut self, how: &'static str, keys: Vec<String>, backup: Option<String>) {
        self.board.moving = keys.into_iter().map(|k| (k, how)).collect();
        self.board.backup = backup;
        self.publish();
    }

    /// The link moved from `before`: its changed entries synced now.
    fn synced(&mut self, before: &BTreeMap<String, String>) {
        let at = now_ms();
        for key in changed_keys(before, &self.link.hashes) {
            self.board.at.insert(key, at);
        }
        self.moving("", Vec::new(), None);
    }

    /// Bring the site's newest revision into the folder, if it is not the one linked.
    fn catch_up(&mut self) -> Result<(), Pull> {
        let (revision, body) =
            fetch(&self.agent, &mut self.creds, &self.link.project).map_err(Pull::Transient)?;
        if revision == self.link.revision {
            return Ok(());
        }
        let packed = folder_body(self.dir).map_err(Pull::Unpackable)?;
        let incoming = changed_keys(&self.link.hashes, &hashes(&body));
        self.moving(
            "receiving",
            incoming,
            Some(format!("{BACKUP_DIR}/{revision}")),
        );
        let before = self.link.hashes.clone();
        let next = self.bring_in_revision(revision, &body, &packed);
        self.board.moving.clear();
        let next = next?;
        let wrote = changed(&self.link.hashes, &next.hashes).join(", ");
        writeln!(
            self.out,
            "Wrote {wrote} from revision {revision} (saved elsewhere, such as the Studio)."
        )
        .map_err(|e| Pull::Fatal(e.into()))?;
        (self.link, self.site) = (next, body);
        self.synced(&before);
        Ok(())
    }

    fn bring_in_revision(
        &mut self,
        revision: i64,
        body: &Value,
        packed: &Packed,
    ) -> Result<Link, Pull> {
        bring_in_folder(
            self.dir,
            &self.link,
            revision,
            body,
            packed,
            &mut self.clashes,
            &mut *self.out,
        )
        .map_err(|e| match e.is::<FolderMoved>() {
            true => Pull::Moved(e),
            false => Pull::Fatal(e),
        })
    }

    /// One revision-checked save. The link advances only on the site's answer, or, when the
    /// answer was lost or refused, on a site that already holds this exact content.
    fn save(&mut self, body: Value) -> Result<Saved> {
        let before = self.link.hashes.clone();
        self.moving("sending", changed_keys(&before, &hashes(&body)), None);
        let saved = self.send(body);
        self.synced(&before);
        saved
    }

    fn send(&mut self, body: Value) -> Result<Saved> {
        // Recorded before the PUT: if its answer is lost and the folder edits on, the next
        // connect still knows the site's revision as this folder's own.
        self.link.pending = hashes(&body);
        self.link.write(self.dir)?;
        let (agent, creds, link) = (&self.agent, &mut self.creds, &self.link);
        let mut request = body.clone();
        request["expectedRevision"] = link.revision.into();
        let path = format!("/api/projects/{}", link.project);
        let answer = send(agent, creds, "PUT", &path, Some(request));
        let revision = match answer {
            Ok((200, v)) => v["revision"]
                .as_i64()
                .context("the site answered without a revision")?,
            Ok((status @ (401 | 404), v)) => return Err(refused(creds, &link.project, status, &v)),
            Ok((409, _)) | Err(_) => match fetch(agent, creds, &link.project) {
                // The save landed and its answer was lost, or an earlier one did.
                Ok((revision, now)) if same_content(&now, &body) => revision,
                // The save never reached the site, which has not moved.
                Ok((revision, _)) if revision == link.revision => {
                    return Ok(Saved::Transient("the site could not be reached".into()))
                }
                Ok(_) => return Ok(Saved::Behind),
                Err(e) => return Ok(Saved::Transient(format!("{e:#}"))),
            },
            Ok((status, v)) if status >= 500 => {
                return Ok(Saved::Transient(login::refusal(status, &v)))
            }
            Ok((status, v)) => {
                writeln!(
                    self.out,
                    "Not saved: {}. Save again to retry.",
                    login::refusal(status, &v)
                )?;
                return Ok(Saved::Done);
            }
        };
        let next = adopt(self.dir, &link, revision, &body)?;
        let names = changed(&link.hashes, &next.hashes).join(", ");
        writeln!(self.out, "Saved {names} as revision {revision}.")?;
        (self.link, self.site) = (next, body);
        Ok(Saved::Done)
    }
}

/// What the Connect panel's dashboard can ask a running connect to do, as the change stream
/// carries it (`{"show": {"path": "art/hero.png"}}`): a closed set, each argument checked before it
/// runs, and nothing from the panel ever reaches a shell. A new button is one variant here and one
/// arm in [`Sync::command`].
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
enum Command {
    Pause {},
    Resume {},
    OpenFolder {},
    /// A file of the folder in the OS file manager.
    Show {
        path: String,
    },
    /// `ppu art palette sync`: every PNG naming this `.gpl` takes its colours.
    PaletteSync {
        palette: String,
    },
    /// `ppu skills` (`--reset` overwrites the ones the person edited).
    Skills {
        reset: bool,
    },
    /// `ppu types .`
    Types {},
    /// Send a PNG of the folder back for the add-art dialog's preview, as `{"file": {request,
    /// path, data | error}}`; the site hands it to the tab that asked and keeps nothing.
    Read {
        path: String,
        /// The tab's id for this read; its reply names it.
        request: String,
    },
    /// `ppu art convert <path>` with the add-art dialog's choices as its flags, which
    /// `art convert` checks against the converter's ranges as it does on the command line.
    Convert {
        path: String,
        /// For a PNG `ppu.json` does not list; a listed one keeps its kind (a different one is
        /// refused).
        kind: Option<String>,
        bpp: Option<u8>,
        cell: Option<u8>,
        cell_height: Option<u8>,
        dither: Option<String>,
        dither_strength: Option<u8>,
        alpha_threshold: Option<u8>,
        max_palettes: Option<u8>,
    },
}

/// The largest PNG `read` sends to the Studio: art far bigger than any SNES screen.
const READ_MAX: u64 = 4 * 1024 * 1024;

/// The bytes of the PNG `rel` inside the folder `dir`, up to [`READ_MAX`].
fn read_png(dir: &Path, rel: &str) -> Result<Vec<u8>> {
    let path = inside(dir, rel)?.1;
    let png = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("png"));
    ensure!(png, "{rel} is not a PNG");
    let len = std::fs::metadata(&path)?.len();
    ensure!(
        len <= READ_MAX,
        "{rel} is {:.1} MB, over the {} MB the Studio's preview reads",
        len as f64 / 1048576.0,
        READ_MAX / 1048576
    );
    Ok(std::fs::read(&path)?)
}

/// `rel` as a path inside the folder `dir`, which it must be (after links resolve) and exist in;
/// with the folder itself, both canonical.
fn inside(dir: &Path, rel: &str) -> Result<(std::path::PathBuf, std::path::PathBuf)> {
    let root = dir.canonicalize()?;
    ensure!(
        !Path::new(rel).is_absolute(),
        "{rel} is not a path in the folder"
    );
    let path = root
        .join(rel)
        .canonicalize()
        .with_context(|| format!("{rel} is not in the folder"))?;
    ensure!(
        path.starts_with(&root) && path != root,
        "{rel} is outside the folder"
    );
    Ok((root, path))
}

/// Open `path` (a folder) in the OS file manager, or with `select` the folder holding it with it
/// picked where the file manager can (macOS, Windows).
pub(crate) fn reveal(path: &Path, select: bool) -> Result<()> {
    use std::process::{Command, Stdio};
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut cmd = Command::new("open");
        if select {
            cmd.arg("-R");
        }
        cmd.arg(path);
        cmd
    };
    #[cfg(windows)]
    let mut cmd = {
        use std::os::windows::process::CommandExt;
        // canonicalize's \\?\ prefix is not a path explorer takes
        let shown = path.to_string_lossy();
        let shown = shown.trim_start_matches(r"\\?\");
        let mut cmd = Command::new("explorer");
        match select {
            // explorer parses its own command line: /select, and the path are one quoted argument
            true => cmd.raw_arg(format!("/select,\"{shown}\"")),
            false => cmd.arg(shown),
        };
        cmd
    };
    // shortcut: xdg-open opens the folder but cannot pick the file in it; the FileManager1 D-Bus
    // call would, if people ask for it.
    #[cfg(not(any(target_os = "macos", windows)))]
    let mut cmd = {
        let mut cmd = Command::new("xdg-open");
        cmd.arg(if select {
            path.parent().unwrap_or(path)
        } else {
            path
        });
        cmd
    };
    let program = format!("{:?}", cmd.get_program());
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("cannot start {program}"))?;
    // reaped on a thread of its own: a file manager may stay running
    std::thread::spawn(move || child.wait());
    Ok(())
}

impl Sync<'_> {
    /// Run one dashboard command; what it did (or why not) is a line in the panel's Activity and
    /// the next snapshot. Only a broken terminal is an error.
    fn command(&mut self, command: Value) -> Result<()> {
        let command = match serde_json::from_value::<Command>(command.clone()) {
            Ok(c) => c,
            Err(e) => {
                writeln!(
                    self.out,
                    "Refused a command from the Studio ({e}): {command}"
                )?;
                return Ok(());
            }
        };
        let done = self.run_command(command);
        if let Err(e) = done {
            writeln!(self.out, "Not done: {e:#}")?;
        }
        self.out.flush()?;
        Ok(())
    }

    fn run_command(&mut self, command: Command) -> Result<()> {
        let dir = self.dir;
        match command {
            Command::Pause {} if !self.board.paused => {
                self.board.paused = true;
                writeln!(self.out, "Paused: nothing syncs either way until Resume.")?;
            }
            Command::Resume {} if self.board.paused => {
                self.board.paused = false;
                self.board.waiting.clear();
                writeln!(self.out, "Resumed: catching up.")?;
            }
            Command::Pause {} | Command::Resume {} => {}
            Command::OpenFolder {} => {
                reveal(&dir.canonicalize()?, false)?;
                writeln!(self.out, "Opened {} in the file manager.", shown_dir(dir))?;
            }
            Command::Show { path } => {
                reveal(&inside(dir, &path)?.1, true)?;
                writeln!(self.out, "Showed {path} in the file manager.")?;
            }
            Command::PaletteSync { palette } => {
                let (root, gpl) = inside(dir, &palette)?;
                let backup = format!("{BACKUP_DIR}/{}", self.link.revision);
                let mut moved = Vec::new();
                crate::art::palette_sync_with(&gpl, &mut |png| {
                    let rel = png.strip_prefix(&root).unwrap_or(png);
                    let rel = rel.to_string_lossy().replace('\\', "/");
                    if !dir.join(&backup).join(&rel).exists() {
                        back_up(dir, &backup, &rel)?;
                    }
                    moved.push(rel);
                    Ok(())
                })?;
                match moved.is_empty() {
                    true => writeln!(self.out, "No PNG names {palette}: nothing to sync.")?,
                    false => writeln!(
                        self.out,
                        "{} now follow {palette}; the old copies are in {backup}/.",
                        moved.join(", ")
                    )?,
                }
            }
            Command::Skills { reset } => {
                let skills = crate::write_skills(dir, reset)?;
                let version = env!("CARGO_PKG_VERSION");
                match skills.written.is_empty() {
                    true => writeln!(self.out, "Skills up to date (ppu {version}).")?,
                    false => writeln!(
                        self.out,
                        "Wrote {} for ppu {version}.",
                        skills.written.join(", ")
                    )?,
                }
                self.board.skills = serde_json::json!({"version": version, "kept": skills.kept});
            }
            Command::Types {} => {
                let wrote = crate::write_editor_files(dir)?;
                writeln!(self.out, "Wrote {}.", wrote.join(", "))?;
            }
            Command::Read { path, request } => {
                let file = match read_png(dir, &path) {
                    Ok(bytes) => serde_json::json!({
                        "request": request, "path": path, "data": BASE64.encode(bytes)}),
                    Err(e) => serde_json::json!({
                        "request": request, "path": path, "error": format!("{e:#}")}),
                };
                let _ = self.board_tx.send(serde_json::json!({ "file": file }));
            }
            Command::Convert {
                path,
                kind,
                bpp,
                cell,
                cell_height,
                dither,
                dither_strength,
                alpha_threshold,
                max_palettes,
            } => {
                let (root, png) = inside(dir, &path)?;
                let stem = png.file_stem().and_then(|n| n.to_str()).unwrap_or_default();
                // a palette an earlier convert left is not refused: the first free name
                let names = crate::read_manifest(dir)?.sources;
                let palette = (1..1000)
                    .map(|i| match i {
                        1 => format!("{stem}_pal"),
                        i => format!("{stem}_pal{i}"),
                    })
                    .find(|n| {
                        names.iter().all(|s| &s.name != n)
                            && !root.join("palettes").join(format!("{n}.gpl")).exists()
                    })
                    .context("no free palette name")?;
                let mut args = vec!["--new-palette".to_string(), palette];
                let num = [
                    ("--bpp", bpp),
                    ("--cell", cell),
                    ("--cell-height", cell_height),
                    ("--dither-strength", dither_strength),
                    ("--alpha-threshold", alpha_threshold),
                    ("--max-palettes", max_palettes),
                ];
                for (flag, v) in num.iter().filter_map(|(f, v)| Some((f, (*v)?.to_string()))) {
                    args.extend([flag.to_string(), v]);
                }
                for (flag, v) in [("--kind", kind), ("--dither", dither)] {
                    if let Some(v) = v {
                        args.extend([flag.to_string(), v]);
                    }
                }
                args.push(png.to_string_lossy().into_owned());
                let done = crate::art::convert_with(args)?;
                let backup = done.backup.strip_prefix(&root).unwrap_or(&done.backup);
                writeln!(
                    self.out,
                    "Converted {path}: {} colours; the original is in {}.",
                    done.colours,
                    backup.display()
                )?;
                for n in &done.notes {
                    writeln!(self.out, "  {n}")?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The panel's commands are a closed set, and paths stay inside the folder.
    #[test]
    fn dashboard_commands_are_known_and_stay_in_the_folder() {
        let parse = |v: Value| serde_json::from_value::<Command>(v).is_ok();
        assert!(parse(serde_json::json!({"show": {"path": "main.luau"}})));
        assert!(parse(serde_json::json!({"skills": {"reset": true}})));
        assert!(!parse(serde_json::json!({"shell": {"run": "rm -rf ~"}})));
        assert!(parse(
            serde_json::json!({"convert": {"path": "art/a.png", "bpp": 4, "dither": "bayer"}})
        ));
        assert!(!parse(
            serde_json::json!({"convert": {"path": "art/a.png", "bpp": 300}})
        ));
        assert!(!parse(
            serde_json::json!({"show": {"path": "a", "then": "b"}})
        ));
        let dir = std::env::temp_dir().join(format!("ppu-inside-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("art")).unwrap();
        std::fs::write(dir.join("art/a.png"), b"").unwrap();
        assert!(inside(&dir, "art/a.png").is_ok());
        assert!(inside(&dir, "../").is_err());
        assert!(inside(&dir, "art/../..").is_err());
        assert!(inside(&dir, "/etc/passwd").is_err());
        assert!(inside(&dir, "missing.png").is_err());
        assert!(parse(
            serde_json::json!({"read": {"path": "art/a.png", "request": "r"}})
        ));
        assert!(read_png(&dir, "art/a.png").is_ok());
        std::fs::write(dir.join("main.luau"), b"").unwrap();
        assert!(read_png(&dir, "main.luau").is_err(), "PNGs only");
        let big = std::fs::File::create(dir.join("art/big.png")).unwrap();
        big.set_len(READ_MAX + 1).unwrap();
        assert!(read_png(&dir, "art/big.png").is_err(), "over the cap");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Art lands where the folder's tilesets look for it, so Tiled finds the PNGs.
    #[test]
    fn tileset_images_resolve_inside_the_folder() {
        let dir = std::env::temp_dir().join(format!("ppu-tilesets-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        std::fs::write(dir.join("tiles.tsj"), r#"{"image": "assets/tiles.png"}"#).unwrap();
        std::fs::write(dir.join("maps/cave.tsj"), r#"{"image": "../gfx/cave.png"}"#).unwrap();
        std::fs::write(
            dir.join("maps/w.tmj"),
            r#"{"tilesets": [{"image": "sea.png"}, {"source": "x.tsj"}]}"#,
        )
        .unwrap();
        std::fs::write(dir.join("evil.tsj"), r#"{"image": "../../etc/evil.png"}"#).unwrap();
        let got = tileset_images(&dir);
        assert_eq!(
            got.get("tiles").map(String::as_str),
            Some("assets/tiles.png")
        );
        assert_eq!(got.get("cave").map(String::as_str), Some("gfx/cave.png"));
        assert_eq!(got.get("sea").map(String::as_str), Some("maps/sea.png"));
        assert!(!got.contains_key("evil"), "outside the folder");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A pulled world's art lands where its tileset points (`assets/tiles.png`), not under
    /// `art/`, so Tiled opens the map with its tiles.
    #[test]
    fn pulled_art_lands_where_tilesets_look() {
        let base = std::env::temp_dir().join(format!("ppu-pull-world-{}", std::process::id()));
        let (src, dst) = (base.join("src"), base.join("dst"));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        crate::author::new_world_project(&src).unwrap();
        let body: Value = serde_json::from_str(&crate::pack(&src).unwrap()).unwrap();
        std::fs::create_dir_all(&dst).unwrap();
        write_new_folder(&dst, "https://example.test", "p", 1, &body, &mut Vec::new()).unwrap();
        assert!(
            dst.join("assets/tiles.png").exists(),
            "lands at the tileset's image"
        );
        assert!(!dst.join("art/tiles.png").exists());
        std::fs::remove_dir_all(&base).unwrap();
    }

    /// A pulled song lands as `music/<name>.song.json` that packs back to the site's bytes; one
    /// whose JSON would not import back to them stays a `.ppu/sources` payload.
    #[test]
    fn pulled_songs_land_as_song_json_only_when_they_round_trip() {
        use ppu_core::song::{encode, Note, Pattern, Row, Song};
        let base = std::env::temp_dir().join(format!("ppu-pull-song-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let song = Song {
            tempo: 12000,
            voice_mask: 0xff,
            rows: vec![Row {
                sound: "kick".into(),
                vol: 100,
                ..Default::default()
            }],
            patterns: vec![Pattern {
                name: "A".into(),
                length: 48,
                notes: vec![Note {
                    len: 12,
                    vel: 100,
                    ..Default::default()
                }],
                ..Default::default()
            }],
            arrangement: vec![0],
            ..Default::default()
        };
        // Decodes, but a vol over 127 never passes the JSON's check.
        let mut loud = song.clone();
        loud.rows[0].vol = 200;
        let payload = |s: &Song| BASE64.encode(SourcePayload::Song(encode(s)).encode());
        let body = serde_json::json!({
            "title": "Demo", "description": "",
            "files": [{"name": "main.luau", "source": "function frame(t, f) end\n"}],
            "sources": [
                {"name": "theme", "kind": "song", "options": {}, "meta": {}, "payload": payload(&song)},
                {"name": "loud", "kind": "song", "options": {}, "meta": {}, "payload": payload(&loud)},
            ],
        });
        let mut out = Vec::new();
        write_new_folder(&base, "https://example.test", "p", 1, &body, &mut out).unwrap();
        let text = std::fs::read_to_string(base.join("music/theme.song.json")).unwrap();
        assert_eq!(ppu_core::song::from_json(&text).unwrap(), song);
        let manifest = crate::read_manifest(&base).unwrap();
        let entry = |n: &str| {
            manifest
                .sources
                .iter()
                .find(|s| s.name == n)
                .unwrap()
                .clone()
        };
        assert_eq!(
            entry("theme").file.as_deref(),
            Some("music/theme.song.json")
        );
        assert!(entry("loud")
            .payload
            .as_deref()
            .is_some_and(|p| p.starts_with(".ppu/sources/")));
        assert!(!base.join("music/loud.song.json").exists());
        let packed: Value = serde_json::from_str(&crate::pack(&base).unwrap()).unwrap();
        for s in packed["sources"].as_array().unwrap() {
            let want = body["sources"]
                .as_array()
                .unwrap()
                .iter()
                .find(|b| b["name"] == s["name"]);
            assert_eq!(
                s["payload"],
                want.unwrap()["payload"],
                "{} packs to the site's bytes",
                s["name"]
            );
        }
        let said = String::from_utf8(out).unwrap();
        assert!(said.contains("loud stays raw data"), "{said}");
        std::fs::remove_dir_all(&base).unwrap();
    }

    /// Art pulled where no tileset looks lands in `assets/`; a folder connected before keeps
    /// `art/` until connect starts, which moves only the PNGs a tileset names.
    #[test]
    fn connect_start_moves_art_to_where_tilesets_look() {
        let base = std::env::temp_dir().join(format!("ppu-follow-{}", std::process::id()));
        let (src, dst) = (base.join("src"), base.join("dst"));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        crate::author::new_world_project(&src).unwrap();
        // No tileset looks for tiles: the default folder.
        let tsj = std::fs::read_to_string(src.join("tiles.tsj")).unwrap();
        std::fs::write(
            src.join("tiles.tsj"),
            tsj.replace("assets/tiles.png", "elsewhere.png"),
        )
        .unwrap();
        let body: Value = serde_json::from_str(&crate::pack(&src).unwrap()).unwrap();
        std::fs::create_dir_all(&dst).unwrap();
        write_new_folder(&dst, "https://example.test", "p", 1, &body, &mut Vec::new()).unwrap();
        assert!(dst.join("assets/tiles.png").exists(), "default is assets/");
        // An old folder: the PNG in art/, a tileset looking in gfx/.
        std::fs::create_dir_all(dst.join("art")).unwrap();
        std::fs::rename(dst.join("assets/tiles.png"), dst.join("art/tiles.png")).unwrap();
        let manifest = std::fs::read_to_string(dst.join(crate::MANIFEST)).unwrap();
        std::fs::write(
            dst.join(crate::MANIFEST),
            manifest.replace("assets/tiles.png", "art/tiles.png"),
        )
        .unwrap();
        std::fs::write(dst.join("tiles.tsj"), r#"{"image": "gfx/tiles.png"}"#).unwrap();
        let before = crate::read_manifest(&dst).unwrap();
        let mut log = Vec::new();
        follow_tilesets(&dst, &mut log).unwrap();
        assert!(dst.join("gfx/tiles.png").exists() && !dst.join("art/tiles.png").exists());
        let after = crate::read_manifest(&dst).unwrap();
        assert_eq!(after.sources[0].file.as_deref(), Some("gfx/tiles.png"));
        assert_eq!(
            after.sources[0].options, before.sources[0].options,
            "only file changes"
        );
        assert!(String::from_utf8(log)
            .unwrap()
            .contains("moved art/tiles.png to gfx/tiles.png"));
        // Run again: nothing left to move. A taken path is left alone.
        follow_tilesets(&dst, &mut Vec::new()).unwrap();
        std::fs::write(dst.join("tiles.tsj"), r#"{"image": "main.luau.png"}"#).unwrap();
        std::fs::write(dst.join("other.tsj"), r#"{"image": "x/tiles.png"}"#).unwrap();
        std::fs::create_dir_all(dst.join("x")).unwrap();
        std::fs::write(dst.join("x/tiles.png"), b"someone else's").unwrap();
        std::fs::remove_file(dst.join("tiles.tsj")).unwrap();
        follow_tilesets(&dst, &mut Vec::new()).unwrap();
        assert!(dst.join("gfx/tiles.png").exists(), "taken path: stays put");
        std::fs::remove_dir_all(&base).unwrap();
    }

    /// Site maps are sorted, a folder's follow ppu.json: the same source must hash alike.
    #[test]
    fn source_hashes_ignore_key_order() {
        let site: Value = serde_json::from_str(
            r#"{"sources":[{"name":"a","kind":"image","payload":"p",
            "options":{"bit_depth":4,"nest":{"a":1,"b":2},"palette":"p"},
            "meta":{"height":8,"width":8}}]}"#,
        )
        .unwrap();
        let folder: Value = serde_json::from_str(
            r#"{"sources":[{"name":"a","kind":"image","payload":"p",
            "options":{"palette":"p","nest":{"b":2,"a":1},"bit_depth":4},
            "meta":{"width":8,"height":8}}]}"#,
        )
        .unwrap();
        assert_eq!(hashes(&site), hashes(&folder));
    }

    /// The person saves main.luau after the folder was packed for the compare: the site's copy
    /// must not replace it or remove it, and the link must not move.
    #[test]
    fn a_folder_edit_after_the_compare_is_never_overwritten_or_removed() {
        let body = serde_json::json!({"title": "T", "description": "", "sources": [],
            "files": [{"name": "main.luau", "source": "-- base\n"}]});
        let mut changed = body.clone();
        changed["files"][0]["source"] = "-- site\n".into();
        let mut dropped = body.clone();
        dropped["files"] = serde_json::json!([]);
        for site in [changed, dropped] {
            let dir = tempfile::tempdir().unwrap();
            let link = write_new_folder(
                dir.path(),
                "https://ppu.toys",
                "p1",
                1,
                &body,
                &mut std::io::sink(),
            )
            .unwrap();
            let folder = folder_body(dir.path()).unwrap();
            std::fs::write(dir.path().join("main.luau"), "-- typed meanwhile\n").unwrap();
            let err = bring_in_folder(
                dir.path(),
                &link,
                2,
                &site,
                &folder,
                &mut Clashes::Refuse,
                &mut std::io::sink(),
            )
            .unwrap_err();
            assert!(err.is::<FolderMoved>(), "{err:#}");
            let main = std::fs::read_to_string(dir.path().join("main.luau")).unwrap();
            assert_eq!(main, "-- typed meanwhile\n");
            assert_eq!(Link::read(dir.path()).unwrap().revision, 1);
        }
    }

    /// ppu.json is edited after the folder was packed (a prompt may wait for the person): the
    /// site's change to another field must not overwrite it.
    #[test]
    fn a_manifest_edit_after_the_compare_is_never_overwritten() {
        let body = serde_json::json!({"title": "T", "description": "", "sources": [],
            "files": [{"name": "main.luau", "source": "-- base\n"}]});
        let mut site = body.clone();
        site["description"] = "from the site".into();
        let dir = tempfile::tempdir().unwrap();
        let link = write_new_folder(
            dir.path(),
            "https://ppu.toys",
            "p1",
            1,
            &body,
            &mut std::io::sink(),
        )
        .unwrap();
        let packed = folder_body(dir.path()).unwrap();
        let path = dir.path().join(crate::MANIFEST);
        let edited = std::fs::read_to_string(&path)
            .unwrap()
            .replace("\"T\"", "\"typed meanwhile\"");
        std::fs::write(&path, &edited).unwrap();
        let err = bring_in_folder(
            dir.path(),
            &link,
            2,
            &site,
            &packed,
            &mut Clashes::Refuse,
            &mut std::io::sink(),
        )
        .unwrap_err();
        assert!(err.is::<FolderMoved>(), "{err:#}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), edited);
        assert_eq!(Link::read(dir.path()).unwrap().revision, 1);
    }

    /// A PNG a taken source reads from is edited after the pack: the manifest must not move off it.
    #[test]
    fn a_png_edit_after_the_compare_stops_a_source_the_site_changed() {
        let png = |dir: &Path, shade: u8| {
            let file = std::fs::File::create(dir.join("ship.png")).unwrap();
            let mut e = png::Encoder::new(file, 8, 8);
            e.set_color(png::ColorType::Rgba);
            e.set_depth(png::BitDepth::Eight);
            e.write_header()
                .unwrap()
                .write_image_data(&[shade; 8 * 8 * 4])
                .unwrap();
        };
        let dir = tempfile::tempdir().unwrap();
        png(dir.path(), 1);
        std::fs::write(
            dir.path().join(crate::MANIFEST),
            serde_json::json!({"title": "T", "description": "", "files": ["main.luau"],
                "sources": [{"name": "ship", "kind": "bg", "file": "ship.png"}]})
            .to_string(),
        )
        .unwrap();
        std::fs::write(dir.path().join("main.luau"), "-- base\n").unwrap();
        let packed = folder_body(dir.path()).unwrap();
        assert!(packed.assets.contains_key("ship.png"));
        let mut link = Link::default();
        (link.project, link.revision) = ("p1".into(), 1);
        link.hashes = hashes(&packed.body);
        let mut site = packed.body.clone();
        site["sources"][0]["payload"] = "BBBB".into();
        png(dir.path(), 2);
        let path = dir.path().join(crate::MANIFEST);
        let before = std::fs::read_to_string(&path).unwrap();
        let err = bring_in_folder(
            dir.path(),
            &link,
            2,
            &site,
            &packed,
            &mut Clashes::Refuse,
            &mut std::io::sink(),
        )
        .unwrap_err();
        assert!(err.is::<FolderMoved>(), "{err:#}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    }
}
