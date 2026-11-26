//! `ppu build` against a stub site that answers `/api/rom` like ppu.toys: a pending job, then its result.

use ppu_cli::{cart, login::Credentials};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

/// A site whose `POST /api/rom` answers `first` (status, body); a 202 there names job `/api/rom/j1`,
/// which answers pending once and then `result`. Returns its URL and the requests it saw, as
/// `METHOD path authorization body`.
fn stub(first: (u16, &'static str), result: &'static str) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        let mut polls = 0;
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let mut parts = line.split_whitespace();
            let (method, path) = (
                parts.next().unwrap().to_string(),
                parts.next().unwrap().to_string(),
            );
            let (mut len, mut auth) = (0, String::new());
            loop {
                let mut h = String::new();
                reader.read_line(&mut h).unwrap();
                let h = h.trim_end();
                if h.is_empty() {
                    break;
                }
                let (k, v) = h.split_once(": ").unwrap();
                match k.to_ascii_lowercase().as_str() {
                    "content-length" => len = v.parse().unwrap(),
                    "authorization" => auth = v.to_string(),
                    _ => {}
                }
            }
            let mut body = vec![0; len];
            reader.read_exact(&mut body).unwrap();
            log.lock().unwrap().push(format!(
                "{method} {path} {auth} {}",
                String::from_utf8_lossy(&body)
            ));
            let (status, reply) = match (method.as_str(), path.as_str()) {
                ("POST", "/api/rom") => first,
                ("GET", "/api/rom/j1") if polls == 0 => {
                    polls += 1;
                    (202, r#"{"pending":true,"job":"/api/rom/j1"}"#)
                }
                ("GET", "/api/rom/j1") => (200, result),
                _ => (404, ""),
            };
            write!(stream, "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}", reply.len()).unwrap();
        }
    });
    (url, seen)
}

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    ppu_cli::new_project(&dir.path().join("toy"), false).unwrap();
    dir
}

const PENDING: (u16, &str) = (202, r#"{"pending":true,"job":"/api/rom/j1"}"#);

#[test]
fn a_pending_build_is_polled_until_its_image_arrives() {
    let (site, seen) = stub(PENDING, r#"{"ok":true,"diagnostics":[],"rom":"AAEC/w=="}"#);
    let creds = Credentials {
        site,
        token: "tok1".into(),
    };
    let dir = project();
    let mut out = Vec::new();
    let rom = cart::build(&creds, &dir.path().join("toy"), &mut out).unwrap();
    assert_eq!(rom, [0, 1, 2, 255]);
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 3, "{seen:?}");
    // the packed folder, as an async job, with the bearer
    let post: serde_json::Value =
        serde_json::from_str(seen[0].strip_prefix("POST /api/rom Bearer tok1 ").unwrap()).unwrap();
    assert_eq!(post["async"], true);
    assert!(post["files"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["name"] == "main.luau"));
    assert!(post["sources"].is_array());
    assert!(seen[1].starts_with("GET /api/rom/j1 Bearer tok1"));
    assert!(String::from_utf8(out)
        .unwrap()
        .contains("/api/rom/j1; waiting"));
}

#[test]
fn image_names_are_utc_stamps() {
    assert_eq!(cart::stamp(0), "19700101-000000");
    // 2024-02-29 is a leap day; 2026-11-21 23:10:45 UTC
    assert_eq!(cart::stamp(1_709_164_800), "20240229-000000");
    assert_eq!(cart::stamp(1_795_302_645), "20261121-231045");
}

#[test]
fn a_failed_build_prints_its_diagnostics_as_file_lines() {
    let (site, _) = stub(
        PENDING,
        r#"{"ok":false,"diagnostics":[{"severity":"error","code":"snes.bound","file":"game.luau","line":180,"message":"x has no bound"},{"severity":"warning","code":"snes.cost","file":"main.luau","line":3,"message":"slow"}]}"#,
    );
    let creds = Credentials {
        site,
        token: "tok1".into(),
    };
    let dir = project();
    let mut out = Vec::new();
    let err = cart::build(&creds, &dir.path().join("toy"), &mut out).unwrap_err();
    assert_eq!(
        err.to_string(),
        "the cartridge did not build: 1 error(s) above"
    );
    let out = String::from_utf8(out).unwrap();
    assert!(
        out.contains(
            "/toy/game.luau:180: error[snes.bound]: x has no bound\n  docs: ppu docs snes.bound\n"
        ),
        "{out}"
    );
    assert!(
        out.contains("/toy/main.luau:3: warning[snes.cost]: slow\n"),
        "{out}"
    );
}

#[test]
fn a_refused_build_says_why() {
    let (site, _) = stub(
        (429, r#"{"error":"Too many ROM builds; wait a minute."}"#),
        "",
    );
    let creds = Credentials {
        site: site.clone(),
        token: "tok1".into(),
    };
    let dir = project();
    let err = cart::build(&creds, &dir.path().join("toy"), &mut Vec::new()).unwrap_err();
    assert_eq!(
        err.to_string(),
        format!("{site} refused the build: Too many ROM builds; wait a minute.")
    );
}
