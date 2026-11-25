//! The ppu.toys docs as a graph of atomic notes: one claim each, with one snippet.
//!
//! Hand notes are the Markdown files in ppu/docs/notes: YAML-style frontmatter (`id`, `kind`,
//! `title`, `codes`, `fixture`), a short body, at most one fenced snippet, and links only inline as
//! `[[id|text]]` or `[[id]]`. API notes are generated from the Luau declarations (ppu.d.luau): the
//! comment block above a declaration is its prose, so the declarations stay the one home of API
//! text. Example lines come from the templates' calls. A diagnostic code is a note id.
//!
//! One search-and-walk, used by `ppu docs`, the server's MCP `docs()` and the site.

use std::collections::BTreeSet;
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/embedded.rs"));

/// What a note is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Api,
    Rule,
    Recipe,
    Gotcha,
    Diagnostic,
    Concept,
}

impl Kind {
    const ALL: [Kind; 6] = [
        Kind::Api,
        Kind::Rule,
        Kind::Recipe,
        Kind::Gotcha,
        Kind::Diagnostic,
        Kind::Concept,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Api => "api",
            Kind::Rule => "rule",
            Kind::Recipe => "recipe",
            Kind::Gotcha => "gotcha",
            Kind::Diagnostic => "diagnostic",
            Kind::Concept => "concept",
        }
    }

    fn parse(s: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.name() == s)
    }
}

#[derive(Clone, Debug)]
pub struct Note {
    pub id: String,
    pub kind: Kind,
    /// A claim for hand notes; the declaration for API notes.
    pub title: String,
    /// Diagnostic codes this note explains.
    pub codes: Vec<String>,
    /// The template whose ppu.json and assets the snippet runs in.
    pub fixture: Option<String>,
    /// The Markdown after the frontmatter (an API note's declaration prose).
    pub body: String,
}

/// An inline link: `[[target|text]]`, or `[[target]]` with the target as its text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub target: String,
    pub text: String,
}

/// A fenced block in a note's body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fence {
    pub tag: String,
    pub body: String,
}

impl Fence {
    /// The toy files a luau fence writes: it splits on `-- <name>.luau` header lines (text after the
    /// name, like `-- play.luau, init`, is a remark), each header line kept as its file's first line.
    /// Lines before the first header, or a fence with none, are main.luau. A name repeated appends.
    pub fn files(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = vec![];
        let mut current = "main.luau".to_string();
        for line in self.body.lines() {
            if let Some(name) = header(line) {
                current = name;
            }
            let i = match out.iter().position(|(n, _)| *n == current) {
                Some(i) => i,
                None => {
                    out.push((current.clone(), String::new()));
                    out.len() - 1
                }
            };
            out[i].1 += line;
            out[i].1 += "\n";
        }
        // Blank lines between files belong to neither.
        out.retain(|(_, text)| !text.trim().is_empty());
        for (_, text) in &mut out {
            *text = text.trim().to_string() + "\n";
        }
        out
    }
}

/// The file a `-- <name>.luau` header line names.
fn header(line: &str) -> Option<String> {
    let rest = line.strip_prefix("-- ")?;
    let stem = rest.split(".luau").next()?;
    let ok = !stem.is_empty()
        && rest.len() > stem.len()
        && stem.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    let after = &rest[(stem.len() + 5).min(rest.len())..];
    (ok && (after.is_empty() || after.starts_with([' ', ',', ':']))).then(|| format!("{stem}.luau"))
}

impl Note {
    pub fn links(&self) -> Vec<Link> {
        let mut out = vec![];
        let mut rest = self.body.as_str();
        while let Some(start) = rest.find("[[") {
            let Some(end) = rest[start..].find("]]") else {
                break;
            };
            let inner = &rest[start + 2..start + end];
            let (target, text) = inner.split_once('|').unwrap_or((inner, inner));
            out.push(Link {
                target: target.trim().to_string(),
                text: text.to_string(),
            });
            rest = &rest[start + end + 2..];
        }
        out
    }

    pub fn fences(&self) -> Vec<Fence> {
        let mut out = vec![];
        let mut lines = self.body.lines();
        while let Some(line) = lines.next() {
            let Some(tag) = line.trim_start().strip_prefix("```") else {
                continue;
            };
            let body: Vec<&str> = lines
                .by_ref()
                .take_while(|l| !l.trim_start().starts_with("```"))
                .collect();
            out.push(Fence {
                tag: tag.trim().to_string(),
                body: body.join("\n") + "\n",
            });
        }
        out
    }

    /// The note's one snippet, if it has one.
    pub fn snippet(&self) -> Option<Fence> {
        self.fences().into_iter().next()
    }
}

/// Parse a hand note from its file name and text.
pub fn parse_note(file: &str, text: &str) -> Result<Note, String> {
    let text = text.replace("\r\n", "\n");
    let stem = file.strip_suffix(".md").unwrap_or(file);
    let rest = text
        .strip_prefix("---\n")
        .ok_or("a note opens with --- frontmatter")?;
    let (head, body) = rest
        .split_once("\n---\n")
        .ok_or("frontmatter has no closing ---")?;
    let (mut id, mut kind, mut title, mut codes, mut fixture) = (None, None, None, None, None);
    for line in head.lines() {
        let (k, v) = line
            .split_once(':')
            .ok_or_else(|| format!("frontmatter line {line:?} is not `key: value`"))?;
        let v = v.trim();
        let slot = match k {
            "id" => &mut id,
            "kind" => &mut kind,
            "title" => &mut title,
            "codes" => &mut codes,
            "fixture" => &mut fixture,
            _ => {
                return Err(format!(
                    "unknown frontmatter key {k:?} (id, kind, title, codes, fixture)"
                ))
            }
        };
        if slot.replace(v.to_string()).is_some() {
            return Err(format!("frontmatter repeats {k}"));
        }
    }
    let id = id.ok_or("frontmatter needs id")?;
    if id != stem {
        return Err(format!("id {id:?} is not the file name {stem:?}"));
    }
    let kind = kind.ok_or("frontmatter needs kind")?;
    let kind = match Kind::parse(&kind) {
        Some(Kind::Api) => {
            return Err("api notes are generated from ppu.d.luau, not written".into())
        }
        Some(k) => k,
        None => {
            return Err(format!(
                "unknown kind {kind:?} (rule, recipe, gotcha, diagnostic, concept)"
            ))
        }
    };
    let codes = match codes {
        None => vec![],
        Some(list) => list
            .strip_prefix('[')
            .and_then(|l| l.strip_suffix(']'))
            .ok_or_else(|| format!("codes {list:?} is not a [list]"))?
            .split(',')
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .map(String::from)
            .collect(),
    };
    let note = Note {
        id,
        kind,
        title: title
            .filter(|t| !t.is_empty())
            .ok_or("frontmatter needs a title")?,
        codes,
        fixture,
        body: body.trim().to_string() + "\n",
    };
    if note.fences().len() > 1 {
        return Err("a note has at most one snippet".into());
    }
    Ok(note)
}

/// A chapter as a map: an ordered list of note ids the site reads as one page of prose.
#[derive(Clone, Debug)]
pub struct Map {
    pub id: String,
    pub title: String,
    pub description: String,
    /// Where the site lists it: maps sort by `order`, then id.
    pub order: u32,
    pub notes: Vec<String>,
}

/// Parse a map: frontmatter (`id`, `title`, `description`, `order`), then one `- [[note id]]` line per note.
pub fn parse_map(file: &str, text: &str) -> Result<Map, String> {
    let text = text.replace("\r\n", "\n");
    let stem = file.strip_suffix(".md").unwrap_or(file);
    let rest = text
        .strip_prefix("---\n")
        .ok_or("a map opens with --- frontmatter")?;
    let (head, body) = rest
        .split_once("\n---\n")
        .ok_or("frontmatter has no closing ---")?;
    let (mut id, mut title, mut description, mut order) = (None, None, None, None);
    for line in head.lines() {
        let (k, v) = line
            .split_once(':')
            .ok_or_else(|| format!("frontmatter line {line:?} is not `key: value`"))?;
        let slot = match k {
            "id" => &mut id,
            "title" => &mut title,
            "description" => &mut description,
            "order" => &mut order,
            _ => {
                return Err(format!(
                    "unknown frontmatter key {k:?} (id, title, description, order)"
                ))
            }
        };
        if slot.replace(v.trim().to_string()).is_some() {
            return Err(format!("frontmatter repeats {k}"));
        }
    }
    let id = id.ok_or("frontmatter needs id")?;
    if id != stem {
        return Err(format!("id {id:?} is not the file name {stem:?}"));
    }
    let mut notes = vec![];
    for line in body.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let note = line
            .strip_prefix("- [[")
            .and_then(|l| l.strip_suffix("]]"))
            .filter(|n| !n.contains('|'))
            .ok_or_else(|| format!("map line {line:?} is not `- [[note id]]`"))?;
        notes.push(note.trim().to_string());
    }
    if notes.is_empty() {
        return Err("a map lists at least one note".into());
    }
    Ok(Map {
        id,
        title: title
            .filter(|t| !t.is_empty())
            .ok_or("frontmatter needs a title")?,
        description: description.unwrap_or_default(),
        order: order
            .ok_or("frontmatter needs an order (a whole number: where the site lists the map)")?
            .parse()
            .map_err(|_| "order is a whole number")?,
        notes,
    })
}

/// API notes from Luau declarations: each `declare function`, each global `declare name:`, the
/// fields of a declared table (`sprites.group`) and each `@ppu/<name>` library and its fields
/// (`hud.text`), the comment block above a declaration as its prose.
pub fn api_notes(declarations: &str) -> Vec<Note> {
    let mut out = vec![];
    let mut prose: Vec<String> = vec![];
    let mut after_comment = false;
    // The table whose fields are being read: (prefix for field ids, its prose), and brace depth.
    let mut owner: Option<(String, Vec<String>)> = None;
    let mut depth = 0i32;
    let mut field_prose: Vec<String> = vec![];
    let note = |id: String, title: String, prose: &[String]| Note {
        id,
        kind: Kind::Api,
        title,
        codes: vec![],
        fixture: None,
        body: prose.join("\n") + "\n",
    };
    for line in declarations.lines() {
        let trimmed = line.trim();
        let opens = line.matches('{').count() as i32 - line.matches('}').count() as i32;
        if depth > 0 {
            if let Some((prefix, owner_prose)) = &owner {
                if let Some(c) = trimmed.strip_prefix("--") {
                    field_prose.push(c.trim().to_string());
                } else if depth == 1 {
                    if let Some((name, ty)) = field(trimmed) {
                        let p = if field_prose.is_empty() {
                            owner_prose
                        } else {
                            &field_prose
                        };
                        let id = format!("{prefix}.{name}");
                        out.push(note(
                            id.clone(),
                            format!("{id}: {}", ty.trim_end_matches(',')),
                            p,
                        ));
                    }
                    field_prose.clear();
                }
            }
            depth += opens;
            if depth <= 0 {
                depth = 0;
                owner = None;
            }
            continue;
        }
        if trimmed.is_empty() || (trimmed.starts_with("--") && trimmed.ends_with("----")) {
            prose.clear();
            after_comment = false;
            continue;
        }
        if let Some(c) = trimmed.strip_prefix("--") {
            if !after_comment {
                prose.clear();
            }
            prose.push(c.trim().to_string());
            after_comment = true;
            continue;
        }
        after_comment = false;
        if let Some(rest) = trimmed.strip_prefix("declare function ") {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            out.push(note(name, rest.to_string(), &prose));
        } else if let Some(rest) = trimmed.strip_prefix("declare ") {
            if let Some((name, _)) = field(rest) {
                let title = if opens > 0 {
                    format!("{name}: {{...}}")
                } else {
                    rest.to_string()
                };
                out.push(note(name.to_string(), title, &prose));
                if opens > 0 {
                    owner = Some((name.to_string(), prose.clone()));
                }
            }
        } else if let Some(rest) = trimmed.strip_prefix("export type ppu_") {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            let lib = format!("@ppu/{name}");
            if prose
                .first()
                .is_some_and(|p| p.starts_with(&format!("{lib}:")))
            {
                out.push(note(lib.clone(), format!("require(\"{lib}\")"), &prose));
                if opens > 0 {
                    owner = Some((name, prose.clone()));
                }
            }
        }
        depth = opens.max(0);
        if depth == 0 {
            owner = None;
        }
        field_prose.clear();
    }
    out
}

/// `name: type` with an identifier name.
fn field(s: &str) -> Option<(&str, &str)> {
    let (name, ty) = s.split_once(':')?;
    let ok = !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_');
    ok.then_some((name, ty.trim()))
}

pub struct Graph {
    pub notes: Vec<Note>,
    /// The site's chapters, in `order`.
    pub maps: Vec<Map>,
    /// (path from the repository root, text) of every template Luau file, for example lines.
    templates: Vec<(&'static str, &'static str)>,
}

/// Every embedded note and map and the generated API notes, with every problem found: a note or map
/// that does not parse, an id two notes share, or a map entry that names no note. The notes test reports all of them at once.
pub fn load() -> (Graph, Vec<String>) {
    let mut errors = vec![];
    let mut notes: Vec<Note> = NOTES
        .iter()
        .filter_map(|(file, text)| {
            parse_note(file, text)
                .map_err(|e| errors.push(format!("notes/{file}: {e}")))
                .ok()
        })
        .collect();
    notes.extend(api_notes(ppu_core::LUAU_DECLARATIONS));
    let mut seen = BTreeSet::new();
    for n in &notes {
        if !seen.insert(n.id.as_str()) {
            errors.push(format!("two notes have the id {:?}", n.id));
        }
    }
    let mut maps: Vec<Map> = MAPS
        .iter()
        .filter_map(|(file, text)| {
            parse_map(file, text)
                .map_err(|e| errors.push(format!("maps/{file}: {e}")))
                .ok()
        })
        .collect();
    maps.sort_by(|a, b| (a.order, &a.id).cmp(&(b.order, &b.id)));
    for m in &maps {
        for id in &m.notes {
            if !seen.contains(id.as_str()) {
                errors.push(format!("maps/{}.md: [[{id}]] names no note", m.id));
            }
        }
    }
    let graph = Graph {
        notes,
        maps,
        templates: TEMPLATES.to_vec(),
    };
    (graph, errors)
}

/// The embedded graph. Notes that fail to parse are left out (the notes test fails on them).
pub fn graph() -> &'static Graph {
    static GRAPH: OnceLock<Graph> = OnceLock::new();
    GRAPH.get_or_init(|| load().0)
}

/// Who reads a note: its Next line names the check that reader can run.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reader {
    /// `ppu docs` in a shell: `ppu check .`.
    Shell,
    /// The MCP `docs` tool: an agent with no shell, so the `snes_check` tool.
    Mcp,
}

impl Reader {
    fn check(self) -> &'static str {
        match self {
            Reader::Shell => "ppu check .",
            Reader::Mcp => "snes_check",
        }
    }

    /// What a link to a shell command's note reads as: an MCP agent has the tool instead.
    fn link_text<'a>(self, id: &str, text: &'a str) -> &'a str {
        const TOOLS: &[(&str, &str)] = &[
            ("ppu-song-check", "song_check"),
            ("ppu-song-roll", "read_song with roll"),
            ("ppu-render-wav", "check (its audio line)"),
            ("ppu-sfx", "add_sample_source with sfx"),
        ];
        match self {
            Reader::Mcp => TOOLS
                .iter()
                .find(|(n, _)| *n == id)
                .map_or(text, |(_, t)| t),
            Reader::Shell => text,
        }
    }
}

const STOP: &[&str] = &[
    "a", "an", "the", "to", "of", "in", "on", "for", "and", "or", "is", "it", "how", "do", "i",
    "my", "with", "what", "why", "does", "can", "from", "at", "be", "this", "that",
];

fn tokens(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty() && !STOP.contains(w))
        .map(stem)
        .collect()
}

/// Folds plural and verb endings so `writing` meets `writes`.
fn stem(w: &str) -> String {
    for end in ["ing", "es", "ed", "s"] {
        if w.len() > end.len() + 3 {
            if let Some(s) = w.strip_suffix(end) {
                return s.trim_end_matches('e').to_string();
            }
        }
    }
    w.trim_end_matches('e').to_string()
}

impl Graph {
    pub fn get(&self, id: &str) -> Option<&Note> {
        self.notes.iter().find(|n| n.id == id)
    }

    pub fn by_code(&self, code: &str) -> Option<&Note> {
        self.notes
            .iter()
            .find(|n| n.codes.iter().any(|c| c == code))
    }

    /// The notes that link to `id`.
    pub fn backlinks(&self, id: &str) -> Vec<&Note> {
        self.notes
            .iter()
            .filter(|n| n.links().iter().any(|l| l.target == id))
            .collect()
    }

    /// The API notes whose names a note's snippet calls (what it requires).
    pub fn uses(&self, note: &Note) -> Vec<&Note> {
        let Some(snippet) = note.snippet() else {
            return vec![];
        };
        self.notes
            .iter()
            .filter(|n| n.kind == Kind::Api && n.id != note.id && calls(&snippet.body, &n.id))
            .collect()
    }

    /// `path:line: text` for every template line that uses API note `id`.
    pub fn examples(&self, id: &str) -> Vec<String> {
        let mut out = vec![];
        for (path, text) in &self.templates {
            for (n, line) in text.lines().enumerate() {
                if !line.trim_start().starts_with("--") && calls(line, id) {
                    out.push(format!("{path}:{}: {}", n + 1, line.trim()));
                }
            }
        }
        out
    }

    pub fn map(&self, id: &str) -> Option<&Map> {
        self.maps.iter().find(|m| m.id == id)
    }

    /// The maps that list note `id`.
    pub fn maps_of(&self, id: &str) -> Vec<&Map> {
        self.maps
            .iter()
            .filter(|m| m.notes.iter().any(|n| n == id))
            .collect()
    }

    /// Template lines that show a note at work: an API note's own uses, else the first few uses of
    /// each API its snippet calls.
    pub fn examples_of(&self, note: &Note) -> Vec<String> {
        if note.kind == Kind::Api {
            return self.examples(&note.id);
        }
        self.uses(note)
            .iter()
            .flat_map(|n| self.examples(&n.id).into_iter().take(3))
            .collect()
    }

    /// A note's body as plain Markdown for the site: `[[id|text]]` becomes a link, to `#id` when
    /// the note is on the same `map` page, else to the note's own page `?note=id`.
    pub fn site_markdown(&self, note: &Note, map: Option<&Map>) -> String {
        let mut out = String::new();
        let mut rest = note.body.as_str();
        while let Some(start) = rest.find("[[") {
            let Some(end) = rest[start..].find("]]") else {
                break;
            };
            out += &rest[..start];
            let inner = &rest[start + 2..start + end];
            let (id, text) = inner.split_once('|').unwrap_or((inner, inner));
            let id = id.trim();
            let here = map.is_some_and(|m| m.notes.iter().any(|n| n == id));
            let href = if here {
                format!("#{id}")
            } else {
                format!("?note={id}")
            };
            // A link's text stays on one line: the site's Markdown reads links within a line.
            out += &format!(
                "[{}]({href})",
                text.split_whitespace().collect::<Vec<_>>().join(" ")
            );
            rest = &rest[start + end + 2..];
        }
        out + rest
    }

    /// Notes ranked for a query of words, best first (BM25); hand notes and a note's backlinks lift it.
    pub fn search(&self, query: &str) -> Vec<&Note> {
        let terms = tokens(query);
        if terms.is_empty() {
            return vec![];
        }
        let docs: Vec<(Vec<String>, Vec<String>)> = self
            .notes
            .iter()
            // An API title already starts with its name: counting the id too would double it.
            .map(|n| {
                let head = if n.kind == Kind::Api {
                    n.title.clone()
                } else {
                    format!("{} {}", n.id, n.title)
                };
                (tokens(&head), tokens(&n.body))
            })
            .collect();
        let count = docs.len() as f64;
        let average = docs.iter().map(|d| d.1.len()).sum::<usize>() as f64 / count.max(1.0);
        let mut scored: Vec<(f64, &Note)> = self
            .notes
            .iter()
            .zip(&docs)
            .filter_map(|(note, (head, body))| {
                let mut score = 0.0;
                for t in &terms {
                    let having = docs
                        .iter()
                        .filter(|d| d.0.contains(t) || d.1.contains(t))
                        .count() as f64;
                    let idf = ((count - having + 0.5) / (having + 0.5) + 1.0).ln();
                    let tf = 3.0 * head.iter().filter(|w| *w == t).count() as f64
                        + body.iter().filter(|w| *w == t).count() as f64;
                    let norm = 1.2 * (0.25 + 0.75 * body.len() as f64 / average.max(1.0));
                    score += idf * tf * 2.2 / (tf + norm);
                }
                let mut lift = 1.0 + 0.1 * (1.0 + self.backlinks(&note.id).len() as f64).ln();
                // Words ask how: a hand note teaches it, an API note only declares it.
                if note.kind != Kind::Api {
                    lift *= 1.5;
                }
                // A declaration with no prose yet answers nothing a title does not.
                if body.is_empty() {
                    lift *= 0.5;
                }
                (score > 0.0).then_some((score * lift, note))
            })
            .collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.id.cmp(&b.1.id)));
        scored.into_iter().map(|s| s.1).collect()
    }

    /// The note a query names exactly: its id, a code it explains, or a code inside raw error text.
    pub fn lookup(&self, query: &str) -> Option<&Note> {
        let q = query.trim();
        self.get(q)
            .or_else(|| self.by_code(q))
            .or_else(|| self.notes.iter().find(|n| n.id.eq_ignore_ascii_case(q)))
            .or_else(|| {
                q.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
                    .map(|w| w.trim_end_matches('.'))
                    .filter(|w| w.starts_with("snes."))
                    .find_map(|code| self.get(code).or_else(|| self.by_code(code)))
            })
    }

    /// A query's answer as text: the note it names, a map's note list, or the best search hit
    /// opened and up to three more titles. `links` adds backlinks, what the snippet uses and template examples.
    pub fn answer(&self, query: &str, links: bool, reader: Reader) -> Option<String> {
        if let Some(note) = self.lookup(query) {
            return Some(self.render(note, links, reader));
        }
        if let Some(map) = self.map(query.trim()) {
            return Some(self.listing(map));
        }
        let hits = self.search(query);
        let (best, others) = hits.split_first()?;
        let mut out = self.render(best, links, reader);
        if !others.is_empty() {
            out += "\nAlso:\n";
            for n in others.iter().take(3) {
                out += &format!("  {} (ppu docs {})\n", n.title, n.id);
            }
        }
        Some(out)
    }

    /// Every map, one line each: what `ppu docs` and the MCP `docs` tool list.
    pub fn contents(&self) -> String {
        self.maps
            .iter()
            .map(|m| format!("{} — {}: {}\n", m.id, m.title, m.description))
            .collect()
    }

    /// A map as its notes' titles, each with the id to open it by.
    pub fn listing(&self, map: &Map) -> String {
        let mut out = format!("{}: {}\n\n", map.title, map.description);
        for n in map.notes.iter().filter_map(|id| self.get(id)) {
            out += &format!("  {} (ppu docs {})\n", n.title, n.id);
        }
        out
    }

    /// One note: its title, prose (an inline link as `text (id)`) and snippet, then the next action.
    pub fn render(&self, note: &Note, links: bool, reader: Reader) -> String {
        let mut out = format!("{} [{}: {}]\n\n", note.title, note.kind.name(), note.id);
        if !note.body.trim().is_empty() {
            out += &inline(&note.body, reader);
            out += "\n";
        }
        let step = if note.kind == Kind::Api {
            "call it as declared"
        } else {
            "apply the snippet"
        };
        out += &format!("Next: {step}, then {}\n", reader.check());
        if links {
            let back = self.backlinks(&note.id);
            if !back.is_empty() {
                out += "\nLinked from:\n";
                for n in back {
                    out += &format!("  {} ({})\n", n.title, n.id);
                }
            }
            let uses = self.uses(note);
            if !uses.is_empty() {
                let ids: Vec<&str> = uses.iter().map(|n| n.id.as_str()).collect();
                out += &format!("\nUses: {}\n", ids.join(", "));
            }
            if note.kind == Kind::Api {
                let examples = self.examples(&note.id);
                if !examples.is_empty() {
                    out += "\nExamples:\n";
                    for e in examples.iter().take(5) {
                        out += &format!("  {e}\n");
                    }
                    if examples.len() > 5 {
                        out += &format!("  ({} more)\n", examples.len() - 5);
                    }
                }
            }
        }
        out
    }
}

/// `[[id|text]]` as `text (id)`, `[[id]]` as `id`.
fn inline(body: &str, reader: Reader) -> String {
    let mut out = String::new();
    let mut rest = body;
    while let Some(start) = rest.find("[[") {
        let Some(end) = rest[start..].find("]]") else {
            break;
        };
        out += &rest[..start];
        let inner = &rest[start + 2..start + end];
        out += &match inner.split_once('|') {
            Some((id, text)) => format!("{} ({})", reader.link_text(id.trim(), text), id.trim()),
            None => inner.to_string(),
        };
        rest = &rest[start + end + 2..];
    }
    out + rest
}

/// Whether Luau `code` uses API `id`: `@ppu/x` as a require path, else the name called or indexed
/// where it is not a member of something else (`floor(` but not `math.floor(`).
fn calls(code: &str, id: &str) -> bool {
    if id.starts_with("@ppu/") {
        return code.contains(&format!("\"{id}\""));
    }
    let mut from = 0;
    while let Some(i) = code[from..].find(id) {
        let at = from + i;
        let before = code[..at].chars().next_back();
        let after = code[at + id.len()..].chars().next();
        let bounded =
            !before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == ':');
        if bounded && matches!(after, Some('(' | '[' | '.' | ':' | '{')) {
            return true;
        }
        from = at + id.len();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_readers_see_the_tool_a_shell_command_link_stands_for() {
        let body = "run [[ppu-song-check|ppu song check]], see [[ppu-render|ppu render]]";
        assert_eq!(
            inline(body, Reader::Mcp),
            "run song_check (ppu-song-check), see ppu render (ppu-render)"
        );
        assert_eq!(
            inline(body, Reader::Shell),
            "run ppu song check (ppu-song-check), see ppu render (ppu-render)"
        );
        for id in [
            "ppu-song-check",
            "ppu-song-roll",
            "ppu-render-wav",
            "ppu-sfx",
        ] {
            assert!(graph().get(id).is_some(), "{id} is a note");
        }
    }

    #[test]
    fn declarations_give_functions_globals_fields_and_libraries() {
        let notes = api_notes(
            "-- Section ----------\n-- Shared prose.\ndeclare function a(x: number): ()\ndeclare function b(): ()\n\n\
             -- Sprites.\ndeclare sprites: {\n  -- Own.\n  group: () -> (),\n  art: () -> (),\n}\n\
             -- @ppu/hud: a HUD.\nexport type ppu_hud = {\n  text: (x: number) -> (),\n}\n\
             -- not a library\nexport type ppu_pool<T> = {\n  spawn: () -> (),\n}\n",
        );
        let ids: Vec<&str> = notes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "a",
                "b",
                "sprites",
                "sprites.group",
                "sprites.art",
                "@ppu/hud",
                "hud.text"
            ]
        );
        assert_eq!(notes[1].body, "Shared prose.\n");
        assert_eq!(notes[3].body, "Own.\n");
        assert_eq!(notes[4].body, "Sprites.\n");
    }

    #[test]
    fn maps_list_notes_and_links_point_into_the_page() {
        let map = parse_map(
            "m.md",
            "---\nid: m\ntitle: M\ndescription: d\norder: 1\n---\n- [[a]]\n- [[b]]\n",
        )
        .unwrap();
        assert_eq!(map.notes, ["a", "b"]);
        assert!(parse_map("m.md", "---\nid: m\ntitle: M\n---\nprose\n").is_err());
        let graph = graph();
        let note = parse_note(
            "x.md",
            "---\nid: x\nkind: rule\ntitle: T\n---\nSee [[a|the\nfirst]] and [[c]].\n",
        )
        .unwrap();
        assert_eq!(
            graph.site_markdown(&note, Some(&map)),
            "See [the first](#a) and [c](?note=c).\n"
        );
    }

    #[test]
    fn calls_ignore_members_of_other_names() {
        assert!(calls("local x = floor(y)", "floor"));
        assert!(!calls("local x = math.floor(y)", "floor"));
        assert!(calls("bg[1].scroll.x = 0", "bg"));
        assert!(!calls("local bgx = 0", "bg"));
        assert!(calls("local hud = require(\"@ppu/hud\")", "@ppu/hud"));
    }

    #[test]
    fn a_fence_splits_on_luau_file_headers() {
        let fence = |body: &str| Fence { tag: "luau".into(), body: body.into() };
        assert_eq!(fence("local a = 1\n").files(), vec![("main.luau".into(), "local a = 1\n".into())]);
        let files = fence("-- main.luau\nX = 1\n\n-- play.luau, init\nY = 2\n-- not.luau.txt\n").files();
        assert_eq!(
            files,
            vec![
                ("main.luau".into(), "-- main.luau\nX = 1\n".into()),
                ("play.luau".into(), "-- play.luau, init\nY = 2\n-- not.luau.txt\n".into()),
            ]
        );
    }
}
