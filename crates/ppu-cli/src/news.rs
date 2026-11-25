//! The one-line release notice `ppu login` and `ppu connect` print on stderr: the site's newest
//! What's new entry, once per entry. It never fails or holds up the command: an unreachable
//! site, a slow one or an odd answer prints nothing.

use std::io::Write;
use std::path::Path;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(2);

/// Fetches `site`'s newest entry and prints it to `err` unless `seen` (a file holding the last
/// heading printed) already names it; then remembers it there.
pub fn notice(site: &str, seen: &Path, err: &mut dyn Write) {
    let Some((heading, url)) = fetch(site) else {
        return;
    };
    if std::fs::read_to_string(seen).is_ok_and(|s| s.trim_end() == heading) {
        return;
    }
    if let Some(dir) = seen.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // remembered first: a notice that cannot be remembered would print every run
    if std::fs::write(seen, &heading).is_ok() {
        let _ = writeln!(err, "New on ppu.toys: {heading}  {url}");
    }
}

fn fetch(site: &str) -> Option<(String, String)> {
    let res = ureq::AgentBuilder::new()
        .timeout(TIMEOUT)
        .build()
        .get(&format!("{site}/api/news"))
        .call()
        .ok()?;
    let v: serde_json::Value = serde_json::from_str(&res.into_string().ok()?).ok()?;
    let line = |k: &str| {
        v[k].as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty() && s.len() <= 200 && !s.contains(char::is_control))
            .map(String::from)
    };
    Some((line("heading")?, line("url")?))
}

#[cfg(test)]
mod tests {
    use super::notice;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// A site that answers every request with `body` as JSON; returns its URL.
    fn site(body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for mut s in listener.incoming().flatten() {
                let mut buf = [0; 4096];
                let _ = s.read(&mut buf);
                let _ = write!(
                    s,
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        url
    }

    fn run(site: &str, seen: &std::path::Path) -> String {
        let mut err = Vec::new();
        notice(site, seen, &mut err);
        String::from_utf8(err).unwrap()
    }

    #[test]
    fn prints_each_new_heading_once() {
        let dir = tempfile::tempdir().unwrap();
        let seen = dir.path().join("ppu/news-seen");
        let a = site(r#"{"heading":"2026-11-15 — Tiled worlds","url":"https://ppu.toys/changes"}"#);
        assert_eq!(
            run(&a, &seen),
            "New on ppu.toys: 2026-11-15 — Tiled worlds  https://ppu.toys/changes\n"
        );
        assert_eq!(run(&a, &seen), "");
        let b = site(r#"{"heading":"2026-12-01 — Next","url":"https://ppu.toys/changes"}"#);
        assert!(run(&b, &seen).contains("2026-12-01 — Next"));
        assert_eq!(run(&b, &seen), "");
    }

    #[test]
    fn says_nothing_when_the_site_fails_or_answers_oddly() {
        let dir = tempfile::tempdir().unwrap();
        let seen = dir.path().join("news-seen");
        // nothing listens on a port just freed
        let closed = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            format!("http://{}", l.local_addr().unwrap())
        };
        assert_eq!(run(&closed, &seen), "");
        assert_eq!(run(&site("<html>not json</html>"), &seen), "");
        assert_eq!(run(&site(r#"{"heading":""}"#), &seen), "");
        assert_eq!(
            run(&site(r#"{"heading":"a\u001b[2J","url":"u"}"#), &seen),
            ""
        );
        assert!(!seen.exists());
    }
}
