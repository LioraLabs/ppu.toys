//! AGENTS.md and the agent skills this ppu ships (ppu/docs/AGENTS.md and ppu/docs/skills),
//! written into a toy folder: AGENTS.md at the top, each skill twice as plain copies, under
//! `.agents/skills/` and `.claude/skills/` (no symlinks: Windows). `.ppu/skills.json` records
//! the version that wrote them and a hash per file, so an upgrade refreshes what ppu wrote and
//! keeps what the person edited.
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

include!(concat!(env!("OUT_DIR"), "/agent_files.rs"));

const RECORD: &str = ".ppu/skills.json";

#[derive(Serialize, Deserialize, Default)]
struct Record {
    version: String,
    files: BTreeMap<String, String>,
}

/// What a run did: the files it wrote and the ones it kept because the person edited them.
#[derive(Debug, Default, PartialEq)]
pub struct Skills {
    pub written: Vec<String>,
    pub kept: Vec<String>,
}

/// Each shipped file at its place in a toy folder.
fn placed() -> Vec<(String, &'static str)> {
    let mut out = Vec::new();
    for (path, text) in AGENT_FILES {
        match path.strip_prefix("skills/") {
            Some(rest) => {
                for root in [".agents/skills", ".claude/skills"] {
                    out.push((format!("{root}/{rest}"), *text));
                }
            }
            None => out.push((path.to_string(), *text)),
        }
    }
    out
}

/// Write AGENTS.md and the skills into a project folder. A run by the version that last wrote
/// them writes nothing, unless `reset`. Otherwise a file that is missing, or still as recorded,
/// is rewritten; one the person edited is kept (named in `kept`); `reset` overwrites every one.
pub fn write_skills(dir: &Path, reset: bool) -> Result<Skills> {
    write_version(dir, env!("CARGO_PKG_VERSION"), reset)
}

fn write_version(dir: &Path, version: &str, reset: bool) -> Result<Skills> {
    ensure!(
        dir.join(crate::MANIFEST).is_file(),
        "{} is not a ppu project (no {})",
        dir.display(),
        crate::MANIFEST
    );
    let hash = |bytes: &[u8]| crate::fnv(&[bytes]);
    // A record that does not parse is no record: every differing file then counts as edited.
    let old: Record = fs::read_to_string(dir.join(RECORD))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let mut done = Skills::default();
    if old.version == version && !reset {
        return Ok(done);
    }
    let mut new = Record {
        version: version.into(),
        files: BTreeMap::new(),
    };
    let shipped = placed();
    for (name, text) in &shipped {
        let path = dir.join(name);
        let now = fs::read(&path).ok().map(|b| hash(&b));
        let ours = now.is_none() || now.as_ref() == old.files.get(name);
        if now.as_deref() == Some(hash(text.as_bytes()).as_str()) {
            // Already the shipped text: nothing to write.
        } else if reset || ours {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("cannot create {}", parent.display()))?;
            }
            fs::write(&path, text).with_context(|| format!("cannot write {}", path.display()))?;
            done.written.push(name.clone());
        } else {
            // Keep the old record, so the next upgrade still sees the edit.
            if let Some(h) = old.files.get(name) {
                new.files.insert(name.clone(), h.clone());
            }
            done.kept.push(name.clone());
            continue;
        }
        new.files.insert(name.clone(), hash(text.as_bytes()));
    }
    // A file this ppu no longer ships goes, unless the person edited it.
    for (name, h) in &old.files {
        if !shipped.iter().any(|(n, _)| n == name)
            && fs::read(dir.join(name)).ok().map(|b| hash(&b)).as_ref() == Some(h)
        {
            fs::remove_file(dir.join(name)).with_context(|| format!("cannot remove {name}"))?;
        }
    }
    let path = dir.join(RECORD);
    fs::create_dir_all(path.parent().expect(".ppu"))?;
    fs::write(&path, serde_json::to_string_pretty(&new)? + "\n")
        .with_context(|| format!("cannot write {}", path.display()))?;
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("ppu.json"), "{}").unwrap();
        dir
    }

    const AGENTS: &str = "AGENTS.md";
    const SKILL: &str = ".claude/skills/making-toys/SKILL.md";

    #[test]
    fn a_new_folder_gets_agents_md_and_both_copies_of_every_skill() {
        let dir = folder();
        let done = write_version(dir.path(), "1", false).unwrap();
        assert!(done.kept.is_empty());
        assert_eq!(done.written.len(), placed().len());
        let skill = |root: &str| {
            fs::read_to_string(dir.path().join(root).join("making-toys/SKILL.md")).unwrap()
        };
        assert!(skill(".agents/skills").starts_with("---\nname: making-toys\n"));
        assert_eq!(skill(".agents/skills"), skill(".claude/skills"));
        assert!(dir.path().join(AGENTS).is_file());
        let record: Record =
            serde_json::from_str(&fs::read_to_string(dir.path().join(RECORD)).unwrap()).unwrap();
        assert_eq!(record.version, "1");
        assert_eq!(record.files.len(), placed().len());
    }

    #[test]
    fn the_same_version_writes_nothing_not_even_a_deleted_file() {
        let dir = folder();
        write_version(dir.path(), "1", false).unwrap();
        fs::remove_file(dir.path().join(AGENTS)).unwrap();
        assert_eq!(
            write_version(dir.path(), "1", false).unwrap(),
            Skills::default()
        );
        assert!(!dir.path().join(AGENTS).exists());
    }

    #[test]
    fn an_upgrade_rewrites_missing_and_untouched_files_and_keeps_edited_ones() {
        let dir = folder();
        write_version(dir.path(), "1", false).unwrap();
        // Untouched files as an older ppu wrote them: the record says so.
        let mut record: Record =
            serde_json::from_str(&fs::read_to_string(dir.path().join(RECORD)).unwrap()).unwrap();
        fs::write(dir.path().join(AGENTS), "old agents").unwrap();
        record
            .files
            .insert(AGENTS.into(), crate::fnv(&[b"old agents"]));
        fs::write(
            dir.path().join(RECORD),
            serde_json::to_string(&record).unwrap(),
        )
        .unwrap();
        fs::write(dir.path().join(SKILL), "my own notes").unwrap();
        fs::remove_file(dir.path().join(".agents/skills/making-toys/recipes.md")).unwrap();

        let done = write_version(dir.path(), "2", false).unwrap();
        assert_eq!(done.kept, [SKILL]);
        assert_eq!(
            done.written,
            [AGENTS, ".agents/skills/making-toys/recipes.md"]
        );
        assert_ne!(
            fs::read_to_string(dir.path().join(AGENTS)).unwrap(),
            "old agents"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join(SKILL)).unwrap(),
            "my own notes"
        );
        // Still edited at the next upgrade.
        assert_eq!(write_version(dir.path(), "3", false).unwrap().kept, [SKILL]);
    }

    #[test]
    fn a_file_already_there_before_ppu_wrote_any_is_kept() {
        let dir = folder();
        fs::write(dir.path().join(AGENTS), "the person's own").unwrap();
        assert_eq!(
            write_version(dir.path(), "1", false).unwrap().kept,
            [AGENTS]
        );
        assert_eq!(
            fs::read_to_string(dir.path().join(AGENTS)).unwrap(),
            "the person's own"
        );
    }

    #[test]
    fn reset_overwrites_edited_files_even_at_the_same_version() {
        let dir = folder();
        write_version(dir.path(), "1", false).unwrap();
        fs::write(dir.path().join(SKILL), "my own notes").unwrap();
        let done = write_version(dir.path(), "1", true).unwrap();
        assert_eq!(done.written, [SKILL]);
        assert!(done.kept.is_empty());
        assert!(fs::read_to_string(dir.path().join(SKILL))
            .unwrap()
            .starts_with("---\nname:"));
        assert_eq!(
            write_version(dir.path(), "2", false).unwrap(),
            Skills::default()
        );
    }

    #[test]
    fn a_file_no_longer_shipped_is_removed_unless_edited() {
        let dir = folder();
        write_version(dir.path(), "1", false).unwrap();
        let mut record: Record =
            serde_json::from_str(&fs::read_to_string(dir.path().join(RECORD)).unwrap()).unwrap();
        for (name, text) in [
            (".claude/skills/gone/SKILL.md", "old"),
            (".claude/skills/kept/SKILL.md", "old"),
        ] {
            fs::create_dir_all(dir.path().join(name).parent().unwrap()).unwrap();
            fs::write(dir.path().join(name), text).unwrap();
            record.files.insert(name.into(), crate::fnv(&[b"old"]));
        }
        fs::write(dir.path().join(".claude/skills/kept/SKILL.md"), "edited").unwrap();
        fs::write(
            dir.path().join(RECORD),
            serde_json::to_string(&record).unwrap(),
        )
        .unwrap();
        write_version(dir.path(), "2", false).unwrap();
        assert!(!dir.path().join(".claude/skills/gone/SKILL.md").exists());
        assert!(dir.path().join(".claude/skills/kept/SKILL.md").exists());
    }

    #[test]
    fn a_folder_without_a_manifest_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        assert!(write_version(dir.path(), "1", false).is_err());
        assert!(!dir.path().join(AGENTS).exists());
    }
}
