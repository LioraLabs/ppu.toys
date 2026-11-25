//! `ppu song check` and `ppu song roll`: the Studio's song_check report
//! (`ppu_core::song_check`) and a piano-roll PNG, for a song file or a song
//! source in a folder.

use anyhow::{anyhow, bail, Context, Result};
use ppu_core::song::Song;
use std::io::IsTerminal;
use std::path::Path;

/// A song to check or draw, with the samples its rows may name.
pub struct Loaded {
    pub name: String,
    pub song: Song,
    /// (source name, encoded source payload) of the folder's samples.
    pub samples: Vec<(String, Vec<u8>)>,
}

/// `<file.song.json>` or `<file.mid>`, or `[dir] <name>`: a song source in a folder's ppu.json (dir defaults to
/// the current one).
pub fn load(args: &[String]) -> Result<Loaded> {
    match args {
        [file] if file.ends_with(".song.json") || file.ends_with(".mid") => {
            let path = Path::new(file);
            let bytes = std::fs::read(path).with_context(|| format!("cannot read {file}"))?;
            let (song, stem) = match file.strip_suffix(".song.json") {
                Some(stem) => {
                    let text = String::from_utf8(bytes).context("a .song.json file is text")?;
                    (
                        ppu_core::song::from_json(&text).map_err(|e| anyhow!("{file}: {e}"))?,
                        stem,
                    )
                }
                None => (
                    ppu_core::import::midi::song_from_midi(&bytes)
                        .map_err(|e| anyhow!("{file}: {e}"))?,
                    file.trim_end_matches(".mid"),
                ),
            };
            let dir = path.parent().filter(|d| d.join(crate::MANIFEST).is_file());
            Ok(Loaded {
                name: stem.rsplit('/').next().unwrap_or(stem).into(),
                song,
                samples: dir.map(samples).transpose()?.unwrap_or_default(),
            })
        }
        [name] => from_folder(Path::new("."), name),
        [dir, name] => from_folder(Path::new(dir), name),
        _ => bail!("expected <file.song.json>, <file.mid> or [dir] <song source name>"),
    }
}

fn from_folder(dir: &Path, name: &str) -> Result<Loaded> {
    let manifest = crate::read_manifest(dir)?;
    let source = manifest
        .sources
        .iter()
        .find(|s| s.name == name && s.kind == "song")
        .with_context(|| {
            format!(
                "no song source {name:?} in {}",
                dir.join(crate::MANIFEST).display()
            )
        })?;
    let (bytes, _) =
        crate::author::source_bytes(dir, source).map_err(|e| anyhow!("source {name:?}: {e:#}"))?;
    let psng = match ppu_core::SourcePayload::decode(&bytes)
        .map_err(|e| anyhow!("source {name:?}: {e:?}"))?
    {
        ppu_core::SourcePayload::Song(psng) => psng,
        _ => bail!("source {name:?} is not a song"),
    };
    Ok(Loaded {
        name: name.into(),
        song: ppu_core::song::decode(&psng).map_err(|e| anyhow!("{name}: {e}"))?,
        samples: samples(dir)?,
    })
}

fn samples(dir: &Path) -> Result<Vec<(String, Vec<u8>)>> {
    crate::read_manifest(dir)?
        .sources
        .iter()
        .filter(|s| s.kind == "sample")
        .map(|s| {
            let (bytes, _) = crate::author::source_bytes(dir, s)
                .map_err(|e| anyhow!("source {:?}: {e:#}", s.name))?;
            Ok((s.name.clone(), bytes))
        })
        .collect()
}

/// Prints the song_check report (sentences on a terminal, JSON when piped); true when the song
/// does not play as written (over the voice budget or above C6).
pub fn check(song: &Loaded) -> Result<bool> {
    let samples: Vec<(&str, &[u8])> = song
        .samples
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect();
    let report = ppu_core::song_check::check(&song.song, &samples)
        .map_err(|e| anyhow!("{}: {e}", song.name))?;
    if std::io::stdout().is_terminal() {
        println!("{}", report.lines(&song.name).join("\n"));
    } else {
        let mut json = serde_json::to_value(&report)?;
        json["name"] = song.name.clone().into();
        json["failed"] = report.failed().into();
        println!("{json}");
    }
    Ok(report.failed())
}

pub fn write_png(roll: &ppu_core::song_roll::Roll, out: &Path) -> Result<()> {
    std::fs::write(out, ppu_core::song_roll::encode_png(roll))
        .with_context(|| format!("cannot write {}", out.display()))
}

/// `a..b`, both slots drawn.
pub fn parse_slots(value: &str) -> Result<(usize, usize)> {
    let (a, b) = value
        .split_once("..")
        .context("--slots is a..b (slot numbers, both drawn)")?;
    Ok((
        a.trim()
            .parse()
            .context("--slots is a..b (slot numbers, both drawn)")?,
        b.trim()
            .parse()
            .context("--slots is a..b (slot numbers, both drawn)")?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_a_mid_file_named_after_it() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("tune.mid");
        // ppq 480, one quarter-note C4.
        let mut mid = b"MThd\0\0\0\x06\0\0\0\x01\x01\xe0MTrk\0\0\0\x0d".to_vec();
        mid.extend([0, 0x90, 60, 100, 0x83, 0x60, 0x80, 60, 0, 0, 0xff, 0x2f, 0]);
        std::fs::write(&file, &mid).unwrap();
        let loaded = load(&[file.to_string_lossy().into_owned()]).unwrap();
        assert_eq!(loaded.name, "tune");
        assert_eq!(
            loaded.song,
            ppu_core::import::midi::song_from_midi(&mid).unwrap()
        );
        std::fs::write(&file, b"not midi").unwrap();
        let err = format!(
            "{:#}",
            load(&[file.to_string_lossy().into_owned()]).err().unwrap()
        );
        assert!(
            err.contains("tune.mid") && err.contains("not a MIDI file"),
            "{err}"
        );
    }

    #[test]
    fn parses_slots() {
        assert_eq!(parse_slots("2..4").unwrap(), (2, 4));
    }
}
