//! `ppu login`, `whoami` and `logout` against a stub site that answers like ppu.toys.

use ppu_cli::login;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

/// A site whose poll answers `polls` in turn, then the token; `"drop"` closes the connection
/// unanswered and `"503"` answers as a restarting site would. Returns its URL and the
/// requests it saw, as `METHOD path authorization body`.
fn stub(polls: &'static [&'static str]) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        let mut polls = polls.iter();
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
                ("POST", "/api/cli/login") => (200, r#"{"device_code":"dev1","user_code":"ABCD-2345","verification_uri":"https://site/cli","verification_uri_complete":"https://site/cli?code=ABCD-2345","expires_in":600,"interval":0}"#.to_string()),
                ("POST", "/api/cli/login/token") => match polls.next() {
                    Some(&"drop") => continue,
                    Some(&"503") => (503, "{}".into()),
                    Some(e) => (400, format!(r#"{{"error":"{e}"}}"#)),
                    None => (200, r#"{"access_token":"tok1","token_type":"Bearer"}"#.into()),
                },
                ("GET", "/api/me") if auth == "Bearer tok1" => (200, r#"{"handle":"ann"}"#.into()),
                ("GET", "/api/me") => (401, r#"{"error":"signed out"}"#.into()),
                ("DELETE", "/api/cli/token") => (204, String::new()),
                _ => (404, String::new()),
            };
            write!(stream, "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}", reply.len()).unwrap();
        }
    });
    (url, seen)
}

#[test]
fn login_polls_until_approved_then_whoami_and_logout() {
    let (site, seen) = stub(&["authorization_pending", "authorization_pending"]);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ppu/credentials.json");
    let mut out = Vec::new();

    let creds = login::login(&site, &path, &mut out).unwrap();
    let printed = String::from_utf8(out).unwrap();
    assert!(
        printed.contains("https://site/cli?code=ABCD-2345"),
        "{printed}"
    );
    assert!(printed.contains("ABCD-2345"));
    assert_eq!(
        creds,
        login::Credentials {
            site: site.clone(),
            token: "tok1".into()
        }
    );
    assert_eq!(login::load(&path).unwrap(), Some(creds));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    let polls = seen
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r.contains("/login/token"))
        .count();
    assert_eq!(polls, 3);

    let creds = login::load(&path).unwrap().unwrap();
    assert_eq!(login::whoami(&creds).unwrap(), "ann");

    let mut out = Vec::new();
    assert!(login::logout(&path, &mut out).unwrap());
    assert!(!path.exists());
    assert!(seen
        .lock()
        .unwrap()
        .iter()
        .any(|r| r.starts_with("DELETE /api/cli/token Bearer tok1")));
    assert!(!login::logout(&path, &mut Vec::new()).unwrap());
}

#[test]
fn a_denied_or_expired_login_saves_nothing() {
    for (answer, says) in [("access_denied", "denied"), ("expired_token", "expired")] {
        let polls: &'static [&'static str] = if answer == "access_denied" {
            &["access_denied"]
        } else {
            &["expired_token"]
        };
        let (site, _) = stub(polls);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.json");
        let err = login::login(&site, &path, &mut Vec::new()).unwrap_err();
        assert!(format!("{err:#}").contains(says), "{err:#}");
        assert!(!path.exists());
    }
}

#[test]
fn a_revoked_token_says_to_log_in_again() {
    let (site, _) = stub(&[]);
    let creds = login::Credentials {
        site,
        token: "old".into(),
    };
    let err = login::whoami(&creds).unwrap_err();
    assert!(format!("{err:#}").contains("ppu login"), "{err:#}");
}

#[test]
fn a_site_that_never_answers_times_out() {
    // accepts the connection and then says nothing
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let creds = login::Credentials {
        site: format!("http://{}", listener.local_addr().unwrap()),
        token: "tok".into(),
    };
    let started = std::time::Instant::now();
    let req = login::request_on(
        &login::agent(std::time::Duration::from_millis(300)),
        &creds,
        "GET",
        "/api/projects",
    );
    assert!(login::call(req, None).is_err());
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    drop(listener);
}

#[test]
fn a_dropped_poll_is_retried_and_signing_in_again_revokes_the_old_token() {
    let (site, seen) = stub(&["drop", "503", "authorization_pending"]);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("credentials.json");
    std::fs::write(&path, format!(r#"{{"site":"{site}","token":"old"}}"#)).unwrap();

    let creds = login::login(&site, &path, &mut Vec::new()).unwrap();
    assert_eq!(creds.token, "tok1");
    assert_eq!(login::load(&path).unwrap(), Some(creds));
    assert!(!dir.path().join("credentials.json.tmp").exists());
    let seen = seen.lock().unwrap();
    assert_eq!(
        seen.iter().filter(|r| r.contains("/login/token")).count(),
        4
    );
    assert!(
        seen.iter()
            .any(|r| r.starts_with("DELETE /api/cli/token Bearer old")),
        "{seen:?}"
    );
}
