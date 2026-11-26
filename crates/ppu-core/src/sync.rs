//! Three-way sync of a project body, entry by entry: a hash per entry, and which entries a copy
//! takes from the other side or clashes on, given the hashes both last agreed on. Every client
//! (`ppu connect`, the Studio through wasm) asks this one place.
use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

/// FNV-1a 64 over `parts`, each followed by a zero byte, as 16 hex digits. Stored in folder
/// links: changing it rebases every link.
pub fn fnv(parts: &[&[u8]]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        for b in part.iter().chain(&[0u8]) {
            h = (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{h:016x}")
}

/// One hash per file (`file main.luau`) and per source (`source ship`) of a `ppu.toys/1` or
/// project body, and one per manifest field (`meta title`, `meta description`, `meta targetHz`,
/// `meta romSize`; the last two only when set). A source hashes its kind, options, meta and
/// payload, not `builtinId`.
pub fn hashes(body: &Value) -> BTreeMap<String, String> {
    let list = |k: &str| body[k].as_array().cloned().unwrap_or_default();
    let text = |v: &Value| v.as_str().unwrap_or_default().to_string();
    let mut out = BTreeMap::new();
    for k in ["title", "description"] {
        out.insert(format!("meta {k}"), fnv(&[text(&body[k]).as_bytes()]));
    }
    for k in ["targetHz", "romSize"] {
        if !body[k].is_null() {
            out.insert(format!("meta {k}"), fnv(&[body[k].to_string().as_bytes()]));
        }
    }
    for f in list("files") {
        out.insert(
            format!("file {}", text(&f["name"])),
            fnv(&[text(&f["source"]).as_bytes()]),
        );
    }
    for s in list("sources") {
        let (options, meta) = (
            crate::save::canonical_json(&s["options"]),
            crate::save::canonical_json(&s["meta"]),
        );
        let hash = fnv(&[
            text(&s["kind"]).as_bytes(),
            options.as_bytes(),
            meta.as_bytes(),
            text(&s["payload"]).as_bytes(),
        ]);
        out.insert(format!("source {}", text(&s["name"])), hash);
    }
    out
}

/// A new revision of the other side judged entry by entry against the base. Entries only this
/// side changed are in neither list: it keeps them and saves them next.
#[derive(Debug, Default, PartialEq, serde::Serialize)]
pub struct Compared {
    /// The other side changed these and this side did not: take its copy (or remove it).
    pub take: BTreeSet<String>,
    /// Both sides changed these, differently.
    pub clashes: Vec<String>,
}

/// The one place base, mine and theirs meet. An absent hash is an absent entry.
pub fn compare(
    base: &BTreeMap<String, String>,
    mine: &BTreeMap<String, String>,
    theirs: &BTreeMap<String, String>,
) -> Compared {
    let keys: BTreeSet<&String> = base
        .keys()
        .chain(mine.keys())
        .chain(theirs.keys())
        .collect();
    let mut out = Compared::default();
    for key in keys {
        let (b, m, t) = (base.get(key), mine.get(key), theirs.get(key));
        // They did not change it, or both sides already agree.
        if t == b || t == m {
            continue;
        }
        if m == b {
            out.take.insert(key.clone());
        } else {
            out.clashes.push(key.clone());
        }
    }
    out
}
