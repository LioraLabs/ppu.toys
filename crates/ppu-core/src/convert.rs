//! A Lua toy as a Luau toy, by editing tokens in place: comments, `---@snes` annotations and layout
//! stay where they were.
//! - Every `.lua` file becomes `.luau`.
//! - Lua 5.3 bitwise operators become arithmetic where that is exact for every integer
//!   (`x & 31` -> `x % 32`, `x << 3` -> `x * 8`, `x >> 3` -> `x // 8` when x cannot be negative),
//!   and `bit32` calls otherwise.
//! - Tab order becomes explicit: main.luau requires the files that ran before it at its top and the
//!   files that ran after it at its bottom, in tab order.
//!
//! Converting a converted toy changes nothing.
// Lua 5.3 is read with full_moon 2 (see Cargo.toml); the Luau check of the result uses the frontend's full_moon.
use full_moon_lua53::{ast, node::Node, visitors::Visitor, LuaVersion};
use serde::Serialize;
use std::ops::Range;

/// Lua 5.3 operators and Luau syntax at once: the Luau habits a Lua toy may already have.
fn lua(source: &str) -> Result<ast::Ast, Vec<full_moon_lua53::Error>> {
    full_moon_lua53::parse_fallible(source, LuaVersion::luau().with_lua53()).into_result()
}

// From the first token's start to the last token's end (as the frontend's `range`).
fn range(node: &impl Node) -> Range<usize> {
    let mut tokens = node
        .tokens()
        .map(|t| t.token().start_position().bytes()..t.token().end_position().bytes());
    let first = tokens.next().expect("parsed syntax has tokens");
    tokens.fold(first, |a, b| a.start.min(b.start)..a.end.max(b.end))
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Note {
    pub file: String,
    pub line: u32,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToyFile {
    pub name: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Conversion {
    /// The toy's files in their original order, renamed and rewritten.
    pub files: Vec<ToyFile>,
    /// What changed, file by file.
    pub changes: Vec<Note>,
    /// What the conversion could not make Luau: the cartridge will refuse these lines.
    pub unconverted: Vec<Note>,
}

pub fn convert(files: &[(&str, &str)]) -> Conversion {
    let mut out = Conversion {
        files: vec![],
        changes: vec![],
        unconverted: vec![],
    };
    let converts = |(name, _): &(&str, &str)| name.ends_with(".lua");
    let lua_toy = files.iter().any(|(name, _)| *name == "main.lua");
    // Nesting too deep to parse safely: nothing converts, and the note says where.
    for &(name, source) in files {
        if let Err(e) = crate::frontend::nesting(source, crate::frontend::PARSE_NESTING) {
            out.files = files
                .iter()
                .map(|&(name, source)| ToyFile {
                    name: name.into(),
                    source: source.into(),
                })
                .collect();
            out.unconverted.push(Note {
                file: name.into(),
                line: e.line,
                message: e.message,
            });
            return out;
        }
    }
    for file @ &(name, source) in files {
        if !lua_toy || !converts(file) {
            out.files.push(ToyFile {
                name: name.into(),
                source: source.into(),
            });
            continue;
        }
        let luau = format!("{name}u");
        out.changes.push(Note {
            file: luau.clone(),
            line: 0,
            message: format!("renamed from {name}"),
        });
        let source = bitwise(&luau, source, &mut out);
        out.files.push(ToyFile { name: luau, source });
    }
    if lua_toy {
        explicit_order(files, &mut out);
        for file in &out.files {
            if !file.name.ends_with(".luau") {
                continue;
            }
            if let Err(errors) =
                full_moon::parse_fallible(&file.source, full_moon::LuaVersion::luau()).into_result()
            {
                for e in errors {
                    out.unconverted.push(Note {
                        file: file.name.clone(),
                        line: e.range().0.line() as u32,
                        message: format!("not Luau syntax: {}", e.error_message()),
                    });
                }
            }
        }
    }
    out
}

/// The require lines that make tab order explicit, at the top and bottom of main.luau.
fn explicit_order(files: &[(&str, &str)], out: &mut Conversion) {
    let modules = crate::frontend::required_files(files);
    let Some(main) = out.files.iter().position(|f| f.name == "main.luau") else {
        return;
    };
    let already = crate::frontend::requires(&out.files[main].source)
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
    let mut lines = |range: Range<usize>| -> String {
        let mut text = String::new();
        for (k, (name, _)) in files.iter().enumerate().filter(|(k, _)| range.contains(k)) {
            let converted = &out.files[k].name;
            let Some(stem) = converted.strip_suffix(".luau") else {
                continue;
            };
            if modules.contains(*name) || already.iter().any(|p| p == stem) {
                continue;
            }
            text.push_str(&format!("require(\"./{stem}\")\n"));
            out.changes.push(Note {
                file: "main.luau".into(),
                line: 0,
                message: format!(
                    "requires {converted}, which ran {} main in tab order",
                    if k < main { "before" } else { "after" }
                ),
            });
        }
        text
    };
    let (top, bottom) = (lines(0..main), lines(main + 1..files.len()));
    let source = &mut out.files[main].source;
    if !bottom.is_empty() {
        // A top-level return ends the chunk, so the requires go in front of it.
        let at = lua(source)
            .ok()
            .and_then(|ast| ast.nodes().last_stmt().map(|s| range(s).start))
            .unwrap_or(source.len());
        let lead = if at == 0 || source[..at].ends_with('\n') {
            ""
        } else {
            "\n"
        };
        source.insert_str(at, &format!("{lead}{bottom}"));
    }
    source.insert_str(0, &top);
}

fn literal(e: &ast::Expression) -> Option<u64> {
    let ast::Expression::Number(n) = e else {
        return None;
    };
    let text = n.token().to_string();
    match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => text.parse().ok(),
    }
}

/// Provably not negative, so a logical right shift is a floor division.
fn non_negative(e: &ast::Expression) -> bool {
    match e {
        ast::Expression::Number(_) => literal(e).is_some(),
        ast::Expression::Parentheses { expression, .. } => non_negative(expression),
        ast::Expression::BinaryOperator { lhs, binop, rhs } => match binop {
            ast::BinOp::Ampersand(_) => literal(lhs).is_some() || literal(rhs).is_some(),
            ast::BinOp::DoubleGreaterThan(_) => literal(rhs).is_some_and(|k| k > 0),
            ast::BinOp::Percent(_) => literal(rhs).is_some_and(|k| k > 0),
            _ => false,
        },
        _ => false,
    }
}

#[derive(Default)]
struct Bitwise<'a> {
    source: &'a str,
    /// (span, push order, replacement): an empty span inserts.
    edits: Vec<(Range<usize>, usize, String)>,
    notes: Vec<(u32, String)>,
}

impl Bitwise<'_> {
    fn edit(&mut self, span: Range<usize>, text: impl Into<String>) {
        self.edits.push((span, self.edits.len(), text.into()));
    }
}

impl Visitor for Bitwise<'_> {
    fn visit_expression(&mut self, node: &ast::Expression) {
        let line = node.start_position().map_or(0, |p| p.line() as u32);
        match node {
            ast::Expression::BinaryOperator { lhs, binop, rhs } => {
                let call = match binop {
                    ast::BinOp::Ampersand(_) => "band",
                    ast::BinOp::Pipe(_) => "bor",
                    ast::BinOp::Tilde(_) => "bxor",
                    ast::BinOp::DoubleLessThan(_) => "lshift",
                    ast::BinOp::DoubleGreaterThan(_) => "rshift",
                    _ => return,
                };
                let op = binop.token().token().to_string();
                let k = literal(rhs);
                let exact = match (binop, k) {
                    (ast::BinOp::Ampersand(_), Some(m))
                        if m > 0 && m < 1 << 53 && (m + 1).is_power_of_two() =>
                    {
                        Some(("%", m + 1))
                    }
                    (ast::BinOp::DoubleLessThan(_), Some(s)) if s < 32 => Some(("*", 1 << s)),
                    (ast::BinOp::DoubleGreaterThan(_), Some(s)) if s < 32 && non_negative(lhs) => {
                        Some(("//", 1 << s))
                    }
                    _ => None,
                };
                let (l, r) = (range(lhs.as_ref()), range(rhs.as_ref()));
                if let Some((arith, value)) = exact {
                    // `%`, `*` and `//` bind tighter than `&` and the shifts: `a + b & 31` is `(a + b) % 32`.
                    if matches!(
                        **lhs,
                        ast::Expression::BinaryOperator { .. }
                            | ast::Expression::TypeAssertion { .. }
                    ) {
                        self.edit(l.start..l.start, "(");
                        self.edit(l.end..l.end, ")");
                    }
                    let rhs_text = &self.source[r.clone()];
                    let value = if rhs_text.starts_with("0x") || rhs_text.starts_with("0X") {
                        format!("0x{value:X}")
                    } else {
                        value.to_string()
                    };
                    self.edit(range(binop.token()), arith);
                    self.edit(r, value.clone());
                    self.notes
                        .push((line, format!("`{op} {rhs_text}` is `{arith} {value}`")));
                } else {
                    self.edit(l.start..l.start, format!("bit32.{call}("));
                    let op_span = range(binop.token());
                    let gap = l.end..op_span.end;
                    let between = &self.source[l.end..op_span.start];
                    let span = if between.trim().is_empty() {
                        gap
                    } else {
                        op_span
                    };
                    self.edit(span, ",");
                    self.edit(r.end..r.end, ")");
                    self.notes.push((line, format!("`{op}` is bit32.{call}")));
                }
            }
            ast::Expression::UnaryOperator {
                unop: ast::UnOp::Tilde(token),
                expression,
            } => {
                self.edit(range(token), "bit32.bnot(");
                let end = range(expression.as_ref()).end;
                self.edit(end..end, ")");
                self.notes.push((line, "unary `~` is bit32.bnot".into()));
            }
            _ => {}
        }
    }
}

/// `source` with its bitwise operators rewritten; a file that is not Lua is returned as it was.
fn bitwise(file: &str, source: &str, out: &mut Conversion) -> String {
    let ast = match lua(source) {
        Ok(ast) => ast,
        Err(errors) => {
            for e in errors {
                out.unconverted.push(Note {
                    file: file.into(),
                    line: e.range().0.line() as u32,
                    message: format!("not Lua syntax, left as it was: {}", e.error_message()),
                });
            }
            return source.into();
        }
    };
    let mut rewrite = Bitwise {
        source,
        ..Default::default()
    };
    rewrite.visit_ast(&ast);
    for (line, message) in rewrite.notes {
        out.changes.push(Note {
            file: file.into(),
            line,
            message,
        });
    }
    // Inserts before a replacement at the same place; same-place inserts in visit order (outer first).
    let mut edits = rewrite.edits;
    edits.sort_by_key(|(span, order, _)| (span.start, !span.is_empty(), *order));
    let mut text = String::with_capacity(source.len() + 64 * edits.len());
    let mut at = 0;
    for (span, _, replacement) in edits {
        debug_assert!(span.start >= at, "bitwise edits never overlap");
        text.push_str(&source[at..span.start]);
        text.push_str(&replacement);
        at = span.end;
    }
    text.push_str(&source[at..]);
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(source: &str) -> String {
        let c = convert(&[("main.lua", source)]);
        assert!(c.unconverted.is_empty(), "{:?}", c.unconverted);
        c.files[0].source.clone()
    }

    #[test]
    fn masks_by_a_power_of_two_less_one_are_floor_mods() {
        assert_eq!(one("x = y & 31"), "x = y % 32");
        assert_eq!(one("x = y & 0xFF -- low byte"), "x = y % 0x100 -- low byte");
        // `%` binds tighter than `&`: the left side keeps its grouping.
        assert_eq!(one("x = a - b & 255"), "x = (a - b) % 256");
        assert_eq!(one("x = a .. b & 7"), "x = (a .. b) % 8");
        assert_eq!(one("x = y & 0xFF & 15"), "x = (y % 0x100) % 16");
    }

    #[test]
    fn other_masks_are_bit32() {
        assert_eq!(one("x = y & 6"), "x = bit32.band(y, 6)");
        assert_eq!(one("x = 31 & y"), "x = bit32.band(31, y)");
        assert_eq!(one("x = y & z"), "x = bit32.band(y, z)");
    }

    #[test]
    fn constant_shifts_are_multiplies_and_floor_divisions() {
        assert_eq!(one("x = y << 7"), "x = y * 128");
        assert_eq!(one("x = a + b << 1"), "x = (a + b) * 2");
        // A right shift is a floor division only when its operand cannot be negative.
        assert_eq!(one("x = (y & 0xFF) >> 4"), "x = (y % 0x100) // 16");
        assert_eq!(one("x = y >> 4"), "x = bit32.rshift(y, 4)");
        assert_eq!(one("x = y << n"), "x = bit32.lshift(y, n)");
    }

    #[test]
    fn xor_or_and_not_are_bit32() {
        assert_eq!(
            one("s = s ~ ((s * 128) & 0xFFFF)"),
            "s = bit32.bxor(s, ((s * 128) % 0x10000))"
        );
        assert_eq!(
            one("x = x ~ ((x & 0x1FF) << 7)"),
            "x = bit32.bxor(x, ((x % 0x200) * 128))"
        );
        assert_eq!(one("x = a | b | c"), "x = bit32.bor(bit32.bor(a, b), c)");
        assert_eq!(one("x = ~y"), "x = bit32.bnot(y)");
        assert_eq!(one("x = a ~ b ~= c"), "x = bit32.bxor(a, b) ~= c");
    }

    #[test]
    fn comments_and_annotations_stay() {
        let src = "---@snes range 0 255\nlocal seed = 7 -- the seed\nfunction frame(t, f)\n  seed = seed ~ (seed << 1) --[[ mix ]]\nend\n";
        assert_eq!(
            one(src),
            "---@snes range 0 255\nlocal seed = 7 -- the seed\nfunction frame(t, f)\n  seed = bit32.bxor(seed, (seed * 2)) --[[ mix ]]\nend\n"
        );
    }

    #[test]
    fn tab_order_becomes_requires() {
        let files = [
            ("palette.lua", "P = 1\n"),
            ("lib.lua", "L = {}\n"),
            (
                "main.lua",
                "---@snes range 0 9\nlocal n = 0\nfunction frame(t, f) end",
            ),
            ("late.lua", "Z = 2\n"),
            ("song.json", "{}"),
        ];
        let c = convert(&files);
        let names: Vec<_> = c.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "palette.luau",
                "lib.luau",
                "main.luau",
                "late.luau",
                "song.json"
            ]
        );
        assert_eq!(
            c.files[2].source,
            "require(\"./palette\")\nrequire(\"./lib\")\n---@snes range 0 9\nlocal n = 0\nfunction frame(t, f) end\nrequire(\"./late\")\n"
        );
        assert!(c.unconverted.is_empty());
    }

    #[test]
    fn requires_go_before_a_top_level_return_and_modules_are_not_required_again() {
        let files = [
            ("util.lua", "return {}\n"),
            (
                "main.lua",
                "local U = require(\"./util\")\nfunction frame() end\nreturn\n",
            ),
            ("after.lua", ""),
        ];
        let c = convert(&files);
        assert_eq!(
            c.files[1].source,
            "local U = require(\"./util\")\nfunction frame() end\nrequire(\"./after\")\nreturn\n"
        );
    }

    #[test]
    fn the_frontend_still_reads_lua_operators() {
        // full_moon 3 with its lua53 feature panics here (a Luau-only parse of `&`): the converter keeps its own parser.
        assert_eq!(
            crate::frontend::prepare("x = a & 3 | b ~ c << 1").unwrap(),
            "x = a & 3 | b ~ c << 1"
        );
    }

    #[test]
    fn idempotent() {
        let files = [
            ("a.lua", "A = 1 & 3\n"),
            ("main.lua", "x = y ~ z\n"),
            ("b.lua", "B = 2\n"),
        ];
        let once = convert(&files);
        let again: Vec<_> = once
            .files
            .iter()
            .map(|f| (f.name.as_str(), f.source.as_str()))
            .collect();
        let twice = convert(&again);
        assert_eq!(once.files, twice.files);
        assert!(twice.changes.is_empty());
    }

    #[test]
    fn what_luau_cannot_say_is_reported_with_its_line() {
        let c = convert(&[(
            "main.lua",
            "for i = 1, 3 do\n  goto skip\n  ::skip::\nend\n",
        )]);
        assert_eq!(c.files[0].name, "main.luau");
        assert!(
            c.unconverted
                .iter()
                .any(|n| n.file == "main.luau" && n.line == 2),
            "{:?}",
            c.unconverted
        );
    }
}
