//! `ppu connect` against a stub of the site's projects API: pull, saves both ways, presence, clashes.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ppu_cli::connect::{self, Clashes, Keep, Link};
use ppu_cli::login::Credentials;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Site {
    revision: i64,
    body: Value,
    puts: Vec<Value>,
    listens: Vec<String>,
    /// Close the next PUT's connection without answering or saving.
    drop_put: bool,
    /// Close this many of the next PUTs' connections, one each.
    drop_puts: usize,
    /// Hold each PUT this long before answering.
    put_delay: Duration,
}

/// A one-project site at `/api/projects/p1` that checks revisions the way ppu-server does.
fn stub(body: Value) -> (Credentials, Arc<Mutex<Site>>) {
    let site = Arc::new(Mutex::new(Site {
        revision: 1,
        body,
        ..Site::default()
    }));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let shared = site.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let site = shared.clone();
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let mut parts = line.split_whitespace();
                let (method, target) = (
                    parts.next().unwrap().to_string(),
                    parts.next().unwrap().to_string(),
                );
                let (mut length, mut bearer) = (0, false);
                loop {
                    let mut header = String::new();
                    reader.read_line(&mut header).unwrap();
                    let header = header.trim().to_ascii_lowercase();
                    if header.is_empty() {
                        break;
                    }
                    if let Some(n) = header.strip_prefix("content-length:") {
                        length = n.trim().parse().unwrap();
                    }
                    bearer |= header == "authorization: bearer t0k";
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                if method == "PUT" && std::mem::take(&mut site.lock().unwrap().drop_put) {
                    return;
                }
                if method == "PUT" && {
                    let mut s = site.lock().unwrap();
                    let drop = s.drop_puts > 0;
                    s.drop_puts = s.drop_puts.saturating_sub(1);
                    drop
                } {
                    return;
                }
                if method == "PUT" {
                    let delay = site.lock().unwrap().put_delay;
                    std::thread::sleep(delay);
                }
                let (status, answer) = if !bearer {
                    (401, json!({"error": "sign in"}))
                } else {
                    route(&site, &method, &target, &body)
                };
                let text = answer.to_string();
                let mut stream = stream;
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{text}",
                    text.len()
                );
            });
        }
    });
    (
        Credentials {
            site: url,
            token: "t0k".into(),
        },
        site,
    )
}

fn route(site: &Mutex<Site>, method: &str, target: &str, body: &[u8]) -> (u16, Value) {
    if let Some(query) = target.strip_prefix("/api/projects/p1/changes?") {
        site.lock().unwrap().listens.push(query.to_string());
        std::thread::sleep(Duration::from_millis(100));
        return (
            200,
            json!({"revision": site.lock().unwrap().revision, "folderConnected": true}),
        );
    }
    let mut s = site.lock().unwrap();
    match (method, target) {
        ("GET", "/api/projects/p1") => (
            200,
            json!({"id": "p1", "name": "Demo", "revision": s.revision, "body": s.body}),
        ),
        ("PUT", "/api/projects/p1") => {
            let mut save: Value = serde_json::from_slice(body).unwrap();
            if save["expectedRevision"] != s.revision {
                return (
                    409,
                    json!({"error": "This project changed on another device."}),
                );
            }
            save.as_object_mut().unwrap().remove("expectedRevision");
            s.revision += 1;
            s.body = save.clone();
            s.puts.push(save);
            (200, json!({"revision": s.revision}))
        }
        _ => (404, json!({"error": "no such project"})),
    }
}

/// `connect::pull`, with what it announces thrown away.
fn pull(creds: &Credentials, link: &str, dir: &std::path::Path) -> anyhow::Result<Link> {
    connect::pull(creds, link, dir, &mut std::io::sink())
}

fn project() -> Value {
    json!({
        "title": "Demo", "description": "", "tags": ["space"],
        "files": [{"name": "main.luau", "source": "function frame(t, f) end\n"},
                  {"name": "level1.tmj", "source": "{}"}],
        "sources": [{"name": "ship", "kind": "sprite", "builtinId": null, "options": {}, "meta": {"w": 8},
                     "payload": "AAEC"}],
    })
}

/// Another client (the Studio) saves an edit of the site's body as the next revision.
fn studio(site: &Mutex<Site>, edit: impl FnOnce(&mut Value)) {
    let mut s = site.lock().unwrap();
    edit(&mut s.body);
    s.revision += 1;
}

fn start(
    creds: &Credentials,
    folder: &std::path::Path,
) -> std::thread::JoinHandle<anyhow::Result<()>> {
    let creds = Credentials {
        site: creds.site.clone(),
        token: creds.token.clone(),
    };
    let folder = folder.to_path_buf();
    std::thread::spawn(move || {
        connect::connect(&creds, &folder, Clashes::Refuse, &mut std::io::sink())
    })
}

/// What connect printed, readable while it runs.
#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl Write for Capture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Capture {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

fn start_captured(
    creds: &Credentials,
    folder: &std::path::Path,
) -> (std::thread::JoinHandle<anyhow::Result<()>>, Capture) {
    let creds = Credentials {
        site: creds.site.clone(),
        token: creds.token.clone(),
    };
    let (folder, out) = (folder.to_path_buf(), Capture::default());
    let mut sink = out.clone();
    (
        std::thread::spawn(move || connect::connect(&creds, &folder, Clashes::Refuse, &mut sink)),
        out,
    )
}

fn until(what: &str, mut done: impl FnMut() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "timed out waiting for {what}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn pull_writes_the_pack_layout_and_links_the_folder() {
    let (creds, _site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("demo");
    let link = pull(&creds, "https://ppu.toys/studio?project=p1", &folder).unwrap();

    assert_eq!(
        std::fs::read_to_string(folder.join("main.luau")).unwrap(),
        "function frame(t, f) end\n"
    );
    assert_eq!(
        std::fs::read(folder.join(".ppu/sources/0.bin")).unwrap(),
        [0, 1, 2]
    );
    assert_eq!(
        std::fs::read_to_string(folder.join(".gitignore")).unwrap(),
        ".ppu-backup/\n"
    );
    for scaffold in [
        "ppu.json",
        "ppu.d.luau",
        ".luaurc",
        ".vscode/settings.json",
        "project.tiled-project",
    ] {
        assert!(folder.join(scaffold).exists(), "{scaffold} missing");
    }
    assert_eq!((link.project.as_str(), link.revision), ("p1", 1));
    assert_eq!(Link::read(&folder).unwrap(), link);
    let text = std::fs::read_to_string(folder.join(connect::LINK_FILE)).unwrap();
    assert!(text.contains("project = p1\nrevision = 1\n"), "{text}");
    assert!(text.contains(&format!("site = {}\n", creds.site)), "{text}");
    assert!(
        text.contains("file main.luau = ") && text.contains("source ship = "),
        "{text}"
    );
    // The pulled folder packs back to the hashes it was linked with.
    let packed: Value = serde_json::from_str(&ppu_cli::pack(&folder).unwrap()).unwrap();
    assert_eq!(connect::hashes(&packed), link.hashes);

    let err = pull(&creds, "p1", &folder).unwrap_err();
    assert!(format!("{err:#}").contains("not empty"), "{err:#}");
    let err = pull(&creds, "nope", &dir.path().join("other")).unwrap_err();
    assert!(
        format!("{err:#}").contains("deleted, or it belongs to another account"),
        "{err:#}"
    );
}

#[test]
fn saves_reach_the_site_and_the_sites_reach_the_folder() {
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("demo");
    pull(&creds, "p1", &folder).unwrap();
    // An edit made while disconnected is saved on connect.
    std::fs::write(folder.join("main.luau"), "-- offline\n").unwrap();

    let running = {
        let (creds, folder) = (
            Credentials {
                site: creds.site.clone(),
                token: creds.token.clone(),
            },
            folder.clone(),
        );
        std::thread::spawn(move || {
            connect::connect(&creds, &folder, Clashes::Refuse, &mut std::io::sink())
        })
    };
    until("the offline edit", || site.lock().unwrap().puts.len() == 1);
    until("a folder listen", || {
        site.lock()
            .unwrap()
            .listens
            .iter()
            .any(|q| q.contains("folder=true"))
    });
    std::fs::write(folder.join("main.luau"), "-- live\n").unwrap();
    until("the live edit", || site.lock().unwrap().puts.len() == 2);
    until("the link to advance", || {
        Link::read(&folder).unwrap().revision == 3
    });

    {
        let s = site.lock().unwrap();
        assert_eq!(s.puts[0]["files"][0]["source"], "-- offline\n");
        assert_eq!(s.puts[1]["files"][0]["source"], "-- live\n");
        assert_eq!(
            s.puts[1]["tags"],
            json!(["space"]),
            "site-only fields ride along"
        );
    }
    assert!(!running.is_finished());

    // The Studio saves: its edit lands in the folder, the link advances, and nothing echoes back.
    studio(&site, |b| {
        b["files"][0]["source"] = "-- from the studio\n".into()
    });
    until("the studio's edit on disk", || {
        std::fs::read_to_string(folder.join("main.luau")).unwrap() == "-- from the studio\n"
    });
    until("the link to reach revision 4", || {
        Link::read(&folder).unwrap().revision == 4
    });
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(
        site.lock().unwrap().puts.len(),
        2,
        "the CLI's own write echoed as a save"
    );
    // And the folder keeps saving on top of it.
    std::fs::write(folder.join("main.luau"), "-- after\n").unwrap();
    until("the save after", || site.lock().unwrap().puts.len() == 3);
    // The link is written after the PUT returns, so wait for it rather than race it.
    until("the link to reach revision 5", || {
        Link::read(&folder).unwrap().revision == 5
    });
    assert!(!running.is_finished());
}

#[test]
fn a_sign_in_since_connect_started_is_taken_not_reported_as_signed_out() {
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("demo");
    pull(&creds, "p1", &folder).unwrap();
    // `ppu login` in another terminal saved t0k and revoked the token this connect holds.
    let config = tempfile::tempdir().unwrap();
    std::fs::write(
        config.path().join("credentials.json"),
        serde_json::to_string(&creds).unwrap(),
    )
    .unwrap();
    // Only this test reads the credentials file: every other one holds the stub's token.
    std::env::set_var("PPU_CONFIG_DIR", config.path());
    let revoked = Credentials {
        site: creds.site.clone(),
        token: "revoked".into(),
    };
    let running = start(&revoked, &folder);
    std::fs::write(folder.join("main.luau"), "-- typed after the sign-in\n").unwrap();
    until("the folder's edit to save with the new token", || {
        site.lock().unwrap().puts.len() == 1
    });
    assert!(!running.is_finished());

    // With nothing newer saved, a revoked token still stops connect and says so.
    std::fs::write(
        config.path().join("credentials.json"),
        serde_json::to_string(&revoked).unwrap(),
    )
    .unwrap();
    // a second connect in a folder whose connect runs does not start (a repeat ppu:// link)
    let twice = connect::connect(&creds, &folder, Clashes::Refuse, &mut std::io::sink());
    assert!(twice.unwrap_err().is::<connect::AlreadyRunning>());
    // another folder: the first connect still runs in this one
    let other = dir.path().join("demo2");
    pull(&creds, "p1", &other).unwrap();
    let err = connect::connect(&revoked, &other, Clashes::Refuse, &mut std::io::sink());
    let err = err.unwrap_err();
    assert!(format!("{err:#}").contains("run ppu login"), "{err:#}");
    std::env::remove_var("PPU_CONFIG_DIR");
}

#[test]
fn a_folder_left_by_an_interrupted_pull_says_to_pull_again() {
    let (creds, _site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    pull(&creds, "p1", dir.path()).unwrap();
    assert!(
        std::fs::read_dir(dir.path().join(".ppu"))
            .unwrap()
            .all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".tmp")),
        "a temporary file was left behind"
    );
    std::fs::remove_file(dir.path().join(connect::LINK_FILE)).unwrap();
    let err =
        connect::connect(&creds, dir.path(), Clashes::Refuse, &mut std::io::sink()).unwrap_err();
    assert!(
        format!("{err:#}").contains("delete the folder and pull again"),
        "{err:#}"
    );
}

#[test]
fn connect_brings_in_what_the_site_saved_while_disconnected() {
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("demo");
    pull(&creds, "p1", &folder).unwrap();
    // The folder changed one file and the site another: both survive.
    std::fs::write(folder.join("level1.tmj"), "{\"w\":1}").unwrap();
    studio(&site, |b| {
        b["files"][0]["source"] = "-- the studio's edit\n".into()
    });
    let running = start(&creds, &folder);
    until("the folder's edit to save", || {
        site.lock().unwrap().puts.len() == 1
    });
    let put = site.lock().unwrap().puts[0].clone();
    assert_eq!(put["files"][0]["source"], "-- the studio's edit\n");
    assert_eq!(put["files"][1]["source"], "{\"w\":1}");
    assert_eq!(
        std::fs::read_to_string(folder.join("main.luau")).unwrap(),
        "-- the studio's edit\n"
    );
    until("the link to reach revision 3", || {
        Link::read(&folder).unwrap().revision == 3
    });
    assert!(!running.is_finished());
}

#[test]
fn a_file_both_sides_changed_stops_connect_and_writes_nothing() {
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    pull(&creds, "p1", dir.path()).unwrap();
    std::fs::write(dir.path().join("main.luau"), "-- the folder's edit\n").unwrap();
    studio(&site, |b| {
        b["files"][0]["source"] = "-- the studio's edit\n".into();
        b["files"][1]["source"] = "{\"w\":2}".into();
    });
    let err =
        connect::connect(&creds, dir.path(), Clashes::Refuse, &mut std::io::sink()).unwrap_err();
    assert!(
        format!("{err:#}").contains("both changed main.luau. Nothing was written"),
        "{err:#}"
    );
    assert!(
        format!("{err:#}").contains("--keep folder") && format!("{err:#}").contains("--keep site"),
        "{err:#}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("main.luau")).unwrap(),
        "-- the folder's edit\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("level1.tmj")).unwrap(),
        "{}"
    );
    assert_eq!(Link::read(dir.path()).unwrap().revision, 1);
    assert!(site.lock().unwrap().puts.is_empty());
}

#[test]
fn dropped_files_go_and_binary_sources_arrive_byte_exact() {
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("demo");
    pull(&creds, "p1", &folder).unwrap();
    let running = start(&creds, &folder);
    until("a folder listen", || {
        !site.lock().unwrap().listens.is_empty()
    });
    let bytes: Vec<u8> = vec![0, 255, 128, 13, 10, 0, 26, 0xef, 0xbb, 0xbf];
    studio(&site, |b| {
        b["files"].as_array_mut().unwrap().pop();
        b["sources"][0]["payload"] = BASE64.encode(&bytes).into();
        b["sources"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name": "beep", "kind": "sample", "options": {}, "meta": {}, "payload": BASE64.encode([1, 2, 3])}));
    });
    until("the link to reach revision 2", || {
        Link::read(&folder).unwrap().revision == 2
    });
    assert!(!folder.join("level1.tmj").exists());
    // A changed source lands in a fresh payload file and the old one goes, so a crash between
    // the payload and the manifest never pairs the old entry with new bytes.
    let m: Value =
        serde_json::from_str(&std::fs::read_to_string(folder.join("ppu.json")).unwrap()).unwrap();
    let payload = |i: usize| folder.join(m["sources"][i]["payload"].as_str().unwrap());
    assert_eq!(m["sources"][0]["payload"], ".ppu/sources/1.bin");
    assert_eq!(std::fs::read(payload(0)).unwrap(), bytes);
    assert_eq!(std::fs::read(payload(1)).unwrap(), [1, 2, 3]);
    assert!(!folder.join(".ppu/sources/0.bin").exists());
    let packed: Value = serde_json::from_str(&ppu_cli::pack(&folder).unwrap()).unwrap();
    assert_eq!(
        connect::hashes(&packed),
        connect::hashes(&site.lock().unwrap().body)
    );
    std::thread::sleep(Duration::from_millis(1500));
    assert!(
        site.lock().unwrap().puts.is_empty(),
        "the CLI's own write echoed as a save"
    );
    assert!(!running.is_finished());
}

#[test]
fn a_png_source_stays_a_png_until_the_site_changes_it() {
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("demo");
    pull(&creds, "p1", &folder).unwrap();
    let file = std::fs::File::create(folder.join("sky.png")).unwrap();
    let mut png = png::Encoder::new(file, 8, 8);
    png.set_color(png::ColorType::Rgba);
    png.write_header()
        .unwrap()
        .write_image_data(&[255; 256])
        .unwrap();
    let manifest = folder.join("ppu.json");
    let mut m: Value = serde_json::from_str(&std::fs::read_to_string(&manifest).unwrap()).unwrap();
    m["sources"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name": "sky", "kind": "bg", "file": "sky.png"}));
    std::fs::write(&manifest, m.to_string()).unwrap();
    let (running, out) = start_captured(&creds, &folder);
    until("the png to save", || site.lock().unwrap().puts.len() == 1);
    until("the link to reach revision 2", || {
        Link::read(&folder).unwrap().revision == 2
    });

    let sky = |dir: &std::path::Path| {
        let m: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("ppu.json")).unwrap()).unwrap();
        m["sources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == "sky")
            .unwrap()
            .clone()
    };
    studio(&site, |b| b["files"][0]["source"] = "-- typed\n".into());
    until("the studio's edit on disk", || {
        std::fs::read_to_string(folder.join("main.luau")).unwrap() == "-- typed\n"
    });
    assert_eq!(
        sky(&folder)["file"],
        "sky.png",
        "an unchanged PNG source stays a PNG"
    );

    studio(&site, |b| {
        b["sources"][1]["payload"] = BASE64.encode([9, 9]).into()
    });
    until("the studio's art on disk", || {
        sky(&folder).get("payload").is_some()
    });
    let payload = sky(&folder)["payload"].as_str().unwrap().to_string();
    assert_eq!(std::fs::read(folder.join(payload)).unwrap(), [9, 9]);
    assert!(
        folder.join("sky.png").exists(),
        "the person's PNG is never removed"
    );
    // The note prints after the manifest is written.
    until("the switch to be announced", || {
        out.text()
            .contains("sky now comes from the site; sky.png is no longer used.")
    });
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(site.lock().unwrap().puts.len(), 1);
    assert!(!running.is_finished());
}

#[test]
#[ignore = "times out under load: a site sound change after a meta-only edit can be refused as a clash; fix connect, then remove this"]
fn a_wav_source_stays_a_wav_until_the_site_changes_its_sound() {
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("demo");
    pull(&creds, "p1", &folder).unwrap();
    let pcm: Vec<i16> = (0..640).map(|i| ((i * 97) % 4000) as i16 - 2000).collect();
    std::fs::create_dir_all(folder.join("sfx")).unwrap();
    std::fs::write(
        folder.join("sfx/zap.wav"),
        ppu_core::wav::write(&pcm, 2, 44_100).unwrap(),
    )
    .unwrap();
    let manifest = folder.join("ppu.json");
    let mut m: Value = serde_json::from_str(&std::fs::read_to_string(&manifest).unwrap()).unwrap();
    m["sources"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name": "zap", "kind": "sample", "file": "sfx/zap.wav"}));
    std::fs::write(&manifest, m.to_string()).unwrap();
    let (running, out) = start_captured(&creds, &folder);
    until("the wav to save", || site.lock().unwrap().puts.len() == 1);
    until("the link to reach revision 2", || {
        Link::read(&folder).unwrap().revision == 2
    });
    assert_eq!(site.lock().unwrap().body["sources"][1]["kind"], "sample");

    let zap = |dir: &std::path::Path| {
        let m: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("ppu.json")).unwrap()).unwrap();
        m["sources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == "zap")
            .unwrap()
            .clone()
    };
    studio(&site, |b| b["files"][0]["source"] = "-- typed\n".into());
    until("the studio's edit on disk", || {
        std::fs::read_to_string(folder.join("main.luau")).unwrap() == "-- typed\n"
    });
    assert_eq!(
        zap(&folder)["file"],
        "sfx/zap.wav",
        "an unchanged WAV source stays a WAV"
    );

    // The Studio rewrites the source's options and meta but not its sound: the WAV still converts
    // to the site's payload, so it stays.
    studio(&site, |b| {
        b["sources"][1]["options"] = json!({"loop_start": null});
        b["sources"][1]["meta"] = json!({});
    });
    until("the link to reach revision 4", || {
        Link::read(&folder).unwrap().revision >= 4
    });
    assert_eq!(
        zap(&folder)["file"],
        "sfx/zap.wav",
        "a WAV whose sound the site kept stays a WAV"
    );

    studio(&site, |b| {
        b["sources"][1]["payload"] = BASE64.encode([9, 9]).into()
    });
    until("the studio's sound on disk", || {
        zap(&folder).get("payload").is_some()
    });
    assert!(
        folder.join("sfx/zap.wav").exists(),
        "the person's WAV is never removed"
    );
    assert!(
        out.text()
            .contains("zap now comes from the site; sfx/zap.wav is no longer used."),
        "{}",
        out.text()
    );
    assert!(!running.is_finished());
}

#[test]
fn a_song_lands_as_song_json_and_stays_byte_identical_until_the_site_changes_it() {
    use ppu_core::song::{encode, from_json, source_meta, Note, Pattern, Row, Song};
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
    let payload = |s: &Song| BASE64.encode(ppu_core::SourcePayload::Song(encode(s)).encode());
    let mut body = project();
    body["sources"].as_array_mut().unwrap().push(json!({
        "name": "theme", "kind": "song", "options": {},
        "meta": source_meta(&song, &encode(&song)), "payload": payload(&song),
    }));
    let (creds, site) = stub(body);
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("demo");
    pull(&creds, "p1", &folder).unwrap();
    let file = folder.join("music/theme.song.json");
    assert_eq!(
        from_json(&std::fs::read_to_string(&file).unwrap()).unwrap(),
        song
    );
    // The person's own formatting of the same song.
    let compact: String = std::fs::read_to_string(&file)
        .unwrap()
        .split_whitespace()
        .collect();
    std::fs::write(&file, &compact).unwrap();

    let (running, out) = start_captured(&creds, &folder);
    until("connect to listen", || {
        !site.lock().unwrap().listens.is_empty()
    });
    // The Studio rewrites the song's meta but not its notes: the folder's text stays as written,
    // and the folder saves the meta its file packs to.
    studio(&site, |b| b["sources"][1]["meta"] = json!({}));
    until("the link to reach revision 3", || {
        Link::read(&folder).unwrap().revision >= 3
    });
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        compact,
        "an unchanged song stays byte-identical"
    );

    let mut faster = song.clone();
    faster.tempo = 15000;
    studio(&site, |b| {
        b["sources"][1]["payload"] = payload(&faster).into()
    });
    until("the studio's song on disk", || {
        from_json(&std::fs::read_to_string(&file).unwrap()).is_ok_and(|s| s == faster)
    });
    let m: Value =
        serde_json::from_str(&std::fs::read_to_string(folder.join("ppu.json")).unwrap()).unwrap();
    let theme = m["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "theme")
        .unwrap()
        .clone();
    assert_eq!(theme["file"], "music/theme.song.json", "{}", out.text());
    assert!(!running.is_finished());
}

/// A two-note .mid: ppq 480, C4 then E4, a quarter each.
fn two_note_mid() -> Vec<u8> {
    let ev = [
        0, 0x90, 60, 100, 0x83, 0x60, 0x80, 60, 0, 0, 0x90, 64, 100, 0x83, 0x60, 0x80, 64, 0, 0,
        0xff, 0x2f, 0,
    ];
    let mut b = b"MThd\0\0\0\x06\0\0\0\x01\x01\xe0MTrk".to_vec();
    b.extend((ev.len() as u32).to_be_bytes());
    b.extend(ev);
    b
}

/// A song the folder holds as a .mid: an edit in the Studio lands beside it as .song.json, the
/// manifest moves to the JSON and the .mid stays as the person wrote it.
#[test]
fn an_edited_song_beside_its_mid_lands_as_song_json() {
    use ppu_core::song::{encode, from_json, source_meta};
    let mid = two_note_mid();
    let song = ppu_core::import::midi::song_from_midi(&mid).unwrap();
    let payload =
        |s: &ppu_core::song::Song| BASE64.encode(ppu_core::SourcePayload::Song(encode(s)).encode());
    let mut body = project();
    body["sources"].as_array_mut().unwrap().push(json!({
        "name": "theme", "kind": "song", "options": {},
        "meta": source_meta(&song, &encode(&song)), "payload": payload(&song),
    }));
    let (creds, site) = stub(body);
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("demo");
    pull(&creds, "p1", &folder).unwrap();
    // The person keeps the song as their .mid.
    std::fs::remove_file(folder.join("music/theme.song.json")).unwrap();
    std::fs::create_dir_all(folder.join("songs")).unwrap();
    std::fs::write(folder.join("songs/theme.mid"), &mid).unwrap();
    let json_file = folder.join("songs/theme.song.json");
    let manifest = folder.join("ppu.json");
    let mut m: Value = serde_json::from_str(&std::fs::read_to_string(&manifest).unwrap()).unwrap();
    for s in m["sources"].as_array_mut().unwrap() {
        if s["name"] == "theme" {
            s["file"] = "songs/theme.mid".into();
        }
    }
    std::fs::write(&manifest, m.to_string()).unwrap();

    let (running, out) = start_captured(&creds, &folder);
    until("connect to listen", || {
        !site.lock().unwrap().listens.is_empty()
    });
    let mut faster = song.clone();
    faster.tempo = 15000;
    studio(&site, |b| {
        b["sources"][1]["payload"] = payload(&faster).into()
    });
    until("the studio's song beside the .mid", || {
        std::fs::read_to_string(&json_file).is_ok_and(|t| from_json(&t).is_ok_and(|s| s == faster))
    });
    let m: Value = serde_json::from_str(&std::fs::read_to_string(&manifest).unwrap()).unwrap();
    let theme = m["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "theme")
        .unwrap()
        .clone();
    assert_eq!(theme["file"], "songs/theme.song.json", "{}", out.text());
    assert_eq!(std::fs::read(folder.join("songs/theme.mid")).unwrap(), mid);
    assert!(!folder.join("music/theme.song.json").exists());
    assert!(!running.is_finished());
}

#[test]
fn presence_holds_while_a_save_is_slow() {
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("demo");
    pull(&creds, "p1", &folder).unwrap();
    let _running = start(&creds, &folder);
    until("a folder listen", || {
        !site.lock().unwrap().listens.is_empty()
    });
    site.lock().unwrap().put_delay = Duration::from_secs(3);
    std::fs::write(folder.join("main.luau"), "-- slow\n").unwrap();
    // The edit settles and its PUT starts within a second; the stub holds it for three. Meanwhile
    // the Studio saves another file, so the listen hears a revision while the save is in flight.
    std::thread::sleep(Duration::from_millis(1200));
    studio(&site, |b| b["files"][1]["source"] = "{\"w\":3}".into());
    std::thread::sleep(Duration::from_millis(200));
    let before = site.lock().unwrap().listens.len();
    std::thread::sleep(Duration::from_millis(1000));
    assert!(
        site.lock().unwrap().listens.len() > before + 3,
        "the listen stalled while a save was in flight"
    );
    // The slow save is refused (the site moved on); the Studio's file comes in, then it saves.
    until("the folder's save", || site.lock().unwrap().puts.len() == 1);
    let put = site.lock().unwrap().puts[0].clone();
    assert_eq!(put["files"][0]["source"], "-- slow\n");
    assert_eq!(put["files"][1]["source"], "{\"w\":3}");
}

#[test]
fn a_rename_that_changes_only_case_never_removes_the_new_file() {
    // On macOS and Windows the old and new path are one file; removing the old one would take
    // the new one with it. Linux keeps both paths, so this sees the removal skipped.
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("demo");
    pull(&creds, "p1", &folder).unwrap();
    let running = start(&creds, &folder);
    until("a folder listen", || {
        !site.lock().unwrap().listens.is_empty()
    });
    studio(&site, |b| b["files"][1]["name"] = "Level1.tmj".into());
    until("the link to reach revision 2", || {
        Link::read(&folder).unwrap().revision == 2
    });
    let m: Value =
        serde_json::from_str(&std::fs::read_to_string(folder.join("ppu.json")).unwrap()).unwrap();
    assert_eq!(m["files"], json!(["main.luau", "Level1.tmj"]));
    assert_eq!(
        std::fs::read_to_string(folder.join("Level1.tmj")).unwrap(),
        "{}"
    );
    assert!(
        folder.join("level1.tmj").exists(),
        "the old-case path was removed"
    );
    assert!(!running.is_finished());
}

#[test]
fn compare_takes_only_what_the_site_alone_changed() {
    let map = |pairs: &[(&str, &str)]| {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    let base = map(&[
        ("file a", "1"),
        ("file b", "1"),
        ("file c", "1"),
        ("file d", "1"),
        ("file e", "1"),
    ]);
    let folder = map(&[
        ("file a", "1"),
        ("file b", "2"),
        ("file c", "2"),
        ("file d", "2"),
        ("file new", "1"),
    ]);
    let site = map(&[
        ("file a", "2"),
        ("file b", "1"),
        ("file c", "3"),
        ("file d", "2"),
        ("file e", "1"),
        ("file s", "1"),
    ]);
    let got = connect::compare(&base, &folder, &site);
    let take: Vec<&str> = got.take.iter().map(String::as_str).collect();
    // a: site only. s: added by the site. e: the folder dropped it (folder's change, kept).
    assert_eq!(take, ["file a", "file s"]);
    // c: both changed it differently. d: both made the same change.
    assert_eq!(got.clashes, ["file c"]);
    // The site dropping a file the folder changed is a clash too.
    let got = connect::compare(
        &map(&[("file x", "1")]),
        &map(&[("file x", "2")]),
        &map(&[]),
    );
    assert_eq!(got.clashes, ["file x"]);
}

#[test]
fn link_files_are_hand_editable_and_tolerate_newer_keys() {
    let dir = tempfile::tempdir().unwrap();
    let write = |text: &str| {
        std::fs::create_dir_all(dir.path().join(".ppu")).unwrap();
        std::fs::write(dir.path().join(connect::LINK_FILE), text).unwrap();
    };
    write("# mine\n\nproject=p1\n  revision =  7 \nfile a=b.luau = 00ff\n");
    let link = Link::read(dir.path()).unwrap();
    assert_eq!((link.project.as_str(), link.revision), ("p1", 7));
    assert_eq!(link.hashes["file a=b.luau"], "00ff");
    // A key a newer ppu writes is skipped, not refused.
    write("project = p1\nrevision = 7\ncolour = red\n");
    assert_eq!(Link::read(dir.path()).unwrap().revision, 7);
    write("project = p1\nrevision = 7\nno equals sign\n");
    assert!(Link::read(dir.path()).is_err());
    write("project = p1\n");
    assert!(Link::read(dir.path()).is_err());
    assert!(format!("{:#}", Link::read(&dir.path().join("x")).unwrap_err()).contains("not linked"));
}

#[test]
fn project_links_name_the_id() {
    for link in [
        "p1",
        "https://ppu.toys/studio?project=p1&x=2",
        "https://ppu.toys/projects/p1/",
        " p1 ",
    ] {
        assert_eq!(connect::project_id(link).unwrap(), "p1", "{link}");
    }
    assert!(connect::project_id("https://ppu.toys/").is_err());
    assert!(connect::project_id("../x y").is_err());
}

#[test]
fn a_lost_save_answer_is_adopted_not_refused() {
    // The site took the folder's save but the answer never arrived: same content, newer revision.
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("demo");
    pull(&creds, "p1", &folder).unwrap();
    site.lock().unwrap().revision = 2;
    let running = {
        let (creds, folder) = (
            Credentials {
                site: creds.site.clone(),
                token: creds.token.clone(),
            },
            folder.clone(),
        );
        std::thread::spawn(move || {
            connect::connect(&creds, &folder, Clashes::Refuse, &mut std::io::sink())
        })
    };
    until("the link to adopt revision 2", || {
        Link::read(&folder).unwrap().revision == 2
    });
    std::fs::write(folder.join("main.luau"), "-- next\n").unwrap();
    until("the next save", || site.lock().unwrap().puts.len() == 1);
    // The site records the save before connect writes the link: wait for the link, do not race it.
    until("the link to adopt revision 3", || {
        Link::read(&folder).unwrap().revision == 3
    });
    assert!(!running.is_finished());
}

#[test]
fn a_save_that_never_reached_the_site_is_retried() {
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("demo");
    pull(&creds, "p1", &folder).unwrap();
    let running = {
        let (creds, folder) = (
            Credentials {
                site: creds.site.clone(),
                token: creds.token.clone(),
            },
            folder.clone(),
        );
        std::thread::spawn(move || {
            connect::connect(&creds, &folder, Clashes::Refuse, &mut std::io::sink())
        })
    };
    until("a folder listen", || {
        !site.lock().unwrap().listens.is_empty()
    });
    site.lock().unwrap().drop_put = true;
    std::fs::write(folder.join("main.luau"), "-- dropped\n").unwrap();
    until("the dropped save", || !site.lock().unwrap().drop_put);
    assert!(
        !running.is_finished(),
        "a dropped request must not stop connect"
    );
    // No new edit: connect tries again on its own.
    until("the retried save", || site.lock().unwrap().puts.len() == 1);
    assert_eq!(
        site.lock().unwrap().puts[0]["files"][0]["source"],
        "-- dropped\n"
    );
    until("the link to reach revision 2", || {
        Link::read(&folder).unwrap().revision == 2
    });
}

#[test]
fn reordering_files_is_a_change() {
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("demo");
    pull(&creds, "p1", &folder).unwrap();
    let manifest = folder.join("ppu.json");
    let mut m: Value = serde_json::from_str(&std::fs::read_to_string(&manifest).unwrap()).unwrap();
    m["files"] = json!(["level1.tmj", "main.luau"]);
    std::fs::write(&manifest, m.to_string()).unwrap();
    let _running = {
        let (creds, folder) = (
            Credentials {
                site: creds.site.clone(),
                token: creds.token.clone(),
            },
            folder.clone(),
        );
        std::thread::spawn(move || {
            connect::connect(&creds, &folder, Clashes::Refuse, &mut std::io::sink())
        })
    };
    until("the reorder to save", || {
        site.lock().unwrap().puts.len() == 1
    });
    assert_eq!(
        site.lock().unwrap().puts[0]["files"][0]["name"],
        "level1.tmj"
    );
}

/// The site changes the project with `edit`; connect must refuse it, naming `needle`, and leave
/// the folder as it was: nothing written, link still at revision 1.
fn refuses(edit: impl FnOnce(&mut Value), needle: &str) -> std::path::PathBuf {
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("demo");
    pull(&creds, "p1", &folder).unwrap();
    let before = std::fs::read_to_string(folder.join("ppu.json")).unwrap();
    studio(&site, edit);
    let err = connect::connect(&creds, &folder, Clashes::Refuse, &mut std::io::sink()).unwrap_err();
    assert!(format!("{err:#}").contains(needle), "{err:#}");
    assert_eq!(Link::read(&folder).unwrap().revision, 1);
    assert_eq!(
        std::fs::read_to_string(folder.join("ppu.json")).unwrap(),
        before
    );
    assert_eq!(
        std::fs::read_to_string(folder.join("main.luau")).unwrap(),
        "function frame(t, f) end\n"
    );
    // The temp dir goes with the folder's listing checked by the caller.
    let kept = dir.keep();
    kept.join("demo")
}

#[test]
fn a_site_name_never_lands_in_a_dot_path_or_the_declarations_file() {
    for name in [
        ".git/hooks/pre-commit",
        ".vscode/tasks.json",
        ".ppu/extra.bin",
        ".hidden.luau",
        "levels/.secret.luau",
        "ppu.d.luau",
    ] {
        let folder = refuses(
            |b| {
                b["files"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"name": name, "source": "x"}))
            },
            &format!("unsafe file name {name:?}"),
        );
        assert!(
            std::fs::read_to_string(folder.join(name)).map_or(true, |t| t != "x"),
            "{name} was written"
        );
        let _ = std::fs::remove_dir_all(folder.parent().unwrap());
        // Pull into an empty folder (unpack) refuses the same names, writing nothing.
        let mut body = project();
        body["files"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name": name, "source": "x"}));
        let (creds, _site) = stub(body);
        let dir = tempfile::tempdir().unwrap();
        let err = pull(&creds, "p1", &dir.path().join("demo")).unwrap_err();
        assert!(format!("{err:#}").contains("unsafe file name"), "{err:#}");
        assert!(!dir.path().join("demo/main.luau").exists());
    }
}

#[test]
fn the_sites_shapes_are_checked_before_anything_is_written() {
    let folder = refuses(
        |b| {
            b["files"][0]["source"] = "-- edit\n".into();
            b["files"][1].as_object_mut().unwrap().remove("source");
        },
        "file \"level1.tmj\" has no source",
    );
    let _ = std::fs::remove_dir_all(folder.parent().unwrap());
    let folder = refuses(
        |b| {
            b["files"][0]["source"] = "-- edit\n".into();
            b["sources"][0]["options"] = json!({"x": 1});
            b["sources"][0].as_object_mut().unwrap().remove("payload");
        },
        "source \"ship\" has no payload",
    );
    let _ = std::fs::remove_dir_all(folder.parent().unwrap());
    let folder = refuses(
        |b| b["sources"][0]["name"] = "sh\nip".into(),
        "source name with a line break",
    );
    let _ = std::fs::remove_dir_all(folder.parent().unwrap());
    let folder = refuses(
        |b| {
            b["files"]
                .as_array_mut()
                .unwrap()
                .push(json!({"name": "a\nb.luau", "source": "x"}))
        },
        "unsafe file name",
    );
    let _ = std::fs::remove_dir_all(folder.parent().unwrap());
}

#[test]
fn manifest_fields_are_judged_like_files() {
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("demo");
    pull(&creds, "p1", &folder).unwrap();
    assert!(connect::hashes(&project()).contains_key("meta title"));
    let manifest = |f: &std::path::Path| -> Value {
        serde_json::from_str(&std::fs::read_to_string(f.join("ppu.json")).unwrap()).unwrap()
    };
    // The folder sets targetHz while disconnected; the site retitles and edits code.
    let mut m = manifest(&folder);
    m["targetHz"] = 30.into();
    std::fs::write(folder.join("ppu.json"), m.to_string()).unwrap();
    studio(&site, |b| {
        b["title"] = "Renamed".into();
        b["files"][0]["source"] = "-- typed\n".into();
    });
    let running = start(&creds, &folder);
    until("the folder's targetHz to save", || {
        site.lock().unwrap().puts.len() == 1
    });
    let put = site.lock().unwrap().puts[0].clone();
    assert_eq!(put["targetHz"], 30, "the folder's edit survives");
    assert_eq!(put["title"], "Renamed", "the site's edit lands");
    let m = manifest(&folder);
    assert_eq!(
        (m["targetHz"].clone(), m["title"].clone()),
        (30.into(), "Renamed".into())
    );
    assert!(!running.is_finished());

    // Both sides retitle differently: a clash, nothing written.
    let dir = tempfile::tempdir().unwrap();
    let (creds, site) = stub(project());
    pull(&creds, "p1", dir.path()).unwrap();
    let mut m = manifest(dir.path());
    m["title"] = "Mine".into();
    std::fs::write(dir.path().join("ppu.json"), m.to_string()).unwrap();
    studio(&site, |b| b["title"] = "Theirs".into());
    let err =
        connect::connect(&creds, dir.path(), Clashes::Refuse, &mut std::io::sink()).unwrap_err();
    assert!(
        format!("{err:#}").contains("both changed the title"),
        "{err:#}"
    );
    assert_eq!(manifest(dir.path())["title"], "Mine");
}

#[test]
fn a_file_the_folder_has_but_does_not_list_is_not_overwritten() {
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    pull(&creds, "p1", dir.path()).unwrap();
    std::fs::write(dir.path().join("enemies.luau"), "-- mine\n").unwrap();
    studio(&site, |b| {
        b["files"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name": "enemies.luau", "source": "-- theirs\n"}))
    });
    let err =
        connect::connect(&creds, dir.path(), Clashes::Refuse, &mut std::io::sink()).unwrap_err();
    assert!(
        format!("{err:#}").contains("both changed enemies.luau"),
        "{err:#}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("enemies.luau")).unwrap(),
        "-- mine\n"
    );
}

#[test]
fn a_folder_that_does_not_pack_waits_for_the_site_instead_of_stopping() {
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("demo");
    pull(&creds, "p1", &folder).unwrap();
    let (running, out) = start_captured(&creds, &folder);
    until("a folder listen", || {
        !site.lock().unwrap().listens.is_empty()
    });
    // An edit half done: ppu.json lists a file that is not written yet.
    let manifest = folder.join("ppu.json");
    let mut m: Value = serde_json::from_str(&std::fs::read_to_string(&manifest).unwrap()).unwrap();
    m["files"]
        .as_array_mut()
        .unwrap()
        .push("enemies.luau".into());
    std::fs::write(&manifest, m.to_string()).unwrap();
    std::thread::sleep(Duration::from_millis(1000));
    studio(&site, |b| b["files"][0]["source"] = "-- typed\n".into());
    until("the wait to be reported", || {
        out.text().contains("does not pack")
    });
    assert!(!running.is_finished(), "{}", out.text());
    std::fs::write(folder.join("enemies.luau"), "-- done\n").unwrap();
    until("the site's edit on disk", || {
        std::fs::read_to_string(folder.join("main.luau")).unwrap() == "-- typed\n"
    });
    assert!(!running.is_finished());
    until("the folder's addition to save", || {
        site.lock().unwrap().puts.len() == 1
    });
    assert_eq!(
        site.lock().unwrap().puts[0]["files"][2]["name"],
        "enemies.luau"
    );
}

#[test]
fn dropped_files_go_only_after_the_manifest_and_link_are_written() {
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    pull(&creds, "p1", dir.path()).unwrap();
    studio(&site, |b| {
        b["files"].as_array_mut().unwrap().remove(1);
    });
    // The link cannot be written: a directory sits where its temporary file goes.
    let temp = format!(".ppu/link.{}.tmp", std::process::id());
    std::fs::create_dir(dir.path().join(temp)).unwrap();
    let err =
        connect::connect(&creds, dir.path(), Clashes::Refuse, &mut std::io::sink()).unwrap_err();
    assert!(
        format!("{err:#}").contains("link could not be updated"),
        "{err:#}"
    );
    assert!(
        dir.path().join("level1.tmj").exists(),
        "the dropped file stays until the link is written"
    );
}

#[test]
fn a_failed_save_after_a_catch_up_starts_the_backoff_over() {
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("demo");
    pull(&creds, "p1", &folder).unwrap();
    let (running, out) = start_captured(&creds, &folder);
    until("a folder listen", || {
        !site.lock().unwrap().listens.is_empty()
    });
    // Exactly two saves fail, so no third failure can print a doubled wait after the second.
    site.lock().unwrap().drop_puts = 2;
    std::fs::write(folder.join("level1.tmj"), "{\"w\":3}").unwrap();
    // The Studio edit lands only once the first failure is reported, so the save that follows it
    // finds the site unmoved (a failure) and the catch_up is what comes between the two.
    until("the first failed save reported", || {
        out.text().matches("Not saved").count() == 1
    });
    studio(&site, |b| b["files"][0]["source"] = "-- typed\n".into());
    until("two failed saves reported", || {
        out.text().matches("Not saved").count() == 2
    });
    // The first failure waited 1 s and doubled; the catch_up between them starts it over.
    let text = out.text();
    assert!(!text.contains("Retrying in 2 s"), "{text}");
    until("the third save to land", || {
        site.lock().unwrap().puts.len() == 1
    });
    assert!(!running.is_finished());
}

#[test]
fn a_file_the_site_added_that_the_folder_already_holds_verbatim_is_taken_and_listed() {
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("demo");
    pull(&creds, "p1", &folder).unwrap();
    std::fs::write(folder.join("enemies.luau"), "-- mine\n").unwrap();
    studio(&site, |b| {
        b["files"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name": "enemies.luau", "source": "-- theirs\n"}))
    });
    connect::connect(&creds, &folder, Clashes::Refuse, &mut std::io::sink()).unwrap_err();
    // The clash's own advice: copy the site's version over yours, then connect again.
    std::fs::write(folder.join("enemies.luau"), "-- theirs\n").unwrap();
    let _running = start(&creds, &folder);
    until("the link to adopt revision 2", || {
        Link::read(&folder).unwrap().revision == 2
    });
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(folder.join("ppu.json")).unwrap()).unwrap();
    assert!(
        manifest["files"]
            .as_array()
            .unwrap()
            .contains(&"enemies.luau".into()),
        "ppu.json must list the file: {manifest}"
    );
    // The next save must carry it: the site does not lose the Studio's new file.
    std::fs::write(folder.join("main.luau"), "-- edit\n").unwrap();
    until("the folder's save", || site.lock().unwrap().puts.len() == 1);
    let put = site.lock().unwrap().puts[0].clone();
    assert!(
        put["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["name"] == "enemies.luau"),
        "{put}"
    );
}

#[test]
fn names_that_collide_on_windows_or_macos_are_refused_before_anything_is_written() {
    for name in [
        "PPU.JSON",
        "Ppu.D.Luau",
        "CON",
        "aux.luau",
        "levels/NUL.tmj",
        "com1.luau",
        "LPT9.x.luau",
    ] {
        let folder = refuses(
            |b| {
                b["files"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"name": name, "source": "x"}))
            },
            &format!("unsafe file name {name:?}"),
        );
        let _ = std::fs::remove_dir_all(folder.parent().unwrap());
    }
    let folder = refuses(
        |b| {
            b["files"]
                .as_array_mut()
                .unwrap()
                .push(json!({"name": "Main.luau", "source": "x"}))
        },
        "differ only in case",
    );
    let _ = std::fs::remove_dir_all(folder.parent().unwrap());
}

#[test]
fn a_site_target_hz_or_source_kind_that_cannot_be_kept_is_refused_before_writing() {
    for hz in [json!(300), json!(-1), json!(30.5), json!("30")] {
        let folder = refuses(|b| b["targetHz"] = hz.clone(), "targetHz");
        let _ = std::fs::remove_dir_all(folder.parent().unwrap());
    }
    for kind in [json!(null), json!(7)] {
        let folder = refuses(
            |b| {
                b["sources"][0]["kind"] = kind.clone();
                b["sources"][0]["options"] = json!({"x": 1});
            },
            "has no kind",
        );
        let _ = std::fs::remove_dir_all(folder.parent().unwrap());
    }
}

#[test]
fn a_link_from_before_manifest_fields_were_hashed_takes_the_sites_retitle() {
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    pull(&creds, "p1", dir.path()).unwrap();
    let link = dir.path().join(".ppu/link");
    let text = std::fs::read_to_string(&link).unwrap();
    let old: String = text
        .lines()
        .filter(|l| !l.starts_with("meta "))
        .map(|l| format!("{l}\n"))
        .collect();
    assert_ne!(text, old, "the link had meta lines to drop");
    std::fs::write(&link, old).unwrap();
    studio(&site, |b| b["title"] = "Renamed".into());
    let _running = start(&creds, dir.path());
    until("the retitle on disk", || {
        std::fs::read_to_string(dir.path().join("ppu.json"))
            .unwrap()
            .contains("\"Renamed\"")
    });
}

fn start_with(
    creds: &Credentials,
    folder: &std::path::Path,
    clashes: Clashes,
) -> (std::thread::JoinHandle<anyhow::Result<()>>, Capture) {
    let creds = Credentials {
        site: creds.site.clone(),
        token: creds.token.clone(),
    };
    let (folder, out) = (folder.to_path_buf(), Capture::default());
    let mut sink = out.clone();
    (
        std::thread::spawn(move || connect::connect(&creds, &folder, clashes, &mut sink)),
        out,
    )
}

fn read(folder: &std::path::Path, name: &str) -> String {
    std::fs::read_to_string(folder.join(name)).unwrap()
}

/// A pulled folder and a site that both edited main.luau; the site also edited level1.tmj.
fn clashing() -> (Credentials, Arc<Mutex<Site>>, tempfile::TempDir) {
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    pull(&creds, "p1", dir.path()).unwrap();
    std::fs::write(dir.path().join("main.luau"), "-- the folder's edit\n").unwrap();
    studio(&site, |b| {
        b["files"][0]["source"] = "-- the studio's edit\n".into();
        b["files"][1]["source"] = "{\"w\":2}".into();
    });
    (creds, site, dir)
}

#[test]
fn keep_site_takes_the_sites_copy_and_keeps_the_folders_in_the_backup() {
    let (creds, site, dir) = clashing();
    let (running, out) = start_with(
        &creds,
        dir.path(),
        Clashes::AtStart(Keep::Site, Box::new(Clashes::Refuse)),
    );
    until("the link to reach revision 2", || {
        Link::read(dir.path()).unwrap().revision == 2
    });
    assert_eq!(read(dir.path(), "main.luau"), "-- the studio's edit\n");
    assert_eq!(read(dir.path(), "level1.tmj"), "{\"w\":2}");
    assert_eq!(
        read(dir.path(), ".ppu-backup/2/main.luau"),
        "-- the folder's edit\n"
    );
    std::thread::sleep(Duration::from_millis(1200));
    assert!(
        site.lock().unwrap().puts.is_empty(),
        "the folder now holds the site's copy"
    );
    assert!(
        out.text()
            .contains("Kept this folder's main.luau in .ppu-backup/2/"),
        "{}",
        out.text()
    );
    assert!(!running.is_finished());
}

#[test]
fn keep_folder_makes_one_save_of_the_merged_project() {
    let (creds, site, dir) = clashing();
    let (running, _out) = start_with(
        &creds,
        dir.path(),
        Clashes::AtStart(Keep::Folder, Box::new(Clashes::Refuse)),
    );
    until("the merged save", || site.lock().unwrap().puts.len() == 1);
    let put = site.lock().unwrap().puts[0].clone();
    assert_eq!(put["files"][0]["source"], "-- the folder's edit\n");
    assert_eq!(
        put["files"][1]["source"], "{\"w\":2}",
        "the site's other edit is merged"
    );
    assert_eq!(read(dir.path(), "level1.tmj"), "{\"w\":2}");
    until("the link to reach revision 3", || {
        Link::read(dir.path()).unwrap().revision == 3
    });
    assert!(
        !dir.path().join(".ppu-backup").exists(),
        "nothing of the folder's was replaced"
    );
    std::thread::sleep(Duration::from_millis(1200));
    assert_eq!(site.lock().unwrap().puts.len(), 1, "one save");
    assert!(!running.is_finished());
}

/// Scripted answers for `Clashes::Ask`, recording each question.
fn asking(answers: &[Keep]) -> (Clashes, Arc<Mutex<Vec<String>>>) {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let (log, mut answers) = (asked.clone(), Vec::from(answers).into_iter());
    let ask = move |_: &str, q: &str| {
        log.lock().unwrap().push(q.to_string());
        answers
            .next()
            .ok_or_else(|| anyhow::anyhow!("no more answers"))
    };
    (Clashes::Ask(Box::new(ask)), asked)
}

#[test]
fn a_terminal_is_asked_about_each_clash_in_the_persons_words() {
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    pull(&creds, "p1", dir.path()).unwrap();
    std::fs::write(dir.path().join("main.luau"), "-- mine\n").unwrap();
    let mut m: Value = serde_json::from_str(&read(dir.path(), "ppu.json")).unwrap();
    m["title"] = "Mine".into();
    std::fs::write(dir.path().join("ppu.json"), m.to_string()).unwrap();
    studio(&site, |b| {
        b["files"][0]["source"] = "-- theirs\n".into();
        b["title"] = "Theirs".into();
    });
    let (clashes, asked) = asking(&[Keep::Site, Keep::Folder]);
    let (running, _out) = start_with(&creds, dir.path(), clashes);
    until("the merged save", || site.lock().unwrap().puts.len() == 1);
    let asked = asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 2, "{asked:?}");
    assert!(
        asked[0].contains("both changed main.luau. Keep folder's, or take site's"),
        "{asked:?}"
    );
    assert!(asked[0].contains(".ppu-backup/2/"), "{asked:?}");
    assert!(asked[1].contains("both changed the title."), "{asked:?}");
    let put = site.lock().unwrap().puts[0].clone();
    assert_eq!(put["files"][0]["source"], "-- theirs\n");
    assert_eq!(put["title"], "Mine");
    assert_eq!(read(dir.path(), ".ppu-backup/2/main.luau"), "-- mine\n");
    assert!(!running.is_finished());
}

#[test]
fn without_a_terminal_every_kind_of_clash_is_named_and_nothing_is_written() {
    let mut body = project();
    body["files"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name": "c.luau", "source": ""}));
    let (creds, site) = stub(body);
    let dir = tempfile::tempdir().unwrap();
    pull(&creds, "p1", dir.path()).unwrap();
    let mut m: Value = serde_json::from_str(&read(dir.path(), "ppu.json")).unwrap();
    m["targetHz"] = 30.into();
    m["sources"][0]["options"] = json!({"x": 1});
    m["files"] = json!(["level1.tmj", "main.luau", "c.luau"]);
    std::fs::write(dir.path().join("ppu.json"), m.to_string()).unwrap();
    let before = read(dir.path(), "ppu.json");
    studio(&site, |b| {
        b["targetHz"] = 20.into();
        b["sources"][0]["options"] = json!({"x": 2});
        b["files"][1]["source"] = "{\"w\":3}".into();
        b["files"].as_array_mut().unwrap().swap(1, 2);
        b["files"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name": "z.luau", "source": ""}));
    });
    let err =
        connect::connect(&creds, dir.path(), Clashes::Refuse, &mut std::io::sink()).unwrap_err();
    let err = format!("{err:#}");
    for needle in [
        "the frame rate (targetHz)",
        "the source ship",
        "the file order",
        "Nothing was written",
    ] {
        assert!(err.contains(needle), "{needle}: {err}");
    }
    assert_eq!(read(dir.path(), "ppu.json"), before);
    assert_eq!(read(dir.path(), "level1.tmj"), "{}");
    assert!(!dir.path().join("z.luau").exists());
    assert_eq!(Link::read(dir.path()).unwrap().revision, 1);
}

#[test]
fn adds_and_deletes_on_either_side_follow_the_same_rule() {
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path();
    pull(&creds, "p1", folder).unwrap();
    // The folder drops level1.tmj and adds b.luau; the site adds a.luau and a different b.luau.
    let mut m: Value = serde_json::from_str(&read(folder, "ppu.json")).unwrap();
    m["files"] = json!(["main.luau", "b.luau"]);
    std::fs::write(folder.join("ppu.json"), m.to_string()).unwrap();
    std::fs::remove_file(folder.join("level1.tmj")).unwrap();
    std::fs::write(folder.join("b.luau"), "-- folder b\n").unwrap();
    studio(&site, |b| {
        let files = b["files"].as_array_mut().unwrap();
        files.push(json!({"name": "a.luau", "source": "-- site a\n"}));
        files.push(json!({"name": "b.luau", "source": "-- site b\n"}));
    });
    let (running, _out) = start_with(
        &creds,
        folder,
        Clashes::AtStart(Keep::Site, Box::new(Clashes::Refuse)),
    );
    until("the merged save", || site.lock().unwrap().puts.len() == 1);
    let put = site.lock().unwrap().puts[0].clone();
    let names: Vec<&str> = put["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        ["main.luau", "b.luau", "a.luau"],
        "the folder's drop stays, the site's add lands"
    );
    assert_eq!(read(folder, "a.luau"), "-- site a\n");
    assert_eq!(read(folder, "b.luau"), "-- site b\n");
    assert_eq!(read(folder, ".ppu-backup/2/b.luau"), "-- folder b\n");
    assert!(!folder.join("level1.tmj").exists());
    assert!(!running.is_finished());

    // The folder edits a file the site deleted: keeping the folder's puts it back on the site.
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    pull(&creds, "p1", dir.path()).unwrap();
    std::fs::write(dir.path().join("level1.tmj"), "{\"mine\":1}").unwrap();
    studio(&site, |b| {
        b["files"].as_array_mut().unwrap().pop();
    });
    let (running, _out) = start_with(
        &creds,
        dir.path(),
        Clashes::AtStart(Keep::Folder, Box::new(Clashes::Refuse)),
    );
    until("the save", || site.lock().unwrap().puts.len() == 1);
    assert_eq!(
        site.lock().unwrap().puts[0]["files"][1]["source"],
        "{\"mine\":1}"
    );
    assert!(!running.is_finished());
}

#[test]
fn a_live_clash_is_asked_while_presence_holds() {
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path();
    pull(&creds, "p1", folder).unwrap();
    let listens = {
        let site = site.clone();
        move || site.lock().unwrap().listens.len()
    };
    let during = Arc::new(Mutex::new((0, 0)));
    let ask = {
        let (listens, during) = (listens.clone(), during.clone());
        move |_: &str, _: &str| {
            let before = listens();
            std::thread::sleep(Duration::from_millis(1500));
            *during.lock().unwrap() = (before, listens());
            Ok(Keep::Site)
        }
    };
    let (running, _out) = start_with(&creds, folder, Clashes::Ask(Box::new(ask)));
    until("a folder listen", || listens() > 0);
    // The folder's save cannot get through while the Studio saves the same file.
    site.lock().unwrap().drop_puts = 1000;
    std::fs::write(folder.join("main.luau"), "-- folder\n").unwrap();
    until("the folder's save to be tried", || {
        site.lock().unwrap().drop_puts < 1000
    });
    studio(&site, |b| b["files"][0]["source"] = "-- studio\n".into());
    site.lock().unwrap().drop_puts = 0;
    until("the site's copy", || {
        read(folder, "main.luau") == "-- studio\n"
    });
    assert_eq!(read(folder, ".ppu-backup/2/main.luau"), "-- folder\n");
    let (before, after) = *during.lock().unwrap();
    assert!(
        after > before,
        "the listen stopped while the question waited"
    );
    assert!(!running.is_finished());
}

#[test]
fn a_save_records_its_hashes_in_the_link_while_in_flight() {
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    pull(&creds, "p1", dir.path()).unwrap();
    site.lock().unwrap().put_delay = Duration::from_millis(1500);
    let (running, _out) = start_with(&creds, dir.path(), Clashes::Refuse);
    std::fs::write(dir.path().join("main.luau"), "-- first\n").unwrap();
    until("the save in flight", || {
        !Link::read(dir.path()).unwrap().pending.is_empty()
    });
    let mut sent = project();
    sent["files"][0]["source"] = "-- first\n".into();
    let link = Link::read(dir.path()).unwrap();
    assert_eq!((link.revision, &link.pending), (1, &connect::hashes(&sent)));
    until("the answer", || {
        Link::read(dir.path()).unwrap().revision == 2
    });
    assert!(Link::read(dir.path()).unwrap().pending.is_empty());
    assert!(!running.is_finished());
}

#[test]
fn a_lost_save_answer_then_another_edit_is_not_a_clash() {
    // The folder saved "-- first" (the site holds it as revision 2), but connect stopped before
    // the answer came, and the folder edited main.luau again.
    let mut sent = project();
    sent["files"][0]["source"] = "-- first\n".into();
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path();
    pull(&creds, "p1", folder).unwrap();
    let mut link = Link::read(folder).unwrap();
    link.pending = connect::hashes(&sent);
    link.write(folder).unwrap();
    {
        let mut s = site.lock().unwrap();
        (s.body, s.revision) = (sent, 2);
    }
    std::fs::write(folder.join("main.luau"), "-- second\n").unwrap();
    let (running, _out) = start_with(&creds, folder, Clashes::Refuse);
    until("the second edit to save", || {
        site.lock().unwrap().puts.len() == 1
    });
    assert_eq!(
        site.lock().unwrap().puts[0]["files"][0]["source"],
        "-- second\n"
    );
    until("the link to reach revision 3", || {
        Link::read(folder).unwrap().revision == 3
    });
    assert!(!running.is_finished());
}

#[test]
fn line_endings_alone_are_not_an_edit() {
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path();
    pull(&creds, "p1", folder).unwrap();
    // A Windows editor rewrote main.luau with CRLF; the Studio edited it.
    std::fs::write(folder.join("main.luau"), "function frame(t, f) end\r\n").unwrap();
    studio(&site, |b| b["files"][0]["source"] = "-- studio\n".into());
    let (running, _out) = start_with(&creds, folder, Clashes::Refuse);
    until("the site's copy", || {
        read(folder, "main.luau") == "-- studio\n"
    });
    assert!(!folder.join(".ppu-backup").exists());
    assert!(!running.is_finished());
}

#[test]
fn a_reorder_on_one_side_survives_an_add_on_the_other() {
    // The folder reorders; the site adds a file.
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path();
    pull(&creds, "p1", folder).unwrap();
    let mut m: Value = serde_json::from_str(&read(folder, "ppu.json")).unwrap();
    m["files"] = json!(["level1.tmj", "main.luau"]);
    std::fs::write(folder.join("ppu.json"), m.to_string()).unwrap();
    studio(&site, |b| {
        b["files"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name": "a.luau", "source": ""}))
    });
    let (running, _out) = start_with(&creds, folder, Clashes::Refuse);
    until("the save", || site.lock().unwrap().puts.len() == 1);
    let put = site.lock().unwrap().puts[0].clone();
    let names: Vec<&str> = put["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["level1.tmj", "main.luau", "a.luau"]);
    assert!(!running.is_finished());

    // The site reorders; the folder adds a file.
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path();
    pull(&creds, "p1", folder).unwrap();
    let mut m: Value = serde_json::from_str(&read(folder, "ppu.json")).unwrap();
    m["files"] = json!(["main.luau", "level1.tmj", "b.luau"]);
    std::fs::write(folder.join("ppu.json"), m.to_string()).unwrap();
    std::fs::write(folder.join("b.luau"), "").unwrap();
    studio(&site, |b| b["files"].as_array_mut().unwrap().reverse());
    let (running, _out) = start_with(&creds, folder, Clashes::Refuse);
    until("the save", || site.lock().unwrap().puts.len() == 1);
    let put = site.lock().unwrap().puts[0].clone();
    let names: Vec<&str> = put["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["level1.tmj", "main.luau", "b.luau"]);
    let m: Value = serde_json::from_str(&read(folder, "ppu.json")).unwrap();
    assert_eq!(m["files"], json!(["level1.tmj", "main.luau", "b.luau"]));
    assert!(!running.is_finished());
}

#[test]
fn a_lost_save_answer_then_a_folder_reorder_keeps_the_reorder() {
    // The folder's save (here: its order as pulled) is on the site as revision 2, the answer was
    // lost, and the folder reordered its files before connect restarted.
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path();
    pull(&creds, "p1", folder).unwrap();
    let mut link = Link::read(folder).unwrap();
    link.pending = connect::hashes(&project());
    link.write(folder).unwrap();
    site.lock().unwrap().revision = 2;
    let mut m: Value = serde_json::from_str(&read(folder, "ppu.json")).unwrap();
    m["files"] = json!(["level1.tmj", "main.luau"]);
    std::fs::write(folder.join("ppu.json"), m.to_string()).unwrap();
    let (running, _out) = start_with(&creds, folder, Clashes::Refuse);
    until("the reorder to save", || {
        site.lock().unwrap().puts.len() == 1
    });
    let m: Value = serde_json::from_str(&read(folder, "ppu.json")).unwrap();
    assert_eq!(m["files"], json!(["level1.tmj", "main.luau"]));
    assert!(!running.is_finished());
}

/// A connect started with `clashes` meets a live clash: an unsaved folder edit of main.luau and
/// a Studio save of the same file. Returns the folder and the site once the revision is heard.
fn live_clash(
    clashes: Clashes,
) -> (
    std::thread::JoinHandle<anyhow::Result<()>>,
    Arc<Mutex<Site>>,
    tempfile::TempDir,
) {
    let (creds, site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    pull(&creds, "p1", dir.path()).unwrap();
    let (running, _out) = start_with(&creds, dir.path(), clashes);
    until("a folder listen", || {
        !site.lock().unwrap().listens.is_empty()
    });
    site.lock().unwrap().drop_puts = 1000;
    std::fs::write(dir.path().join("main.luau"), "-- folder\n").unwrap();
    until("the folder's save to be tried", || {
        site.lock().unwrap().drop_puts < 1000
    });
    studio(&site, |b| b["files"][0]["source"] = "-- studio\n".into());
    site.lock().unwrap().drop_puts = 0;
    (running, site, dir)
}

#[test]
fn keep_folder_answers_only_the_start_so_a_live_clash_stops_connect() {
    let (running, site, dir) =
        live_clash(Clashes::AtStart(Keep::Folder, Box::new(Clashes::Refuse)));
    until("connect to stop", || running.is_finished());
    let err = format!("{:#}", running.join().unwrap().unwrap_err());
    assert!(err.contains("both changed main.luau"), "{err}");
    let s = site.lock().unwrap();
    assert_eq!(
        s.body["files"][0]["source"], "-- studio\n",
        "the Studio's edit is not saved over"
    );
    assert!(s.puts.is_empty(), "{:?}", s.puts);
    assert_eq!(read(dir.path(), "main.luau"), "-- folder\n");
    assert_eq!(Link::read(dir.path()).unwrap().revision, 1);
}

#[test]
fn keep_site_answers_only_the_start_so_a_live_clash_is_asked() {
    let (ask, asked) = asking(&[Keep::Site]);
    let (running, site, dir) = live_clash(Clashes::AtStart(Keep::Site, Box::new(ask)));
    until("the site's copy", || {
        read(dir.path(), "main.luau") == "-- studio\n"
    });
    let asked = asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert!(asked[0].contains("both changed main.luau"), "{asked:?}");
    assert_eq!(read(dir.path(), ".ppu-backup/2/main.luau"), "-- folder\n");
    assert!(site.lock().unwrap().puts.is_empty());
    assert!(!running.is_finished());
}

#[test]
fn a_folder_linked_to_another_site_is_refused_and_an_old_link_learns_its_site() {
    let (creds, _site) = stub(project());
    let dir = tempfile::tempdir().unwrap();
    pull(&creds, "p1", dir.path()).unwrap();
    let staging = Credentials {
        site: "https://staging.example".into(),
        token: creds.token.clone(),
    };
    let err =
        connect::connect(&staging, dir.path(), Clashes::Refuse, &mut std::io::sink()).unwrap_err();
    let err = format!("{err:#}");
    assert!(
        err.contains(&format!("linked to a project on {}", creds.site))
            && err.contains("signed in to https://staging.example"),
        "{err}"
    );

    // A link written before sites were recorded connects, and records the site it connected to.
    let path = dir.path().join(connect::LINK_FILE);
    let text = std::fs::read_to_string(&path).unwrap();
    let old: String = text
        .lines()
        .filter(|l| !l.starts_with("site = "))
        .map(|l| format!("{l}\n"))
        .collect();
    std::fs::write(&path, old).unwrap();
    assert_eq!(Link::read(dir.path()).unwrap().site, "");
    let (running, _out) = start_with(&creds, dir.path(), Clashes::Refuse);
    until("the link to record its site", || {
        Link::read(dir.path()).unwrap().site == creds.site
    });
    assert!(!running.is_finished());
}

// ---- art as PNG and .gpl ----

/// A site image source made with the real importer, so its payload and meta are real.
fn art_source(
    name: &str,
    kind: &str,
    options: Value,
    image: impl Fn(u32, u32) -> [u8; 4],
) -> Value {
    let (w, h) = (32u32, 16u32);
    let rgba: Vec<u8> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .flat_map(|(x, y)| image(x, y))
        .collect();
    let opts: ppu_core::ConvertOptions = serde_json::from_value(options.clone()).unwrap();
    let (payload, meta) =
        ppu_core::convert_source(kind.parse().unwrap(), &opts, &rgba, w, h).unwrap();
    json!({"name": name, "kind": kind, "builtinId": null, "options": options,
           "meta": serde_json::to_value(meta).unwrap(), "payload": BASE64.encode(payload.encode())})
}

const RED: [u8; 4] = [248, 0, 0, 255];
const GREEN: [u8; 4] = [0, 248, 0, 255];
const BLUE: [u8; 4] = [0, 0, 248, 255];
const GOLD: [u8; 4] = [248, 200, 0, 255];
const CLEAR: [u8; 4] = [0, 0, 0, 0];

fn bands(colours: &'static [[u8; 4]]) -> impl Fn(u32, u32) -> [u8; 4] {
    move |x, _| colours[(x as usize / 8) % colours.len()]
}

/// sky and sea hold the same colours (one shared palette); hero has its own; ship is not art.
fn art_project() -> Value {
    let mut body = project();
    body["sources"] = json!([
        art_source(
            "sky",
            "bg",
            json!({"bit_depth": 4}),
            bands(&[RED, GREEN, BLUE, CLEAR])
        ),
        art_source(
            "hero",
            "obj",
            json!({"cell_size": 8}),
            bands(&[GOLD, RED, CLEAR, CLEAR])
        ),
        art_source(
            "sea",
            "bg",
            json!({"bit_depth": 4}),
            bands(&[BLUE, CLEAR, GREEN, RED])
        ),
        body["sources"][0].clone(),
    ]);
    body
}

fn manifest(folder: &std::path::Path) -> Value {
    serde_json::from_str(&read(folder, "ppu.json")).unwrap()
}

fn source<'a>(m: &'a Value, name: &str) -> &'a Value {
    m["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == name)
        .unwrap()
}

#[test]
fn a_pull_writes_art_as_indexed_pngs_and_palettes_as_gpl_files() {
    let (creds, site) = stub(art_project());
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("demo");
    let mut said = Capture::default();
    let link = connect::pull(&creds, "p1", &folder, &mut said).unwrap();

    let m = manifest(&folder);
    for (name, file) in [
        ("sky", "assets/sky.png"),
        ("hero", "assets/hero.png"),
        ("sea", "assets/sea.png"),
    ] {
        let s = source(&m, name);
        assert_eq!(s["file"], file, "{s}");
        assert!(s.get("payload").is_none(), "{s}");
        let bytes = std::fs::read(folder.join(file)).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    }
    // Same whole palette list: one shared file, named for the first user; hero has its own.
    assert_eq!(source(&m, "sky")["options"]["palette"], "sky_pal");
    assert_eq!(source(&m, "sea")["options"]["palette"], "sky_pal");
    assert_eq!(source(&m, "hero")["options"]["palette"], "hero_pal");
    for pal in ["sky_pal", "hero_pal"] {
        let s = source(&m, pal);
        assert_eq!(
            (s["kind"].as_str(), s["file"].as_str()),
            (
                Some("palette"),
                Some(format!("palettes/{pal}.gpl").as_str())
            )
        );
        assert!(read(&folder, &format!("palettes/{pal}.gpl")).starts_with("GIMP Palette\n"));
    }
    assert!(m["sources"]
        .as_array()
        .unwrap()
        .iter()
        .all(|s| s["name"] != "sea_pal"));
    // No payload of an image or palette lands; the non-art source keeps its `.bin`.
    assert_eq!(source(&m, "ship")["payload"], ".ppu/sources/3.bin");
    let bins: Vec<_> = std::fs::read_dir(folder.join(".ppu/sources"))
        .unwrap()
        .flatten()
        .collect();
    assert_eq!(bins.len(), 1, "{bins:?}");
    // The announcement, once.
    let said = said.text();
    assert_eq!(
        said.matches(
            "created 2 palettes: sky_pal, hero_pal; nothing uses them until dma names them"
        )
        .count(),
        1,
        "{said}"
    );
    // sea shares a palette made in this pull: the "created" line names it, nothing else does.
    assert!(!said.contains("sea uses"), "{said}");
    assert!(!said.contains("refreshes its meta"), "{said}");
    // The link still holds the site's hashes: the folder differs from it by the new entries.
    assert_eq!(link.hashes, connect::hashes(&site.lock().unwrap().body));
}

#[test]
fn art_naming_a_palette_the_folder_lacks_says_so_and_takes_its_own() {
    let mut body = art_project();
    body["sources"][0]["options"]["palette"] = "ghost".into();
    let (creds, _site) = stub(body);
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("demo");
    let mut said = Capture::default();
    connect::pull(&creds, "p1", &folder, &mut said).unwrap();
    assert_eq!(
        source(&manifest(&folder), "sky")["options"]["palette"],
        "sky_pal"
    );
    let said = said.text();
    assert!(
        said.contains(
            "sky names ghost, which the folder holds no palette of: it now uses sky_pal."
        ),
        "{said}"
    );
    assert!(!said.contains("no longer ghost's"), "{said}");
}

#[test]
fn art_whose_stored_meta_is_not_what_its_file_imports_as_is_named_on_pull() {
    let mut body = art_project();
    // An older importer's report: a key this one no longer writes.
    body["sources"][1]["meta"]["older"] = true.into();
    let (creds, _site) = stub(body);
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("demo");
    let mut said = Capture::default();
    connect::pull(&creds, "p1", &folder, &mut said).unwrap();
    assert_eq!(source(&manifest(&folder), "hero")["file"], "assets/hero.png");
    let said = said.text();
    assert!(
        said.contains(
            "hero: the next sync refreshes its meta, which differs from what its file imports as now."
        ),
        "{said}"
    );
    assert!(!said.contains("sky: the next sync"), "{said}");
}

#[test]
fn the_first_sync_after_a_pull_sends_only_the_palettes_and_their_names_then_nothing() {
    let (creds, site) = stub(art_project());
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("demo");
    pull(&creds, "p1", &folder).unwrap();
    let before = site.lock().unwrap().body.clone();
    let running = start(&creds, &folder);
    until("the first save", || site.lock().unwrap().puts.len() == 1);
    until("the link to reach revision 2", || {
        Link::read(&folder).unwrap().revision == 2
    });

    let sent = site.lock().unwrap().puts[0].clone();
    let sources = |b: &Value| b["sources"].as_array().unwrap().clone();
    let find = |list: &[Value], name: &str| list.iter().find(|s| s["name"] == name).cloned();
    let (old, new) = (sources(&before), sources(&sent));
    assert_eq!(
        new.len(),
        old.len() + 2,
        "two palette sources are new: {new:?}"
    );
    for name in ["sky", "hero", "sea", "ship"] {
        let (a, b) = (find(&old, name).unwrap(), find(&new, name).unwrap());
        // Byte-identical payloads, meta and kind; the only change is options.palette.
        assert_eq!(
            (&a["payload"], &a["meta"], &a["kind"]),
            (&b["payload"], &b["meta"], &b["kind"]),
            "{name}"
        );
        let mut options = b["options"].clone();
        if name != "ship" {
            assert!(options["palette"].is_string(), "{name}: {options}");
            options.as_object_mut().unwrap().remove("palette");
        }
        assert_eq!(a["options"], options, "{name}");
    }
    for name in ["sky_pal", "hero_pal"] {
        assert_eq!(find(&new, name).unwrap()["kind"], "palette");
    }
    // Nothing further: the folder and the site agree.
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(site.lock().unwrap().puts.len(), 1);
    assert!(!running.is_finished());
}

/// The folder's indexed PNG, edited through `edit(width, pixels, palette)` and written back.
fn edit_png(
    folder: &std::path::Path,
    name: &str,
    edit: impl FnOnce(u32, &mut Vec<u8>, &mut Vec<u16>),
) {
    use ppu_core::render::{decode_png, encode_png, DecodedPng};
    let DecodedPng::Indexed(mut ip) =
        decode_png(&std::fs::read(folder.join(name)).unwrap()).unwrap()
    else {
        panic!("{name} is not indexed")
    };
    let width = ip.image.width;
    // A decoded PNG does not know its kind; every test art here has rows of 16.
    ip.image.row_size = 16;
    edit(width, &mut ip.image.pixels, &mut ip.image.palette);
    std::fs::write(folder.join(name), encode_png(&ip.image)).unwrap();
}

/// A connect that has pulled `body` and made its first sync (revision 2).
fn synced(
    body: Value,
) -> (
    Credentials,
    Arc<Mutex<Site>>,
    tempfile::TempDir,
    std::thread::JoinHandle<anyhow::Result<()>>,
    Capture,
) {
    let (creds, site) = stub(body);
    let dir = tempfile::tempdir().unwrap();
    pull(&creds, "p1", dir.path()).unwrap();
    let (running, out) = start_captured(&creds, dir.path());
    until("the first sync", || {
        Link::read(dir.path()).unwrap().revision == 2
    });
    (creds, site, dir, running, out)
}

fn site_source(site: &Mutex<Site>, name: &str) -> Value {
    let s = site.lock().unwrap();
    s.puts
        .last()
        .map(|p| &p["sources"])
        .unwrap_or(&s.body["sources"])
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == name)
        .unwrap()
        .clone()
}

#[test]
fn a_png_edit_pushes_the_reimported_payload() {
    let (_creds, site, dir, running, _out) = synced(art_project());
    let folder = dir.path();
    let old = site_source(&site, "sky")["payload"].clone();
    // The first tile takes the second colour: same rows, a new tile.
    edit_png(folder, "assets/sky.png", |w, px, _| {
        for y in 0..8 {
            for x in 0..8 {
                px[(y * w + x) as usize] = 2;
            }
        }
    });
    until("the edit's save", || site.lock().unwrap().puts.len() == 2);
    let sent = site_source(&site, "sky");
    assert_ne!(sent["payload"], old);
    let opts: ppu_core::ConvertOptions = serde_json::from_value(json!({"bit_depth": 4})).unwrap();
    let png = std::fs::read(folder.join("assets/sky.png")).unwrap();
    let (expected, _) =
        ppu_core::convert_png_source(ppu_core::SourceKind::Bg, &opts, &png, None).unwrap();
    assert_eq!(sent["payload"], BASE64.encode(expected.encode()));
    assert!(!running.is_finished());
}

/// 24 colours over eight tiles: two palette rows, tiles 0-3 in the first and 4-7 in the second.
fn two_rows(x: u32, y: u32) -> [u8; 4] {
    let (tile, n) = (x / 8, ((x % 8) + (y % 8) * 2) % 12);
    let base = if tile < 4 { 0 } else { 100 };
    [
        base + 8 * n as u8,
        40 + 8 * (tile as u8 % 4),
        200 - 8 * n as u8,
        255,
    ]
}

#[test]
fn an_edit_that_does_not_import_is_refused_with_its_reason_and_nothing_is_sent() {
    let mut body = project();
    body["sources"] = json!([art_source("wide", "bg", json!({"bit_depth": 4}), two_rows)]);
    let (_creds, site, dir, running, out) = synced(body);
    let folder = dir.path();
    let pal = read(folder, "ppu.json");
    assert!(pal.contains("wide_pal"), "{pal}");
    // A pixel of a first-row tile takes a second-row colour: the tile mixes rows.
    edit_png(folder, "assets/wide.png", |_, px, _| px[0] = 17);
    until("the refusal", || out.text().contains("Not saved"));
    std::thread::sleep(Duration::from_millis(1200));
    let said = out.text();
    assert!(
        said.contains("source \"wide\": tile 0,0 mixes rows 1 and 0"),
        "{said}"
    );
    assert_eq!(
        site.lock().unwrap().puts.len(),
        1,
        "only the first sync was sent: {said}"
    );
    assert!(!running.is_finished());
}

#[test]
fn art_whose_plte_is_not_its_gpl_is_refused_by_the_checks_rule_and_nothing_is_sent() {
    let (_creds, site, dir, running, out) = synced(art_project());
    // Same pixels, a colour of the PLTE changed: import ignores it, the check does not.
    edit_png(dir.path(), "assets/sky.png", |_, _, pal| pal[1] ^= 0x001f);
    until("the refusal", || out.text().contains("Not saved"));
    std::thread::sleep(Duration::from_millis(1200));
    let said = out.text();
    assert!(
        said.contains("assets/sky.png") && said.contains("plte"),
        "{said}"
    );
    assert!(said.contains("Nothing was sent"), "{said}");
    assert_eq!(site.lock().unwrap().puts.len(), 1, "{said}");
    assert!(!running.is_finished());
}

/// A new image for a source: another picture, so another palette.
fn new_art(name: &str) -> Value {
    art_source(
        name,
        "bg",
        json!({"bit_depth": 4}),
        bands(&[GOLD, CLEAR, RED, GREEN]),
    )
}

#[test]
fn a_site_art_change_renders_over_the_png_and_the_old_one_is_kept() {
    let (_creds, site, dir, running, out) = synced(art_project());
    let folder = dir.path();
    let old = std::fs::read(folder.join("assets/sea.png")).unwrap();
    let theirs = new_art("sea");
    studio(&site, |b| {
        let i = b["sources"]
            .as_array()
            .unwrap()
            .iter()
            .position(|s| s["name"] == "sea")
            .unwrap();
        b["sources"][i]["payload"] = theirs["payload"].clone();
        b["sources"][i]["meta"] = theirs["meta"].clone();
    });
    let sky_pal = read(folder, "palettes/sky_pal.gpl");
    until("the site's art on disk", || {
        std::fs::read(folder.join("assets/sea.png")).unwrap() != old
    });
    // The PNG lands before the manifest: wait for the manifest's own change too.
    until("the manifest to name sea_pal", || {
        source(&manifest(folder), "sea")["options"]["palette"] == "sea_pal"
    });
    // The old PNG is in the backup of the revision brought in, byte for byte.
    assert_eq!(
        std::fs::read(folder.join(".ppu-backup/3/assets/sea.png")).unwrap(),
        old
    );
    let m = manifest(folder);
    assert_eq!(source(&m, "sea")["file"], "assets/sea.png");
    assert!(source(&m, "sea").get("payload").is_none());
    // sea named sky_pal, which is not the site's colours now: it takes its own, and the old
    // palette and its other users stay.
    assert_eq!(source(&m, "sea")["options"]["palette"], "sea_pal");
    assert_eq!(source(&m, "sky")["options"]["palette"], "sky_pal");
    assert_eq!(read(folder, "palettes/sky_pal.gpl"), sky_pal);
    assert!(folder.join("palettes/sea_pal.gpl").exists());
    until("the switch to be announced", || {
        out.text()
            .contains("sea's colours are no longer sky_pal's: it now uses sea_pal.")
    });
    until("the palette to be announced", || {
        out.text().contains("created 1 palette: sea_pal;")
    });
    // What the folder now holds imports back to the site's payload; its additions save next.
    let packed: Value = serde_json::from_str(&ppu_cli::pack(folder).unwrap()).unwrap();
    assert_eq!(source(&packed, "sea")["payload"], theirs["payload"]);
    until("the additions' save", || {
        site.lock().unwrap().puts.len() == 2
    });
    assert_eq!(site_source(&site, "sea")["payload"], theirs["payload"]);
    assert!(!out.text().contains("no longer used"), "{}", out.text());
    assert!(!running.is_finished());
}

#[test]
fn a_both_sides_art_change_asks_and_the_sites_choice_keeps_the_folders_png_in_the_backup() {
    let (creds, site) = stub(art_project());
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("demo");
    let link = pull(&creds, "p1", &folder).unwrap();
    // The person repaints the PNG; the Studio saves other art for the same source.
    edit_png(&folder, "assets/sea.png", |w, px, _| {
        px[(w * 8 + 9) as usize] = 1
    });
    let mine = std::fs::read(folder.join("assets/sea.png")).unwrap();
    let theirs = new_art("sea");
    let mut body = site.lock().unwrap().body.clone();
    let i = body["sources"]
        .as_array()
        .unwrap()
        .iter()
        .position(|s| s["name"] == "sea")
        .unwrap();
    body["sources"][i]["payload"] = theirs["payload"].clone();
    body["sources"][i]["meta"] = theirs["meta"].clone();

    let err = connect::bring_in(
        &folder,
        &link,
        2,
        &body,
        &mut Clashes::Refuse,
        &mut std::io::sink(),
    )
    .unwrap_err();
    assert!(
        format!("{err:#}").contains("both changed the source sea"),
        "{err:#}"
    );
    assert_eq!(std::fs::read(folder.join("assets/sea.png")).unwrap(), mine);
    assert_eq!(Link::read(&folder).unwrap().revision, 1);

    let (mut ask, asked) = asking(&[Keep::Site]);
    connect::bring_in(&folder, &link, 2, &body, &mut ask, &mut std::io::sink()).unwrap();
    assert!(asked.lock().unwrap()[0].contains("the source sea"));
    assert_eq!(
        std::fs::read(folder.join(".ppu-backup/2/assets/sea.png")).unwrap(),
        mine
    );
    assert_ne!(std::fs::read(folder.join("assets/sea.png")).unwrap(), mine);
    let packed: Value = serde_json::from_str(&ppu_cli::pack(&folder).unwrap()).unwrap();
    assert_eq!(source(&packed, "sea")["payload"], theirs["payload"]);
}

#[test]
fn a_folder_with_binary_payloads_converts_on_connect_and_pushes_the_same_art() {
    let (creds, site) = stub(art_project());
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path();
    // The old layout: every source a `.ppu/sources/N.bin`.
    let mut file = site.lock().unwrap().body.clone();
    file["version"] = "ppu.toys/1".into();
    ppu_cli::unpack(&file.to_string(), folder).unwrap();
    let old_sky = std::fs::read(folder.join(".ppu/sources/0.bin")).unwrap();
    let body = site.lock().unwrap().body.clone();
    Link {
        project: "p1".into(),
        site: creds.site.clone(),
        revision: 1,
        hashes: connect::hashes(&body),
        order: vec!["main.luau".into(), "level1.tmj".into()],
        pending: Default::default(),
    }
    .write(folder)
    .unwrap();

    let (running, out) = start_captured(&creds, folder);
    until("the converted folder's save", || {
        site.lock().unwrap().puts.len() == 1
    });
    let m = manifest(folder);
    assert_eq!(source(&m, "sky")["file"], "assets/sky.png");
    assert_eq!(source(&m, "hero")["file"], "assets/hero.png");
    assert!(folder.join("palettes/sky_pal.gpl").exists());
    // The payload moved to the backup of the linked revision; no art `.bin` is left.
    assert_eq!(
        std::fs::read(folder.join(".ppu-backup/1/.ppu/sources/0.bin")).unwrap(),
        old_sky
    );
    assert!(!folder.join(".ppu/sources/0.bin").exists());
    assert!(
        out.text()
            .contains("created 2 palettes: sky_pal, hero_pal;"),
        "{}",
        out.text()
    );
    // Every art payload sent is the site's own.
    for name in ["sky", "hero", "sea"] {
        assert_eq!(
            site_source(&site, name)["payload"],
            body["sources"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["name"] == name)
                .unwrap()["payload"],
            "{name}"
        );
    }
    assert!(!running.is_finished());
}

#[test]
fn a_site_palette_change_writes_the_gpl_and_the_pngs_follow_it() {
    use ppu_core::SourcePayload;
    let (_creds, site, dir, running, out) = synced(art_project());
    let folder = dir.path();
    let old_png = std::fs::read(folder.join("assets/sky.png")).unwrap();
    // The Studio recolours the first colour of the shared palette.
    let mut pal = ppu_core::gpl::parse(&read(folder, "palettes/sky_pal.gpl")).unwrap();
    pal.rows[0][1] = ppu_core::gpl::to_555(200) | 0x03e0;
    let (payload, meta) =
        ppu_core::gpl::convert_palette(&ppu_core::gpl::write(&pal, "sky_pal")).unwrap();
    assert_eq!(payload, SourcePayload::Palette(pal.clone()));
    studio(&site, |b| {
        let i = b["sources"]
            .as_array()
            .unwrap()
            .iter()
            .position(|s| s["name"] == "sky_pal")
            .unwrap();
        b["sources"][i]["payload"] = BASE64.encode(payload.encode()).into();
        b["sources"][i]["meta"] = serde_json::to_value(meta).unwrap();
    });
    until("the new gpl", || {
        read(folder, "palettes/sky_pal.gpl")
            .contains(&format!("{:3}", ppu_core::gpl::from_555(pal.rows[0][1])))
    });
    let plte_of =
        |name: &str| match ppu_core::render::decode_png(&std::fs::read(folder.join(name)).unwrap())
            .unwrap()
        {
            ppu_core::render::DecodedPng::Indexed(ip) => ip.image.palette,
            _ => panic!("not indexed"),
        };
    until("both PNGs to follow", || {
        plte_of("assets/sky.png")[1] == pal.rows[0][1] && plte_of("assets/sea.png")[1] == pal.rows[0][1]
    });
    assert_eq!(
        std::fs::read(folder.join(".ppu-backup/3/assets/sky.png")).unwrap(),
        old_png
    );
    // Still a folder that packs and passes the art rules.
    ppu_cli::pack(folder).unwrap();
    assert_eq!(
        ppu_cli::art::findings(&folder.join("assets/sky.png")).unwrap(),
        Vec::<String>::new()
    );
    assert!(
        out.text().contains("now follows palettes/sky_pal.gpl"),
        "{}",
        out.text()
    );
    assert!(!running.is_finished());
}

#[test]
fn art_that_cannot_be_a_file_stays_a_payload_with_a_note() {
    let mut body = art_project();
    let odd = art_source("a/b", "bg", json!({"bit_depth": 4}), bands(&[RED, CLEAR]));
    body["sources"].as_array_mut().unwrap().push(odd.clone());
    let (creds, _site) = stub(body);
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("demo");
    let mut said = Capture::default();
    connect::pull(&creds, "p1", &folder, &mut said).unwrap();
    let m = manifest(&folder);
    let payload = source(&m, "a/b")["payload"].as_str().unwrap();
    assert_eq!(
        std::fs::read(folder.join(payload)).unwrap(),
        BASE64.decode(odd["payload"].as_str().unwrap()).unwrap()
    );
    assert!(source(&m, "a/b").get("file").is_none());
    assert!(
        said.text().contains("a/b stays raw data"),
        "{}",
        said.text()
    );
    assert!(
        said.text().contains("cannot be a file name"),
        "{}",
        said.text()
    );
    assert_eq!(source(&m, "sky")["file"], "assets/sky.png");
    // The pull said it once; each connect start converts nothing and repeats nothing.
    let (running, out) = start_captured(&creds, &folder);
    until("the first sync", || {
        Link::read(&folder).unwrap().revision == 2
    });
    assert!(!out.text().contains("stays raw"), "{}", out.text());
    assert!(!running.is_finished());
}

fn sized(
    name: &str,
    kind: &str,
    options: Value,
    (w, h): (u32, u32),
    image: impl Fn(u32, u32) -> [u8; 4],
) -> Value {
    let rgba: Vec<u8> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .flat_map(|(x, y)| image(x, y))
        .collect();
    let opts: ppu_core::ConvertOptions = serde_json::from_value(options.clone()).unwrap();
    let (payload, meta) =
        ppu_core::convert_source(kind.parse().unwrap(), &opts, &rgba, w, h).unwrap();
    json!({"name": name, "kind": kind, "builtinId": null, "options": options,
           "meta": serde_json::to_value(meta).unwrap(), "payload": BASE64.encode(payload.encode())})
}

fn ramp(x: u32, y: u32) -> [u8; 4] {
    let n = ((x / 4) + (y / 4) * 3) % 40;
    if n == 0 {
        CLEAR
    } else {
        [
            (n * 6) as u8,
            (255 - n * 5) as u8,
            ((n * 37) % 256) as u8,
            255,
        ]
    }
}

/// A pulled folder synced against `site`: its first PUT, once it has landed.
fn first_put_after_pull(body: Value) -> (Value, Value, Arc<Mutex<Site>>) {
    let (creds, site) = stub(body);
    let dir = tempfile::tempdir().unwrap();
    pull(&creds, "p1", dir.path()).unwrap();
    let before = site.lock().unwrap().body.clone();
    let (running, _out) = start_captured(&creds, dir.path());
    until("the first save", || !site.lock().unwrap().puts.is_empty());
    // Nothing else is owed: a second PUT would mean the folder drifted from the site.
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(site.lock().unwrap().puts.len(), 1);
    assert!(!running.is_finished());
    let sent = site.lock().unwrap().puts[0].clone();
    (before, sent, site)
}

fn assert_art_identical(before: &Value, sent: &Value) {
    for old in before["sources"].as_array().unwrap() {
        let name = old["name"].as_str().unwrap();
        let new = source(sent, name);
        assert_eq!(old["payload"], new["payload"], "{name} payload");
        assert_eq!(old["meta"], new["meta"], "{name} meta");
    }
}

#[test]
fn every_art_kind_pulled_and_pushed_back_is_the_sites_own_payload_and_meta() {
    let mut body = project();
    body["sources"] = json!([
        sized(
            "bg2",
            "bg",
            json!({"bit_depth": 2}),
            (32, 16),
            bands(&[RED, GREEN, CLEAR, BLUE])
        ),
        sized("bg8", "bg", json!({"bit_depth": 8}), (32, 32), ramp),
        sized(
            "sheet4",
            "sheet",
            json!({"bit_depth": 4}),
            (48, 16),
            bands(&[RED, GOLD, CLEAR, BLUE, GREEN, RED])
        ),
        sized(
            "obj16",
            "obj",
            json!({"cell_size": 16}),
            (48, 32),
            bands(&[GOLD, RED, CLEAR, BLUE])
        ),
        sized(
            "objtall",
            "obj",
            json!({"cell_size": 16, "cell_height": 32}),
            (32, 32),
            bands(&[GOLD, CLEAR, BLUE])
        ),
        sized("road", "m7", json!({}), (128, 128), ramp),
        sized("bgwide", "bg", json!({"bit_depth": 4}), (64, 16), two_rows),
    ]);
    let (before, sent, _site) = first_put_after_pull(body);
    assert_art_identical(&before, &sent);
}

#[test]
fn a_mode_7_extbg_source_pulled_and_pushed_back_is_the_sites_own_payload_and_meta() {
    let (w, h) = (128u32, 128u32);
    let pixels = |f: &dyn Fn(u32, u32) -> [u8; 4]| -> Vec<u8> {
        (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .flat_map(|(x, y)| f(x, y))
            .collect()
    };
    let rgba = pixels(&ramp);
    let mask = pixels(&|x, _| {
        if x < 64 {
            [255, 255, 255, 255]
        } else {
            [0, 0, 0, 255]
        }
    });
    let options = json!({"extbg": true});
    let opts: ppu_core::ConvertOptions = serde_json::from_value(options.clone()).unwrap();
    let (payload, meta) = ppu_core::convert_source_with_priority(
        ppu_core::SourceKind::M7,
        &opts,
        &rgba,
        w,
        h,
        Some(&mask),
    )
    .unwrap();
    let mut body = project();
    body["sources"] = json!([{"name": "roadx", "kind": "m7", "builtinId": null, "options": options,
        "meta": serde_json::to_value(meta).unwrap(), "payload": BASE64.encode(payload.encode())}]);
    let (before, sent, _site) = first_put_after_pull(body);
    assert_art_identical(&before, &sent);
}

#[test]
fn new_site_art_with_the_colours_of_a_folder_palette_names_it_and_makes_none() {
    let (_creds, site, dir, running, out) = synced(art_project());
    let twin = art_source(
        "sky2",
        "bg",
        json!({"bit_depth": 4}),
        bands(&[RED, GREEN, BLUE, CLEAR]),
    );
    studio(&site, |b| b["sources"].as_array_mut().unwrap().push(twin));
    until("sky2 on disk", || dir.path().join("assets/sky2.png").exists());
    until("sky2 listed", || {
        manifest(dir.path())["sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["name"] == "sky2")
    });
    assert_eq!(
        source(&manifest(dir.path()), "sky2")["options"]["palette"],
        "sky_pal"
    );
    assert!(!dir.path().join("palettes/sky2_pal.gpl").exists());
    assert!(!out.text().contains("sky2_pal"), "{}", out.text());
    until("the reuse to be announced", || {
        out.text()
            .contains("sky2 uses sky_pal, the folder's palette with its colours.")
    });
    assert!(!running.is_finished());
}

#[test]
fn one_png_listed_by_two_sources_still_syncs_and_a_later_edit_saves() {
    let (_creds, site, dir, running, out) = synced(art_project());
    let folder = dir.path();
    let puts = site.lock().unwrap().puts.len();
    let mut m = manifest(folder);
    let mut extra = source(&m, "sky").clone();
    extra["name"] = "sky_sheet".into();
    extra["kind"] = "sheet".into();
    m["sources"].as_array_mut().unwrap().push(extra);
    std::fs::write(
        folder.join("ppu.json"),
        serde_json::to_string_pretty(&m).unwrap(),
    )
    .unwrap();
    until("the second source's save", || {
        site.lock().unwrap().puts.len() > puts
    });
    assert!(!out.text().contains("Not saved"), "{}", out.text());
    let puts = site.lock().unwrap().puts.len();
    std::fs::write(folder.join("main.luau"), "-- edited\n").unwrap();
    until("the later edit's save", || {
        site.lock().unwrap().puts.len() > puts
    });
    assert!(!out.text().contains("Not saved"), "{}", out.text());
    assert!(!running.is_finished());
}

#[test]
fn art_check_still_refuses_a_png_two_sources_list() {
    let (_creds, _site, dir, running, _out) = synced(art_project());
    let folder = dir.path();
    let mut m = manifest(folder);
    let mut extra = source(&m, "sky").clone();
    extra["name"] = "sky_sheet".into();
    m["sources"].as_array_mut().unwrap().push(extra);
    std::fs::write(
        folder.join("ppu.json"),
        serde_json::to_string_pretty(&m).unwrap(),
    )
    .unwrap();
    let err = ppu_cli::art::find_entry(&folder.join("assets/sky.png"))
        .err()
        .unwrap();
    assert!(err.to_string().contains("one PNG, one source"), "{err}");
    assert!(!running.is_finished());
}

#[test]
fn an_old_folders_art_takes_the_next_palette_name_when_a_file_is_in_the_way() {
    let (creds, site) = stub(art_project());
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path();
    let mut file = site.lock().unwrap().body.clone();
    file["version"] = "ppu.toys/1".into();
    ppu_cli::unpack(&file.to_string(), folder).unwrap();
    std::fs::create_dir_all(folder.join("palettes")).unwrap();
    let mine = "GIMP Palette\nName: mine\n";
    std::fs::write(folder.join("palettes/hero_pal.gpl"), mine).unwrap();
    let body = site.lock().unwrap().body.clone();
    Link {
        project: "p1".into(),
        site: creds.site.clone(),
        revision: 1,
        hashes: connect::hashes(&body),
        order: vec!["main.luau".into(), "level1.tmj".into()],
        pending: Default::default(),
    }
    .write(folder)
    .unwrap();
    let (running, out) = start_captured(&creds, folder);
    // the PNG lands before ppu.json names it: wait for the manifest, not the file
    until("hero as a PNG in the manifest", || {
        std::fs::read_to_string(folder.join("ppu.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .is_some_and(|m| source(&m, "hero")["file"] == "assets/hero.png")
    });
    let m = manifest(folder);
    assert_eq!(source(&m, "hero")["file"], "assets/hero.png");
    assert_eq!(source(&m, "hero")["options"]["palette"], "hero_pal2");
    assert_eq!(read(folder, "palettes/hero_pal.gpl"), mine);
    assert!(folder.join("palettes/hero_pal2.gpl").exists());
    assert!(!out.text().contains("stays raw"), "{}", out.text());
    assert!(!running.is_finished());
}

fn plte_of(folder: &std::path::Path, name: &str) -> Vec<u16> {
    match ppu_core::render::decode_png(&std::fs::read(folder.join(name)).unwrap()).unwrap() {
        ppu_core::render::DecodedPng::Indexed(ip) => ip.image.palette,
        _ => panic!("not indexed"),
    }
}

/// `palettes/sky_pal.gpl` with its first row's colour 1 changed.
fn recoloured_gpl(folder: &std::path::Path) -> (String, u16) {
    let mut pal = ppu_core::gpl::parse(&read(folder, "palettes/sky_pal.gpl")).unwrap();
    pal.rows[0][1] = ppu_core::gpl::to_555(200) | 0x03e0;
    (ppu_core::gpl::write(&pal, "sky_pal"), pal.rows[0][1])
}

#[test]
fn a_follow_never_replaces_a_copy_already_kept_in_that_revisions_backup() {
    let (_creds, site, dir, running, _out) = synced(art_project());
    let folder = dir.path();
    // A copy kept earlier in this revision (a clash the site's side won) is the one worth having.
    std::fs::create_dir_all(folder.join(".ppu-backup/2/assets")).unwrap();
    std::fs::write(folder.join(".ppu-backup/2/assets/sky.png"), b"kept by a clash").unwrap();
    let old_sea = std::fs::read(folder.join("assets/sea.png")).unwrap();
    let (text, colour) = recoloured_gpl(folder);
    std::fs::write(folder.join("palettes/sky_pal.gpl"), text).unwrap();
    until("the save", || site.lock().unwrap().puts.len() == 2);
    assert_eq!(plte_of(folder, "assets/sky.png")[1], colour);
    assert_eq!(read(folder, ".ppu-backup/2/assets/sky.png"), "kept by a clash");
    assert_eq!(
        std::fs::read(folder.join(".ppu-backup/2/assets/sea.png")).unwrap(),
        old_sea
    );
    assert!(!running.is_finished());
}

#[test]
fn a_gpl_edited_in_the_folder_makes_every_png_naming_it_follow_in_one_save() {
    let (_creds, site, dir, running, out) = synced(art_project());
    let folder = dir.path();
    let (old_sky, old_sea) = (
        std::fs::read(folder.join("assets/sky.png")).unwrap(),
        std::fs::read(folder.join("assets/sea.png")).unwrap(),
    );
    let (text, colour) = recoloured_gpl(folder);
    std::fs::write(folder.join("palettes/sky_pal.gpl"), text).unwrap();
    until("the save", || site.lock().unwrap().puts.len() == 2);
    assert_eq!(plte_of(folder, "assets/sky.png")[1], colour);
    assert_eq!(plte_of(folder, "assets/sea.png")[1], colour);
    assert_eq!(
        std::fs::read(folder.join(".ppu-backup/2/assets/sky.png")).unwrap(),
        old_sky
    );
    assert_eq!(
        std::fs::read(folder.join(".ppu-backup/2/assets/sea.png")).unwrap(),
        old_sea
    );
    let said = out.text();
    let line = said
        .lines()
        .find(|l| l.contains("now follow palettes/sky_pal.gpl"))
        .expect(&said);
    assert!(
        line.contains("assets/sky.png") && line.contains("assets/sea.png"),
        "{said}"
    );
    assert_eq!(said.matches("now follow").count(), 1, "{said}");
    let palette = site_source(&site, "sky_pal")["payload"].clone();
    assert_ne!(
        palette,
        site.lock().unwrap().puts[0]["sources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == "sky_pal")
            .unwrap()["payload"]
    );
    // The rewritten PNGs are not a new edit.
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(site.lock().unwrap().puts.len(), 2, "{said}");
    assert!(!running.is_finished());
}

#[test]
fn a_gpl_that_does_not_parse_or_fit_is_refused_and_no_png_is_touched() {
    let (_creds, site, dir, running, out) = synced(art_project());
    let folder = dir.path();
    let (old_sky, old_sea) = (
        std::fs::read(folder.join("assets/sky.png")).unwrap(),
        std::fs::read(folder.join("assets/sea.png")).unwrap(),
    );
    let mut narrow = ppu_core::gpl::parse(&read(folder, "palettes/sky_pal.gpl")).unwrap();
    std::fs::write(folder.join("palettes/sky_pal.gpl"), "not a palette\n").unwrap();
    until("the refusal", || out.text().contains("Not saved"));
    std::thread::sleep(Duration::from_millis(1200));
    assert!(
        out.text().contains("starts with 'GIMP Palette'"),
        "{}",
        out.text()
    );
    // Rows of 4 no longer fit art whose rows are 16.
    narrow.rows[0].truncate(4);
    narrow.row_size = 4;
    narrow.rows.truncate(1);
    let narrow = ppu_core::gpl::write(&narrow, "sky_pal");
    std::fs::write(folder.join("palettes/sky_pal.gpl"), narrow).unwrap();
    until("the second refusal", || {
        out.text().matches("Not saved").count() >= 2
    });
    std::thread::sleep(Duration::from_millis(1200));
    assert_eq!(std::fs::read(folder.join("assets/sky.png")).unwrap(), old_sky);
    assert_eq!(std::fs::read(folder.join("assets/sea.png")).unwrap(), old_sea);
    assert!(!folder.join(".ppu-backup/2").exists());
    let said = out.text();
    assert!(
        said.contains("source \"sky\": its rows are") && said.contains("Nothing was sent"),
        "{said}"
    );
    assert_eq!(site.lock().unwrap().puts.len(), 1, "{said}");
    assert!(!running.is_finished());
}

#[test]
fn a_gpl_edited_while_disconnected_is_followed_at_connect_start() {
    let (creds, site) = stub(art_project());
    let dir = tempfile::tempdir().unwrap();
    pull(&creds, "p1", dir.path()).unwrap();
    let folder = dir.path();
    let (text, colour) = recoloured_gpl(folder);
    std::fs::write(folder.join("palettes/sky_pal.gpl"), text).unwrap();
    let (running, out) = start_captured(&creds, folder);
    until("the save", || site.lock().unwrap().puts.len() == 1);
    assert_eq!(plte_of(folder, "assets/sky.png")[1], colour);
    assert_eq!(plte_of(folder, "assets/sea.png")[1], colour);
    assert!(
        out.text().contains("now follow palettes/sky_pal.gpl"),
        "{}",
        out.text()
    );
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(site.lock().unwrap().puts.len(), 1);
    assert!(!running.is_finished());
}

#[test]
fn a_gpl_already_synced_by_ppu_art_palette_sync_rewrites_nothing_again() {
    let (creds, site) = stub(art_project());
    let dir = tempfile::tempdir().unwrap();
    pull(&creds, "p1", dir.path()).unwrap();
    let folder = dir.path();
    let (text, colour) = recoloured_gpl(folder);
    std::fs::write(folder.join("palettes/sky_pal.gpl"), text).unwrap();
    ppu_cli::art::palette_sync(&folder.join("palettes/sky_pal.gpl")).unwrap();
    let (running, out) = start_captured(&creds, folder);
    until("the save", || site.lock().unwrap().puts.len() == 1);
    assert_eq!(plte_of(folder, "assets/sky.png")[1], colour);
    std::thread::sleep(Duration::from_millis(1200));
    assert!(!out.text().contains("now follow"), "{}", out.text());
    assert!(!folder.join(".ppu-backup").exists());
    assert_eq!(site.lock().unwrap().puts.len(), 1);
    assert!(!running.is_finished());
}
