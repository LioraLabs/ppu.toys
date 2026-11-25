//! Live pokes written into a scene's own Luau, by editing tokens in place: comments and layout stay
//! where they were (as `convert`).
//!
//! The scene is read once ([`Scene`]): its `init` and `frame` bodies, wherever the module keeps them
//! (`return {init = init, ...}`, an inline function, `function M.init()`, or a program's global
//! `function init()`). Each [`Edit`] kind is one arm in [`apply`] that adds to a [`Plan`]: text
//! changes in place, lines for the end of `init`, and notes. The plan is then written in one pass,
//! creating `init` when the scene has none.
//!
//! - [`Edit::Assign`]: the last plain assignment to the same field directly in `init` is updated in
//!   place when its value is a literal; otherwise the line goes at the end of `init`, with a note
//!   when `init` computes the field or `frame` writes it too (frame overwrites it every update).
//! - [`Edit::Cell`]: a tilemap cell init already assigns (`bg[n].map[x][y] = {tile = ...}`) is updated
//!   in place by the same rule. Other cells with identical entries group into rectangles
//!   ([`map_rects`]), each one `bg[n].map:fill(x, y, w, h, {...})` at the end of `init` (a single cell
//!   too: fill makes its column, so no `if bg[n].map[x] == nil` guard). More than
//!   [`MAX_CELLS`] lines refuse: a whole map belongs in a bg source.
//! - [`Edit::Place`]: a file-level `dma("source", {...})`, before the module returns: the scene's own
//!   setup placement. One already at file level for the same source has its arguments updated.
//! - [`Edit::Sprite`]: a sprite through the sprites library, never `obj[i]`: `group:draw(art, x, y)`
//!   at the end of `frame` (made when the scene has none), its sheet's handle, its `sprites.art` and
//!   its group as file-level locals above `frame`'s definition, so `frame` sees them. A file-level
//!   `local h = dma(sheet)`, an identical art and a plain `sprites.group()` are reused; new names
//!   never shadow one the file already uses.
//! - [`Edit::Hdma`]: a scanline hook for lines `first..=last`, registered at the end of `frame` (the
//!   cartridge reads hooks only from a frame declared `local function frame(t, f)`, and a hook
//!   registered in `init` or at file level never runs). With `bake`, the hook is a file-level
//!   `local name = bakeHdma({}, first, last, function(y) ... end)` above `frame`, called at the end
//!   of `frame`: the form for float ramps and `mode7_floor` views. Hooks append in edit order.
use crate::frontend::range;
use full_moon::{
    ast::{self, Expression, Stmt, Var},
    node::Node,
    tokenizer::TokenType,
    visitors::{Visit, Visitor},
    LuaVersion,
};
use serde::{Deserialize, Serialize};
use std::ops::Range;

/// One poke to write into the scene. A new poke kind is a new variant and its arm in [`apply`].
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum Edit {
    /// `lvalue = expr` in `init`: a register field, a raw register, `cgram[i]`.
    Assign { lvalue: String, expr: String },
    /// One tilemap cell, `word` the entry as VRAM holds it: written as `bg[layer].map[x][y]` in `init`.
    Cell {
        layer: u8,
        x: u16,
        y: u16,
        word: u16,
    },
    /// A source placed at file level: `dma("source", {char = ..., map = ..., pal = ...})`, absent options
    /// left to the engine's defaults.
    Place {
        source: String,
        #[serde(default)]
        char: Option<u16>,
        #[serde(default)]
        map: Option<u16>,
        #[serde(default)]
        pal: Option<u16>,
    },
    /// OAM slot `slot`'s sprite, drawn every frame: cell `cell` of the obj source `sheet` (placed at
    /// file level in the scene, or by a [`Edit::Place`] beside it) as one-cell art of level size
    /// `size`, centred on `x, y`.
    Sprite {
        slot: u8,
        sheet: String,
        cell: u16,
        size: u16,
        x: i32,
        y: i32,
        #[serde(default = "top_prio")]
        prio: u8,
        #[serde(default)]
        flip_x: bool,
        #[serde(default)]
        flip_y: bool,
    },
    /// A scanline hook over lines `first..=last` (0..=223) whose body is `lines`, one Luau statement
    /// each, with `y` the scanline: `hdma(first, last, function(y) ... end)` at the end of `frame`, or
    /// with `bake` an empty-grid `bakeHdma` above `frame` that `frame` calls.
    Hdma {
        first: u8,
        last: u8,
        lines: Vec<String>,
        #[serde(default)]
        bake: bool,
    },
}

fn top_prio() -> u8 {
    3
}

/// The most `bg[n].map` lines (cells and fills) Apply writes; past this a map is a bg source's job.
pub const MAX_CELLS: usize = 64;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Note {
    /// 1-based, in the edited source.
    pub line: u32,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Applied {
    pub source: String,
    /// The lines Apply wrote, 1-based in `source`, ascending.
    pub written: Vec<u32>,
    pub notes: Vec<Note>,
}

/// A text change: `span` of the input becomes `text`; `marks` are offsets in `text` whose lines
/// count as written.
struct Change {
    span: Range<usize>,
    text: String,
    marks: Vec<usize>,
}

/// What the edits add up to, before it is written.
#[derive(Default)]
struct Plan {
    changes: Vec<Change>,
    /// Statements for the end of `init`, in order.
    init_tail: Vec<String>,
    /// File-level statements for before the module returns, in order.
    file_tail: Vec<String>,
    /// File-level statements `frame` reads, for before its definition, in order.
    head: Vec<String>,
    /// Statements for the end of `frame`, in order.
    frame_tail: Vec<String>,
    /// (input offset, message)
    notes: Vec<(usize, String)>,
}

/// Where `init` or `frame` is, or where to make it.
enum Func<'a> {
    Body(&'a ast::FunctionBody),
    /// Insert `decl` + body + `end` at `at`; `field` is where the returned table gains `name = name`
    /// (and whether the table has fields already).
    Create {
        at: usize,
        decl: String,
        field: Option<(usize, bool)>,
    },
}

struct Scene<'a> {
    init: Func<'a>,
    frame: Func<'a>,
    /// Where file-level lines `frame` reads go: before the statement that defines it.
    head_at: usize,
    /// The module's top-level block, where file-level placements live.
    top: &'a ast::Block,
    /// Where file-level lines go: before the return, or at the end of a program without one.
    file_at: usize,
}

/// A file-level `dma("source", ...)`: its arguments, where its statement starts, and the local it
/// is bound to.
struct Dma {
    args: Range<usize>,
    at: usize,
    local: Option<String>,
}

/// `edits` written into `source`, the scene module `file`. Refuses (and changes nothing) for a Lua
/// file, a source that does not parse, or a poke that is not one Luau statement.
pub fn apply(file: &str, source: &str, edits: &[Edit]) -> Result<Applied, String> {
    if !file.ends_with(".luau") {
        return Err(format!(
            "{file} is Lua: convert the toy to Luau first (Convert to Luau), then Apply."
        ));
    }
    crate::frontend::nesting(source, crate::frontend::PARSE_NESTING)
        .map_err(|e| format!("{file}:{}: {}", e.line, e.message))?;
    let ast = parse(source)
        .map_err(|e| format!("{file} does not parse, so Apply left it as it was: {e}"))?;
    let scene = Scene::read(source, &ast).map_err(|e| format!("{file}: {e}"))?;
    let mut plan = Plan::default();
    // One write per field, the last one: a poke list never names a field twice, but two spellings
    // of one (`cgram[0x41]`, `cgram[65]`) are the same field.
    let mut assigns: Vec<(String, &str, &str)> = vec![];
    let mut cells: Vec<(u8, u16, u16, u16)> = vec![];
    let mut places: Vec<(&str, String)> = vec![];
    let mut sprites: Vec<&Edit> = vec![];
    let mut hooks: Vec<&Edit> = vec![];
    for edit in edits {
        match edit {
            Edit::Assign { lvalue, expr } => {
                let key = poke_key(lvalue, expr)?;
                assigns.retain(|(k, ..)| *k != key);
                assigns.push((key, lvalue.trim(), expr.trim()));
            }
            &Edit::Cell { layer, x, y, word } => {
                if !(1..=4).contains(&layer) {
                    return Err(format!(
                        "a map cell names bg[{layer}]: layers are bg[1] to bg[4]"
                    ));
                }
                cells.retain(|c| (c.0, c.1, c.2) != (layer, x, y));
                cells.push((layer, x, y, word));
            }
            Edit::Place {
                source,
                char,
                map,
                pal,
            } => {
                if source.is_empty() || source.contains(['"', '\\', '\n', '\r']) {
                    return Err(format!(
                        "the source name {source:?} cannot be written as a Luau string"
                    ));
                }
                let opts: Vec<String> = [
                    ("char", char.map(hex4)),
                    ("map", map.map(hex4)),
                    ("pal", pal.map(|p| p.to_string())),
                ]
                .into_iter()
                .filter_map(|(k, v)| v.map(|v| format!("{k} = {v}")))
                .collect();
                let args = if opts.is_empty() {
                    format!("(\"{source}\")")
                } else {
                    format!("(\"{source}\", {{{}}})", opts.join(", "))
                };
                places.retain(|(s, _)| s != source);
                places.push((source, args));
            }
            Edit::Sprite {
                slot, sheet, prio, ..
            } => {
                if *prio > 3 {
                    return Err(format!("obj[{slot}]'s priority {prio} is not 0..3"));
                }
                if sheet.is_empty() || sheet.contains(['"', '\\', '\n', '\r']) {
                    return Err(format!(
                        "the source name {sheet:?} cannot be written as a Luau string"
                    ));
                }
                if let Some(at) = indexes_obj(&ast, *slot) {
                    return Err(format!(
                        "{file}:{at}: this scene writes obj[...] itself, which may be obj[{slot}]: Apply draws sprites through a sprites group, so unpoke obj[{slot}] and change it in code"
                    ));
                }
                sprites.push(edit);
            }
            &Edit::Hdma {
                first,
                last,
                ref lines,
                ..
            } => {
                if first > last || last > 223 {
                    return Err(format!("a scanline hook covers lines 0..223 with first <= last, not {first}..{last}"));
                }
                if lines.is_empty()
                    || lines
                        .iter()
                        .any(|l| l.trim().is_empty() || l.contains(['\n', '\r']))
                {
                    return Err(format!("the scanline hook {first}..{last} needs one Luau statement per line, none empty"));
                }
                hooks.push(edit);
            }
        }
    }
    let rects = map_rects(&cells);
    if rects.len() > MAX_CELLS {
        return Err(format!(
            "{} map cells make {} bg[n].map lines, more than Apply writes ({MAX_CELLS}): make the map a bg source (Assets), or apply fewer cells at a time",
            cells.len(),
            rects.len()
        ));
    }
    for (key, lvalue, expr) in assigns {
        scene.assign(&key, lvalue, expr, &mut plan);
    }
    for &Rect { layer, x, y, w, h, word } in &rects {
        let lvalue = format!("bg[{layer}].map[{x}][{y}]");
        let expr = cell_entry(word);
        // A new cell is a fill too: it makes its column, so no guard line. One init already sets is updated in place.
        if (w, h) != (1, 1) || !scene.init_writes(&poke_key(&lvalue, &expr)?) {
            plan.init_tail.push(format!("bg[{layer}].map:fill({x}, {y}, {w}, {h}, {expr})"));
            continue;
        }
        // A fill already in init may cover the cell after its own line: the cell goes after it.
        if matches!(&scene.init, Func::Body(b) if b.block().to_string().contains(&format!("bg[{layer}].map:fill("))) {
            plan.init_tail.push(format!("{lvalue} = {expr}"));
        } else {
            scene.assign(&poke_key(&lvalue, &expr)?, &lvalue, &expr, &mut plan);
        }
    }
    if !sprites.is_empty() && !scene.frame_plain() {
        return Err(format!(
            "{file}: sprites draw from a frame declared `local function frame(t, f)` (`function frame(t, f)` in a program without scenes), which a cartridge needs: write the scene's frame that way, then Apply"
        ));
    }
    if !hooks.is_empty() && !scene.frame_plain() {
        return Err(format!(
            "{file}: scanline hooks are registered from a frame declared `local function frame(t, f)` (`function frame(t, f)` in a program without scenes), which a cartridge needs: write the scene's frame that way, then Apply"
        ));
    }
    let mut names = Names::of(&ast);
    // The sheets sprites draw from, each bound to a file-level local above frame.
    let mut handles: Vec<(&str, String)> = vec![];
    for edit in &sprites {
        let Edit::Sprite { sheet, .. } = edit else {
            continue;
        };
        if handles.iter().all(|(s, _)| s != sheet) {
            let args = places
                .iter()
                .position(|(s, _)| s == sheet)
                .map(|i| places.remove(i).1);
            let handle = scene.bind(file, sheet, args, &mut names, &mut plan)?;
            handles.push((sheet, handle));
        }
    }
    for (name, args) in places {
        scene.place(name, args, &mut plan);
    }
    if !sprites.is_empty() {
        scene.draw(&sprites, &handles, &mut names, &mut plan);
    }
    for edit in hooks {
        let &Edit::Hdma {
            first,
            last,
            ref lines,
            bake,
        } = edit
        else {
            continue;
        };
        let body = lines.iter().map(|l| format!("  {}", l.trim()));
        if bake {
            let name = names.fresh(if lines.iter().any(|l| l.contains("mode7_floor")) {
                "floor"
            } else {
                "scanlines"
            });
            plan.head.push(format!(
                "local {name} = bakeHdma({{}}, {first}, {last}, function(y)"
            ));
            plan.head.extend(body);
            plan.head.push("end)".into());
            plan.frame_tail.push(format!("{name}()"));
        } else {
            plan.frame_tail
                .push(format!("hdma({first}, {last}, function(y)"));
            plan.frame_tail.extend(body);
            plan.frame_tail.push("end)".into());
        }
    }
    write(source, &scene, plan)
}

/// A `w` x `h` rectangle of map cells of one layer, every one the entry `word`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Rect {
    layer: u8,
    x: u16,
    y: u16,
    w: u16,
    h: u16,
    word: u16,
}

/// `cells` (layer, x, y, word), each once, as rectangles of identical entries covering exactly
/// them. Greedy and deterministic: in (layer, column, row) order, each cell not yet covered grows
/// down over equal uncovered cells, then right while the next column holds the same run. A painted
/// pipe of columns is one rectangle per column, and adjacent equal columns merge.
fn map_rects(cells: &[(u8, u16, u16, u16)]) -> Vec<Rect> {
    let mut left: std::collections::BTreeMap<(u8, u16, u16), u16> = cells.iter().map(|&(l, x, y, w)| ((l, x, y), w)).collect();
    let mut out = vec![];
    while let Some((&(layer, x, y), &word)) = left.iter().next() {
        let same = |left: &std::collections::BTreeMap<_, u16>, cx: u16, cy: u16| left.get(&(layer, cx, cy)) == Some(&word);
        let mut h = 1;
        while y.checked_add(h).is_some_and(|cy| same(&left, x, cy)) {
            h += 1;
        }
        let mut w = 1;
        while x.checked_add(w).is_some_and(|cx| (y..y + h).all(|cy| same(&left, cx, cy))) {
            w += 1;
        }
        for cx in x..x + w {
            for cy in y..y + h {
                left.remove(&(layer, cx, cy));
            }
        }
        out.push(Rect { layer, x, y, w, h, word });
    }
    out
}

fn hex4(n: u16) -> String {
    format!("0x{n:04X}")
}

/// A tilemap entry word as the table `read_memory` packs back into the same word.
fn cell_entry(word: u16) -> String {
    let mut fields = vec![
        format!("tile = {}", word & 0x3ff),
        format!("pal = {}", (word >> 10) & 7),
    ];
    if word & 0x2000 != 0 {
        fields.push("prio = 1".into());
    }
    if word & 0x4000 != 0 {
        fields.push("flip_x = true".into());
    }
    if word & 0x8000 != 0 {
        fields.push("flip_y = true".into());
    }
    format!("{{{}}}", fields.join(", "))
}

fn parse(source: &str) -> Result<ast::Ast, String> {
    full_moon::parse_fallible(source, LuaVersion::luau())
        .into_result()
        .map_err(|errors| {
            errors.first().map_or_else(String::new, |e| {
                format!("line {}: {}", e.range().0.line(), e.error_message())
            })
        })
}

/// The field a poke writes, checked to be one plain Luau assignment.
fn poke_key(lvalue: &str, expr: &str) -> Result<String, String> {
    let refuse = || format!("the poke `{lvalue} = {expr}` is not one Luau assignment");
    let ast = parse(&format!("{lvalue} = {expr}")).map_err(|_| refuse())?;
    let mut stmts = ast.nodes().stmts();
    match (stmts.next(), stmts.next(), ast.nodes().last_stmt()) {
        (Some(Stmt::Assignment(a)), None, None)
            if a.variables().len() == 1 && a.expressions().len() == 1 =>
        {
            Ok(var_key(a.variables().iter().next().expect("one variable")))
        }
        _ => Err(refuse()),
    }
}

/// A field's identity: its tokens without layout, numbers by value (`cgram[0x41]` is `cgram[65]`).
fn var_key(var: &Var) -> String {
    var.tokens()
        .map(|t| match t.token().token_type() {
            TokenType::Number { text } => number(text).unwrap_or_else(|| text.to_string()),
            _ => t.token().to_string(),
        })
        .collect()
}

fn number(text: &str) -> Option<String> {
    let text = text.replace('_', "");
    let int = |digits: &str, radix| {
        u64::from_str_radix(digits, radix)
            .ok()
            .map(|v| v.to_string())
    };
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        return int(hex, 16);
    }
    if let Some(bin) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
        return int(bin, 2);
    }
    text.parse::<f64>().ok().map(|v| v.to_string())
}

/// A value Apply may overwrite: a number, a string, true/false/nil, a negated number, a table of
/// named literals (a map cell).
fn literal(e: &Expression) -> bool {
    match e {
        Expression::Number(_) | Expression::String(_) | Expression::Symbol(_) => true,
        Expression::UnaryOperator {
            unop: ast::UnOp::Minus(_),
            expression,
        } => matches!(**expression, Expression::Number(_)),
        Expression::TableConstructor(t) => t
            .fields()
            .iter()
            .all(|f| matches!(f, ast::Field::NameKey { value, .. } if literal(value))),
        _ => false,
    }
}

fn name(t: &full_moon::tokenizer::TokenReference) -> String {
    t.token().to_string()
}

fn name_is(t: &full_moon::tokenizer::TokenReference, n: &str) -> bool {
    t.token().to_string() == n
}

/// The function a top-level statement of `block` names `path` (`["init"]`, `["M", "init"]`), and
/// where that statement starts; the last one wins, as it does when the module runs.
fn function_named<'a>(
    block: &'a ast::Block,
    path: &[&str],
) -> Option<(usize, &'a ast::FunctionBody)> {
    block.stmts().fold(None, |found, stmt| {
        let body = match stmt {
            Stmt::LocalFunction(f) if path.len() == 1 && name(f.name()) == path[0] => {
                Some(f.body())
            }
            Stmt::LocalAssignment(l)
                if path.len() == 1 && l.names().len() == 1 && l.expressions().len() == 1 =>
            {
                match (l.names().iter().next(), l.expressions().iter().next()) {
                    (Some(n), Some(Expression::Function(f))) if name(n) == path[0] => {
                        Some(f.body())
                    }
                    _ => None,
                }
            }
            Stmt::FunctionDeclaration(f) if f.name().method_name().is_none() => {
                let names: Vec<String> = f.name().names().iter().map(name).collect();
                (names == path).then(|| f.body())
            }
            _ => None,
        };
        body.map(|b| (range(stmt).start, b)).or(found)
    })
}

impl<'a> Scene<'a> {
    fn read(source: &'a str, ast: &'a ast::Ast) -> Result<Self, String> {
        let block = ast.nodes();
        let returned = match block.last_stmt() {
            Some(ast::LastStmt::Return(r)) if r.returns().len() == 1 => {
                r.returns().iter().next().map(|e| (r, e))
            }
            _ => None,
        };
        let Some((ret, value)) = returned else {
            // A program without scenes: global functions, init made at the end of the file.
            let lead = if source.is_empty() || source.ends_with("\n\n") {
                ""
            } else if source.ends_with('\n') {
                "\n"
            } else {
                "\n\n"
            };
            let at = block.last_stmt().map_or(source.len(), |s| range(s).start);
            let frame = function_named(block, &["frame"]);
            return Ok(Scene {
                init: function_named(block, &["init"]).map_or_else(
                    || Func::Create {
                        at,
                        decl: format!("{lead}function init()"),
                        field: None,
                    },
                    |(_, b)| Func::Body(b),
                ),
                head_at: frame.map_or(at, |f| f.0),
                frame: frame.map_or_else(
                    || Func::Create {
                        at,
                        decl: format!("{lead}function frame(t, f)"),
                        field: None,
                    },
                    |(_, b)| Func::Body(b),
                ),
                top: block,
                file_at: at,
            });
        };
        let at = range(ret).start;
        match value {
            Expression::TableConstructor(table) => {
                for f in table.fields() {
                    if let ast::Field::ExpressionKey { key, .. } = f {
                        let quoted = key.tokens().map(|t| t.token().to_string()).collect::<String>();
                        let key = quoted.trim_matches(['"', '\'']);
                        if key == "init" || key == "frame" {
                            return Err(format!(
                                "the scene's {key} is written with a quoted key ([{quoted}] = ...): write it as {key} = ... so Apply can find it"
                            ));
                        }
                    }
                }
                // A function the table names, and where its definition starts (the return, inline).
                let field = |key: &str| -> Result<Option<(usize, &'a ast::FunctionBody)>, String> {
                    let found = table.fields().iter().find_map(|f| match f {
                        ast::Field::NameKey { key: k, value, .. } if name(k) == key => Some(value),
                        _ => None,
                    });
                    match found.map(|v| &**v) {
                        None => Ok(None),
                        Some(Expression::Function(f)) => Ok(Some((at, f.body()))),
                        Some(Expression::Var(Var::Name(n))) => function_named(block, &[&name(n)])
                            .map(Some)
                            .ok_or_else(|| format!("the scene's {key} is `{}`, a function Apply cannot find in this file", name(n))),
                        Some(_) => Err(format!("the scene's {key} is not a function Apply can edit")),
                    }
                };
                let first = table.fields().iter().next().map(|f| range(f).start);
                let open = range(table.braces().tokens().0).end;
                let slot = Some(first.map_or((open, false), |at| (at, true)));
                let func = |key: &str, decl: &str| -> Result<(usize, Func<'a>), String> {
                    Ok(match field(key)? {
                        Some((def, body)) => (def, Func::Body(body)),
                        None => (at, Func::Create { at, decl: decl.into(), field: slot }),
                    })
                };
                let (_, init) = func("init", "local function init()")?;
                let (head_at, frame) = func("frame", "local function frame(t, f)")?;
                Ok(Scene { init, frame, head_at, top: block, file_at: at })
            }
            Expression::Var(Var::Name(m)) => {
                let m = name(m);
                let frame = function_named(block, &[&m, "frame"]);
                Ok(Scene {
                    init: function_named(block, &[&m, "init"]).map_or_else(
                        || Func::Create { at, decl: format!("function {m}.init()"), field: None },
                        |(_, b)| Func::Body(b),
                    ),
                    head_at: frame.map_or(at, |f| f.0),
                    frame: frame.map_or_else(|| Func::Create { at, decl: format!("function {m}.frame(t, f)"), field: None }, |(_, b)| Func::Body(b)),
                    top: block,
                    file_at: at,
                })
            }
            _ => Err("the file returns something that is not a scene table, so Apply cannot find its init".into()),
        }
    }

    /// Whether `init` writes the field `key` anywhere in its body.
    fn init_writes(&self, key: &str) -> bool {
        let Func::Body(body) = &self.init else {
            return false;
        };
        let mut writes = Writes { key, at: vec![] };
        body.block().visit(&mut writes);
        !writes.at.is_empty()
    }

    /// The last file-level `dma(name, ...)`, as the module runs it.
    fn dma(&self, name: &str) -> Option<Dma> {
        let found = self.top.stmts().filter_map(|stmt| {
            let (call, local) = match stmt {
                Stmt::FunctionCall(c) => Some((c, None)),
                Stmt::LocalAssignment(l) if l.expressions().len() == 1 => match l.expressions().iter().next() {
                    Some(Expression::FunctionCall(c)) => Some((c, (l.names().len() == 1).then(|| l.names().iter().map(self::name).collect()))),
                    _ => None,
                },
                _ => None,
            }?;
            let ast::Prefix::Name(callee) = call.prefix() else { return None };
            let [ast::Suffix::Call(ast::Call::AnonymousCall(args))] = call.suffixes().collect::<Vec<_>>().as_slice() else {
                return None;
            };
            let ast::FunctionArgs::Parentheses { arguments, .. } = args.as_ref() else { return None };
            let first = arguments.iter().next()?;
            let Expression::String(s) = first else { return None };
            let named = matches!(s.token().token_type(), TokenType::StringLiteral { literal, .. } if literal.as_str() == name);
            (name_is(callee, "dma") && named).then(|| Dma { args: range(args.as_ref()), at: range(stmt).start, local })
        });
        found.last()
    }

    /// `dma(name, ...)` at file level: the last one there for `name` gets `args`, else a new line.
    fn place(&self, name: &str, args: String, plan: &mut Plan) {
        match self.dma(name) {
            Some(d) => plan.changes.push(Change {
                span: d.args,
                text: args,
                marks: vec![0],
            }),
            None => plan.file_tail.push(format!("dma{args}")),
        }
    }

    /// The local a sprite sheet's handle is in, above frame: its file-level `dma` (given `args`
    /// when the poke places it), bound to a new local when it is a bare call, or made above frame.
    fn bind(
        &self,
        file: &str,
        sheet: &str,
        args: Option<String>,
        names: &mut Names,
        plan: &mut Plan,
    ) -> Result<String, String> {
        let Some(d) = self.dma(sheet) else {
            let Some(args) = args else {
                return Err(format!("{file} does not place the sprite sheet \"{sheet}\" at file level, so its sprites have nothing to draw from"));
            };
            let handle = names.fresh(&ident(sheet));
            plan.head.push(format!("local {handle} = dma{args}"));
            return Ok(handle);
        };
        if d.at >= self.head_at {
            return Err(format!(
                "{file}: dma(\"{sheet}\") comes after frame is defined, so frame cannot draw from it: move it above frame, then Apply"
            ));
        }
        if let Some(args) = args {
            plan.changes.push(Change {
                span: d.args,
                text: args,
                marks: vec![0],
            });
        }
        Ok(match d.local {
            Some(local) => local,
            None => {
                let handle = names.fresh(&ident(sheet));
                plan.changes.push(Change {
                    span: d.at..d.at,
                    text: format!("local {handle} = "),
                    marks: vec![0],
                });
                handle
            }
        })
    }

    /// Whether frame is (or is made as) one function named `frame` at file level: the frame a
    /// cartridge draws the library's sprites at the end of.
    fn frame_plain(&self) -> bool {
        match &self.frame {
            Func::Create { decl, .. } => !decl.contains('.'),
            Func::Body(_) => self.top.stmts().any(|stmt| {
                range(stmt).start == self.head_at
                    && match stmt {
                        Stmt::LocalFunction(f) => name_is(f.name(), "frame"),
                        Stmt::FunctionDeclaration(f) => {
                            f.name().method_name().is_none()
                                && f.name().names().len() == 1
                                && f.name().names().iter().all(|n| name_is(n, "frame"))
                        }
                        _ => false,
                    }
            }),
        }
    }

    /// The local of the first plain `local g = sprites.group()` above frame: one with options
    /// (slots, sort, keep) draws differently, so it is not reused.
    fn group(&self) -> Option<String> {
        self.locals_above().find_map(|(local, expr)| {
            (expr == "sprites.group()" || expr == "sprites.group({})").then_some(local)
        })
    }

    /// File-level `local name = <expression>` statements above frame, the expression without
    /// whitespace.
    fn locals_above(&self) -> impl Iterator<Item = (String, String)> + '_ {
        self.top.stmts().filter_map(move |stmt| match stmt {
            Stmt::LocalAssignment(l)
                if range(stmt).start < self.head_at
                    && l.names().len() == 1
                    && l.expressions().len() == 1 =>
            {
                let expr = l.expressions().iter().next()?.to_string();
                Some((
                    l.names().iter().map(self::name).collect(),
                    expr.split_whitespace().collect(),
                ))
            }
            _ => None,
        })
    }

    /// Each sprite as a `group:draw` at the end of frame: its art and group above frame, reused
    /// when the file makes the same ones.
    fn draw(
        &self,
        sprites: &[&Edit],
        handles: &[(&str, String)],
        names: &mut Names,
        plan: &mut Plan,
    ) {
        let group = self.group().unwrap_or_else(|| {
            let g = names.fresh("actors");
            plan.head.push(format!("local {g} = sprites.group()"));
            g
        });
        let mut arts: Vec<(String, String)> = self.locals_above().map(|(l, e)| (e, l)).collect();
        for edit in sprites {
            let &Edit::Sprite {
                ref sheet,
                cell,
                size,
                x,
                y,
                prio,
                flip_x,
                flip_y,
                ..
            } = *edit
            else {
                continue;
            };
            let handle = &handles
                .iter()
                .find(|(s, _)| s == sheet)
                .expect("every sheet is bound")
                .1;
            let make = format!(
                "sprites.art({{{{size = {size}, columns = 1, cells = {{{cell}}}}}}}, {handle})"
            );
            let bare: String = make.split_whitespace().collect();
            let art = match arts.iter().find(|(e, _)| *e == bare) {
                Some((_, art)) => art.clone(),
                None => {
                    let art = names.fresh(&format!("sprite_{cell}"));
                    plan.head.push(format!("local {art} = {make}"));
                    arts.push((bare, art.clone()));
                    art
                }
            };
            let opts: Vec<String> = [
                (prio != 3).then(|| format!("prio = {prio}")),
                flip_x.then(|| "flip_x = true".into()),
                flip_y.then(|| "flip_y = true".into()),
            ]
            .into_iter()
            .flatten()
            .collect();
            let opts = if opts.is_empty() {
                String::new()
            } else {
                format!(", {{{}}}", opts.join(", "))
            };
            plan.frame_tail
                .push(format!("{group}:draw({art}, {x}, {y}{opts})"));
        }
    }

    fn assign(&self, key: &str, lvalue: &str, expr: &str, plan: &mut Plan) {
        let line = format!("{lvalue} = {expr}");
        // The last write to the field directly in init: the one the scene ends its setup with.
        let mut last: Option<(&Expression, bool)> = None;
        if let Func::Body(body) = &self.init {
            for stmt in body.block().stmts() {
                match stmt {
                    Stmt::Assignment(a) => {
                        let one_each = a.variables().len() == a.expressions().len();
                        for (i, var) in a.variables().iter().enumerate() {
                            if var_key(var) == key {
                                let expr = a.expressions().iter().nth(if one_each {
                                    i
                                } else {
                                    usize::MAX
                                });
                                last = Some(match expr {
                                    Some(e) => (e, literal(e)),
                                    None => (
                                        a.expressions()
                                            .iter()
                                            .last()
                                            .expect("an assignment has a value"),
                                        false,
                                    ),
                                });
                            }
                        }
                    }
                    Stmt::CompoundAssignment(c) if var_key(c.lhs()) == key => {
                        last = Some((c.rhs(), false))
                    }
                    _ => {}
                }
            }
        }
        match last {
            Some((e, true)) => plan.changes.push(Change {
                span: range(e),
                text: expr.into(),
                marks: vec![0],
            }),
            Some((e, false)) => {
                plan.notes.push((range(e).start, format!("init sets {lvalue} from code here; the applied line at the end of init wins")));
                plan.init_tail.push(line);
            }
            None => plan.init_tail.push(line),
        }
        if let Func::Body(frame) = &self.frame {
            let mut writes = Writes { key, at: vec![] };
            frame.block().visit(&mut writes);
            for at in writes.at {
                plan.notes.push((
                    at,
                    format!(
                        "frame() writes {lvalue} too: it overwrites the applied value every update"
                    ),
                ));
            }
        }
    }
}

/// The names a file already uses (every identifier in it), and the ones Apply made, so a new local
/// shadows nothing.
struct Names(std::collections::HashSet<String>);

impl Names {
    fn of(ast: &ast::Ast) -> Self {
        Names(
            ast.tokens()
                .filter_map(|t| match t.token().token_type() {
                    TokenType::Identifier { identifier } => Some(identifier.to_string()),
                    _ => None,
                })
                .collect(),
        )
    }

    fn fresh(&mut self, base: &str) -> String {
        let name = std::iter::once(base.to_string())
            .chain((2..).map(|n| format!("{base}_{n}")))
            .find(|n| !self.0.contains(n))
            .expect("a free name");
        self.0.insert(name.clone());
        name
    }
}

/// A source name as a Luau identifier: `title-sprites-0` is `title_sprites_0`.
fn ident(source: &str) -> String {
    let mut out: String = source
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if !out.starts_with(|c: char| c.is_ascii_alphabetic()) {
        out.insert_str(0, "sheet_");
    }
    out
}

/// The line of the first `obj[...]` index that may be `obj[slot]`: a constant `slot`, or one Apply
/// cannot read.
fn indexes_obj(ast: &ast::Ast, slot: u8) -> Option<u32> {
    // A tree's tokens come by node (a bracket pair before what it holds): put them in source order.
    let mut tokens: Vec<_> = ast
        .tokens()
        .filter(|t| !matches!(t.token().token_type(), TokenType::Eof))
        .collect();
    tokens.sort_by_key(|t| t.token().start_position().bytes());
    tokens.windows(4).find_map(|w| {
        let is = |i: usize, text: &str| w[i].token().to_string() == text;
        let obj = matches!(w[0].token().token_type(), TokenType::Identifier { identifier } if identifier.as_str() == "obj") && is(1, "[");
        let other = matches!(w[2].token().token_type(), TokenType::Number { text } if number(text) != Some(slot.to_string())) && is(3, "]");
        (obj && !other).then(|| w[0].token().start_position().line() as u32)
    })
}

/// Every write to one field in a block, at any depth.
struct Writes<'k> {
    key: &'k str,
    at: Vec<usize>,
}

impl Visitor for Writes<'_> {
    fn visit_assignment(&mut self, node: &ast::Assignment) {
        for var in node.variables() {
            if var_key(var) == self.key {
                self.at.push(range(var).start);
            }
        }
    }
    fn visit_compound_assignment(&mut self, node: &ast::CompoundAssignment) {
        if var_key(node.lhs()) == self.key {
            self.at.push(range(node.lhs()).start);
        }
    }
}

/// The whitespace a line starts with, for the line holding byte `at`.
fn indent_at(source: &str, at: usize) -> &str {
    let start = source[..at].rfind('\n').map_or(0, |n| n + 1);
    let rest = &source[start..];
    &rest[..rest.len() - rest.trim_start_matches([' ', '\t']).len()]
}

/// Nothing but whitespace before `at` on its line.
fn first_on_line(source: &str, at: usize) -> bool {
    let start = source[..at].rfind('\n').map_or(0, |n| n + 1);
    source[start..at].trim().is_empty()
}

/// `lines` as one change at the end of an existing function body.
fn append(source: &str, body: &ast::FunctionBody, lines: &[String]) -> Change {
    let block = body.block();
    // A trailing return ends the body, so the lines go in front of it.
    let target = match block.last_stmt() {
        Some(s) => range(s).start,
        None => range(body.end_token()).start,
    };
    let first = block
        .stmts()
        .next()
        .map(|s| range(s).start)
        .filter(|&s| first_on_line(source, s));
    let mut text = String::new();
    let mut marks = vec![];
    if first_on_line(source, target) {
        let base = indent_at(source, target);
        let indent = match (first, block.last_stmt()) {
            (Some(s), _) => indent_at(source, s).to_string(),
            (None, Some(_)) => base.to_string(),
            (None, None) => format!("{base}  "),
        };
        let at = target - base.len();
        for line in lines {
            marks.push(text.len());
            text.push_str(&format!("{indent}{line}\n"));
        }
        Change {
            span: at..at,
            text,
            marks,
        }
    } else {
        // A body on one line (`local function init() end`): open it up.
        let base = indent_at(source, target);
        let indent = format!("{base}  ");
        let start = target
            - (source[..target].len() - source[..target].trim_end_matches([' ', '\t']).len());
        for line in lines {
            text.push('\n');
            marks.push(text.len());
            text.push_str(&format!("{indent}{line}"));
        }
        text.push_str(&format!("\n{base}"));
        Change {
            span: start..target,
            text,
            marks,
        }
    }
}

fn write(source: &str, scene: &Scene, mut plan: Plan) -> Result<Applied, String> {
    if !plan.head.is_empty() {
        if scene.head_at == scene.file_at {
            // Frame is made here, or is inline in the return: the lines go with the placements.
            plan.file_tail.append(&mut plan.head);
        } else {
            let at = scene.head_at;
            let mut text = String::new();
            let mut marks = vec![];
            for line in &plan.head {
                marks.push(text.len());
                text.push_str(&format!("{line}\n"));
            }
            text.push('\n');
            plan.changes.push(Change {
                span: at..at,
                text,
                marks,
            });
        }
    }
    if !plan.file_tail.is_empty() {
        // Before a return (or anything else) the lines get a blank line after them; at the end
        // of a program they follow its last line, a blank line apart.
        let at = scene.file_at;
        let (lead, trail) = if at < source.len() {
            ("", "\n")
        } else if source.is_empty() || source.ends_with("\n\n") {
            ("", "")
        } else if source.ends_with('\n') {
            ("\n", "")
        } else {
            ("\n\n", "")
        };
        let mut text = lead.to_string();
        let mut marks = vec![];
        for line in &plan.file_tail {
            marks.push(text.len());
            text.push_str(&format!("{line}\n"));
        }
        text.push_str(trail);
        // Pushed before init is made at the same offset: the placement comes first.
        plan.changes.push(Change {
            span: at..at,
            text,
            marks,
        });
    }
    // The returned table's new fields, one change: `init = init, frame = frame`.
    let mut fields: Option<(usize, bool, Vec<&str>)> = None;
    for (func, lines, key) in [
        (&scene.init, &plan.init_tail, "init"),
        (&scene.frame, &plan.frame_tail, "frame"),
    ] {
        if lines.is_empty() {
            continue;
        }
        match func {
            Func::Body(body) => plan.changes.push(append(source, body, lines)),
            Func::Create { at, decl, field } => {
                let mut text = format!("{decl}\n");
                let mut marks = vec![];
                for line in lines {
                    marks.push(text.len());
                    text.push_str(&format!("  {line}\n"));
                }
                text.push_str(if field.is_some() { "end\n\n" } else { "end\n" });
                if let Some((at, more)) = field {
                    fields.get_or_insert((*at, *more, vec![])).2.push(key);
                }
                plan.changes.push(Change {
                    span: *at..*at,
                    text,
                    marks,
                });
            }
        }
    }
    if let Some((at, more, keys)) = fields {
        let list: Vec<String> = keys.iter().map(|k| format!("{k} = {k}")).collect();
        let text = list.join(", ") + if more { ", " } else { "" };
        plan.changes.push(Change {
            span: at..at,
            text,
            marks: vec![],
        });
    }
    plan.changes.sort_by_key(|c| c.span.start);
    let mut out = String::with_capacity(source.len() + 64 * plan.changes.len());
    let mut written = vec![];
    // (input end, output - input) after each change, to place notes.
    let mut shifts: Vec<(usize, isize)> = vec![];
    let mut at = 0;
    for c in &plan.changes {
        debug_assert!(c.span.start >= at, "apply changes never overlap");
        out.push_str(&source[at..c.span.start]);
        for &m in &c.marks {
            written.push(line_of(&out, out.len() + m, &c.text));
        }
        out.push_str(&c.text);
        at = c.span.end;
        shifts.push((at, out.len() as isize - at as isize));
    }
    out.push_str(&source[at..]);
    written.sort_unstable();
    written.dedup();
    let place = |offset: usize| {
        let shift = shifts
            .iter()
            .take_while(|(end, _)| *end <= offset)
            .last()
            .map_or(0, |s| s.1);
        let o = (offset as isize + shift) as usize;
        out[..o].matches('\n').count() as u32 + 1
    };
    let mut notes: Vec<Note> = plan
        .notes
        .iter()
        .map(|(offset, message)| Note {
            line: place(*offset),
            message: message.clone(),
        })
        .collect();
    notes.sort_by_key(|n| n.line);
    parse(&out).map_err(|e| {
        format!("Apply would leave code that does not parse ({e}): a poke's value is not Luau")
    })?;
    Ok(Applied {
        source: out,
        written,
        notes,
    })
}

/// The 1-based line of output offset `at`, where `text` is about to follow `out`.
fn line_of(out: &str, at: usize, text: &str) -> u32 {
    let within = at - out.len();
    (out.matches('\n').count() + text[..within].matches('\n').count()) as u32 + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assign(lvalue: &str, expr: &str) -> Edit {
        Edit::Assign {
            lvalue: lvalue.into(),
            expr: expr.into(),
        }
    }

    fn run(file: &str, source: &str, edits: &[Edit]) -> Applied {
        apply(file, source, edits).unwrap()
    }

    const TITLE: &str = "\
-- the title
local sky = dma(\"sky\")

local function init()
  bg[1].char_base = sky.char -- from the source
  brightness = 15 -- full
  cgram[65] = 0x7FFF
  win.w1.lo, win.w1.hi = 32, 200
end

local function frame(t, f)
  bg[1].scroll.x = f
end

return {init = init, frame = frame}
";

    #[test]
    fn a_literal_in_init_is_updated_in_place_and_comments_stay() {
        let a = run(
            "title.luau",
            TITLE,
            &[
                assign("brightness", "9"),
                assign("cgram[0x41]", "0x001F"),
                assign("win.w1.hi", "180"),
            ],
        );
        assert_eq!(
            a.source,
            TITLE
                .replace("brightness = 15 -- full", "brightness = 9 -- full")
                .replace("cgram[65] = 0x7FFF", "cgram[65] = 0x001F")
                .replace("32, 200", "32, 180")
        );
        assert_eq!(a.written, [6, 7, 8]);
        assert!(a.notes.is_empty(), "{:?}", a.notes);
    }

    #[test]
    fn a_new_field_goes_at_the_end_of_init() {
        let a = run(
            "title.luau",
            TITLE,
            &[assign("color.op", "\"sub\""), assign("TM", "0x13")],
        );
        assert!(
            a.source.contains(
                "  win.w1.lo, win.w1.hi = 32, 200\n  color.op = \"sub\"\n  TM = 0x13\nend\n"
            ),
            "{}",
            a.source
        );
        assert_eq!(a.written, [9, 10]);
    }

    #[test]
    fn a_computed_value_or_a_frame_write_still_applies_with_a_note() {
        let a = run(
            "title.luau",
            TITLE,
            &[
                assign("bg[1].char_base", "0x2000"),
                assign("bg[ 1 ].scroll.x", "4"),
            ],
        );
        assert!(
            a.source
                .contains("  bg[1].char_base = 0x2000\n  bg[ 1 ].scroll.x = 4\nend\n"),
            "{}",
            a.source
        );
        assert!(a
            .source
            .contains("bg[1].char_base = sky.char -- from the source"));
        assert_eq!(a.written, [9, 10]);
        let lines: Vec<_> = a
            .notes
            .iter()
            .map(|n| (n.line, n.message.split(' ').next().unwrap()))
            .collect();
        assert_eq!(lines, [(5, "init"), (14, "frame()")], "{:?}", a.notes);
    }

    #[test]
    fn a_column_offset_is_one_field_per_map_column() {
        let src = "local function init()\n  bg[2].columns[6] = -24\nend\nreturn {init = init}\n";
        let a = run("a.luau", src, &[assign("bg[2].columns[6]", "-8"), assign("bg[2].columns[0x7]", "-8")]);
        assert_eq!(a.source, "local function init()\n  bg[2].columns[6] = -8\n  bg[2].columns[0x7] = -8\nend\nreturn {init = init}\n");
        assert_eq!(a.written, [2, 3]);
    }

    #[test]
    fn a_write_nested_in_init_is_not_replaced() {
        let src = "local function init()\n  if pad.a then\n    brightness = 3\n  end\nend\nreturn {init = init}\n";
        let a = run("a.luau", src, &[assign("brightness", "9")]);
        assert_eq!(a.source, "local function init()\n  if pad.a then\n    brightness = 3\n  end\n  brightness = 9\nend\nreturn {init = init}\n");
        assert_eq!(a.written, [5]);
    }

    #[test]
    fn the_lines_go_before_a_trailing_return() {
        let src = "local function init()\n\tmode = 1\n\treturn\nend\nreturn {init = init}\n";
        let a = run("a.luau", src, &[assign("brightness", "9")]);
        assert_eq!(a.source, "local function init()\n\tmode = 1\n\tbrightness = 9\n\treturn\nend\nreturn {init = init}\n");
    }

    #[test]
    fn a_scene_without_init_gets_one() {
        let src = "return {frame = function(t, f) brightness = 5 end}\n";
        let a = run("a.luau", src, &[assign("cgram[0]", "0x7C00")]);
        assert_eq!(a.source, "local function init()\n  cgram[0] = 0x7C00\nend\n\nreturn {init = init, frame = function(t, f) brightness = 5 end}\n");
        assert_eq!(a.written, [2]);
        let empty = run("b.luau", "return {}", &[assign("mode", "1")]);
        assert_eq!(
            empty.source,
            "local function init()\n  mode = 1\nend\n\nreturn {init = init}"
        );
    }

    #[test]
    fn inline_and_one_line_inits_open_up() {
        let a = run(
            "a.luau",
            "return {init = function() end}\n",
            &[assign("mode", "1")],
        );
        assert_eq!(a.source, "return {init = function()\n  mode = 1\nend}\n");
        let b = run(
            "b.luau",
            "local function init() mode = 1 end\nreturn {init = init}\n",
            &[assign("brightness", "2")],
        );
        assert_eq!(
            b.source,
            "local function init() mode = 1\n  brightness = 2\nend\nreturn {init = init}\n"
        );
        assert_eq!(b.written, [2]);
    }

    #[test]
    fn a_module_table_and_a_program_without_scenes() {
        let m = run(
            "a.luau",
            "local M = {}\nfunction M.init()\n  mode = 1\nend\nreturn M\n",
            &[assign("mode", "7")],
        );
        assert_eq!(
            m.source,
            "local M = {}\nfunction M.init()\n  mode = 7\nend\nreturn M\n"
        );
        let made = run("a.luau", "local M = {}\nreturn M\n", &[assign("mode", "7")]);
        assert_eq!(
            made.source,
            "local M = {}\nfunction M.init()\n  mode = 7\nend\nreturn M\n"
        );
        let main = "function init()\n  mode = 1\nend\n\nfunction frame(t, f)\n  brightness = 15 - f % 4\nend\n";
        let a = run("main.luau", main, &[assign("brightness", "3")]);
        assert_eq!(
            a.source,
            main.replace("  mode = 1\n", "  mode = 1\n  brightness = 3\n")
        );
        assert_eq!(a.notes.len(), 1);
        assert_eq!(a.notes[0].line, 7);
        let bare = run(
            "main.luau",
            "function frame(t, f) end\n",
            &[assign("mode", "1")],
        );
        assert_eq!(
            bare.source,
            "function frame(t, f) end\n\nfunction init()\n  mode = 1\nend\n"
        );
        assert_eq!(bare.written, [4]);
    }

    #[test]
    fn the_last_spelling_of_a_field_wins() {
        let a = run(
            "a.luau",
            "return {}",
            &[assign("cgram[0x41]", "1"), assign("cgram[65]", "2")],
        );
        assert!(
            a.source.contains("  cgram[65] = 2\n") && !a.source.contains("= 1"),
            "{}",
            a.source
        );
    }

    #[test]
    fn refusals_change_nothing() {
        assert!(apply("main.lua", "x = 1", &[assign("mode", "1")])
            .unwrap_err()
            .contains("Convert to Luau"));
        assert!(
            apply("a.luau", "local function init(", &[assign("mode", "1")])
                .unwrap_err()
                .contains("does not parse")
        );
        assert!(apply("a.luau", "return {}", &[assign("mode", "1 2")])
            .unwrap_err()
            .contains("not one Luau assignment"));
        assert!(apply("a.luau", "return {}", &[assign("a, b", "1, 2")])
            .unwrap_err()
            .contains("not one Luau assignment"));
        assert!(apply("a.luau", "return 5", &[assign("mode", "1")])
            .unwrap_err()
            .contains("not a scene table"));
        let unknown: Result<Vec<Edit>, _> = serde_json::from_str(r#"[{"kind":"sprite","x":1}]"#);
        assert!(unknown.is_err());
        let known: Vec<Edit> =
            serde_json::from_str(r#"[{"kind":"assign","lvalue":"mode","expr":"1"}]"#).unwrap();
        assert_eq!(known, [assign("mode", "1")]);
    }

    fn cell(layer: u8, x: u16, y: u16, word: u16) -> Edit {
        Edit::Cell { layer, x, y, word }
    }

    fn place(source: &str, char: Option<u16>) -> Edit {
        Edit::Place {
            source: source.into(),
            char,
            map: None,
            pal: None,
        }
    }

    #[test]
    fn new_map_cells_are_fills_and_a_written_cell_updates_in_place() {
        let a = run("a.luau", "return {}", &[cell(1, 3, 4, 0x0405), cell(1, 3, 5, 0xE001), cell(2, 0, 0, 7)]);
        assert_eq!(
            a.source,
            "local function init()\n  bg[1].map:fill(3, 4, 1, 1, {tile = 5, pal = 1})\n  \
             bg[1].map:fill(3, 5, 1, 1, {tile = 1, pal = 0, prio = 1, flip_x = true, flip_y = true})\n  \
             bg[2].map:fill(0, 0, 1, 1, {tile = 7, pal = 0})\nend\n\nreturn {init = init}"
        );
        assert_eq!(a.written, [2, 3, 4]);
        let src = "local function init()\n  if bg[1].map[3] == nil then bg[1].map[3] = {} end\n  bg[1].map[3][4] = {tile = 5, pal = 1}\nend\nreturn {init = init}\n";
        let again = run("a.luau", src, &[cell(1, 3, 4, 9)]);
        assert_eq!(again.source, src.replace("{tile = 5, pal = 1}", "{tile = 9, pal = 0}"), "a cell init writes is updated in place");
        assert!(again.notes.is_empty(), "{:?}", again.notes);
    }

    #[test]
    fn more_cells_than_the_cap_refuse_toward_a_bg_source() {
        let many: Vec<Edit> = (0..=MAX_CELLS as u16).map(|i| cell(1, i % 32, i / 32, i)).collect();
        assert!(apply("a.luau", "return {}", &many).unwrap_err().contains("bg source"));
        assert!(apply("a.luau", "return {}", &many[..MAX_CELLS]).is_ok());
        let same: Vec<Edit> = (0..32 * 32).map(|i| cell(1, i % 32, i / 32, 1)).collect();
        assert!(apply("a.luau", "return {}", &same).unwrap().source.contains("bg[1].map:fill(0, 0, 32, 32, {tile = 1, pal = 0})"), "the cap counts lines, not cells");
        assert!(apply("a.luau", "return {}", &[cell(5, 0, 0, 1)]).unwrap_err().contains("bg[1] to bg[4]"));
    }

    #[test]
    fn a_placement_goes_at_file_level_and_an_existing_one_is_updated() {
        let a = run(
            "title.luau",
            TITLE,
            &[
                place("title-tiles-1000", Some(0x1000)),
                assign("bg[2].char_base", "0x1000"),
            ],
        );
        assert!(
            a.source.ends_with("end\n\ndma(\"title-tiles-1000\", {char = 0x1000})\n\nreturn {init = init, frame = frame}\n"),
            "{}",
            a.source
        );
        assert_eq!(a.written, [9, 16]);
        let moved = run(
            "title.luau",
            &a.source,
            &[place("title-tiles-1000", Some(0x2000)), place("sky", None)],
        );
        assert_eq!(
            moved.source,
            a.source.replace("{char = 0x1000})", "{char = 0x2000})"),
            "both update in place"
        );
        assert_eq!(moved.written, [2, 16]);
        let local = "local sky = dma('sky', {char = 0x1000, map = 0})\nreturn {}\n";
        let kept = run(
            "a.luau",
            local,
            &[Edit::Place {
                source: "sky".into(),
                char: Some(0x3000),
                map: Some(0x400),
                pal: Some(16),
            }],
        );
        assert_eq!(
            kept.source,
            "local sky = dma(\"sky\", {char = 0x3000, map = 0x0400, pal = 16})\nreturn {}\n"
        );
        let made = run(
            "a.luau",
            "return {}",
            &[place("t", Some(0x1000)), assign("mode", "1")],
        );
        assert_eq!(made.source, "dma(\"t\", {char = 0x1000})\n\nlocal function init()\n  mode = 1\nend\n\nreturn {init = init}");
        let main = run(
            "main.luau",
            "function frame(t, f) end\n",
            &[place("t", None), assign("mode", "1")],
        );
        assert_eq!(
            main.source,
            "function frame(t, f) end\n\ndma(\"t\")\n\nfunction init()\n  mode = 1\nend\n"
        );
        assert!(apply("a.luau", "return {}", &[place("a\"b", None)]).is_err());
    }

    #[test]
    fn a_quoted_init_key_refuses_instead_of_making_a_second_init() {
        let src = "local function setup() mode = 1 end\nreturn {[\"init\"] = setup}\n";
        let e = apply("a.luau", src, &[assign("mode", "3")]).unwrap_err();
        assert!(e.contains("quoted key") && e.contains("init = ..."), "{e}");
        assert!(apply(
            "a.luau",
            "return {['frame'] = function() end}",
            &[assign("mode", "3")]
        )
        .unwrap_err()
        .contains("quoted key"));
        assert!(apply("a.luau", "return {[\"other\"] = 1}", &[assign("mode", "3")]).is_ok());
    }

    /// What Apply writes runs on the engine as the VRAM the pokes made: cells packed back into the
    /// same words, the generated sheet placed at its char base.
    #[test]
    fn applied_cells_and_sheet_rebuild_the_poked_vram() {
        let words: Vec<u16> = (0..32).map(|i| 0x0101 * (i + 1)).collect();
        let (payload, _) =
            crate::generated_source(crate::SourceKind::Sheet, 4, &words, None).unwrap();
        let entries = [0x0001u16, 0x2C02, 0xC3FF, 0x5C01];
        let edits: Vec<Edit> = [
            place("t-tiles-1000", Some(0x1000)),
            assign("bg[1].char_base", "0x1000"),
        ]
        .into_iter()
        .chain(
            entries
                .iter()
                .enumerate()
                .map(|(i, &w)| cell(1, i as u16 * 7, 3, w)),
        )
        .collect();
        let a = run("t.luau", "return {frame = function(t, f) end}\n", &edits);
        let mut e = crate::LuaEngine::new();
        e.add_source("t-tiles-1000", &payload.encode()).unwrap();
        e.set_sources(&[
            ("main.luau", "T = scenes.add(require(\"./t\"))\n"),
            ("t.luau", a.source.as_str()),
        ])
        .unwrap();
        e.frame(0.0, 0).unwrap();
        let vram = &e.memory().vram;
        assert_eq!(&vram[0x1000..0x1020], &words[..], "{}", a.source);
        for (i, &w) in entries.iter().enumerate() {
            assert_eq!(vram[3 * 32 + i * 7], w, "cell {i}");
        }
    }

    #[test]
    fn equal_cells_group_into_rectangles_greedily_down_then_right() {
        let r = |layer, x, y, w, h, word| Rect { layer, x, y, w, h, word };
        // a 2x3 block of 5 with a 7 beside its top, and another layer's equal cell apart
        let cells = [(1, 0, 0, 5), (1, 0, 1, 5), (1, 0, 2, 5), (1, 1, 0, 5), (1, 1, 1, 5), (1, 1, 2, 5), (1, 2, 0, 7), (2, 0, 0, 5)];
        assert_eq!(map_rects(&cells), [r(1, 0, 0, 2, 3, 5), r(1, 2, 0, 1, 1, 7), r(2, 0, 0, 1, 1, 5)]);
        // an L: the column grows down first, the rest of the row is its own
        let l = [(1, 0, 0, 1), (1, 0, 1, 1), (1, 1, 1, 1)];
        assert_eq!(map_rects(&l), [r(1, 0, 0, 1, 2, 1), r(1, 1, 1, 1, 1, 1)]);
        let mut shuffled = cells;
        shuffled.reverse();
        assert_eq!(map_rects(&shuffled), map_rects(&cells), "the order cells come in does not matter");
    }

    /// The video's pipe: 3 columns of a 64-row map, body tiles 1..3 above and below a gap at rows
    /// 30..36, cap tiles 4..6 on the rows either side of it.
    fn pipe() -> Vec<(u16, u16, u16)> {
        (0..3u16)
            .flat_map(|c| (0..64u16).map(move |y| (c, y)))
            .filter(|&(_, y)| !(30..37).contains(&y))
            .map(|(c, y)| {
                let cap = y == 29 || y == 37;
                (10 + c, y, 0x0400 | if cap { 4 + c } else { 1 + c })
            })
            .collect()
    }

    #[test]
    fn a_painted_pipe_applies_as_a_handful_of_fills() {
        let edits: Vec<Edit> = pipe().into_iter().map(|(x, y, w)| cell(2, x, y, w)).collect();
        assert_eq!(edits.len(), 171);
        let a = run("a.luau", "return {}", &edits);
        let lines: Vec<&str> = a.source.lines().skip(1).take_while(|l| *l != "end").map(str::trim).collect();
        assert_eq!(
            lines,
            [
                "bg[2].map:fill(10, 0, 1, 29, {tile = 1, pal = 1})",
                "bg[2].map:fill(10, 29, 1, 1, {tile = 4, pal = 1})",
                "bg[2].map:fill(10, 37, 1, 1, {tile = 4, pal = 1})",
                "bg[2].map:fill(10, 38, 1, 26, {tile = 1, pal = 1})",
                "bg[2].map:fill(11, 0, 1, 29, {tile = 2, pal = 1})",
                "bg[2].map:fill(11, 29, 1, 1, {tile = 5, pal = 1})",
                "bg[2].map:fill(11, 37, 1, 1, {tile = 5, pal = 1})",
                "bg[2].map:fill(11, 38, 1, 26, {tile = 2, pal = 1})",
                "bg[2].map:fill(12, 0, 1, 29, {tile = 3, pal = 1})",
                "bg[2].map:fill(12, 29, 1, 1, {tile = 6, pal = 1})",
                "bg[2].map:fill(12, 37, 1, 1, {tile = 6, pal = 1})",
                "bg[2].map:fill(12, 38, 1, 26, {tile = 3, pal = 1})",
            ],
            "{}",
            a.source
        );
    }

    #[test]
    fn a_cell_under_an_applied_fill_goes_after_the_fill() {
        let src = "local function init()\n  bg[1].map[3][4] = {tile = 1, pal = 0}\n  bg[1].map:fill(3, 0, 1, 9, {tile = 2, pal = 0})\nend\nreturn {init = init}\n";
        let a = run("a.luau", src, &[cell(1, 3, 4, 7)]);
        assert_eq!(a.source, src.replace("pal = 0})\nend", "pal = 0})\n  bg[1].map[3][4] = {tile = 7, pal = 0}\nend"));
    }

    /// The applied pipe and a painted block of one entry run on the engine as the VRAM the pokes made.
    #[test]
    fn applied_fills_rebuild_the_poked_map() {
        let mut painted = pipe();
        painted.extend((20..24).flat_map(|x| (5..8).map(move |y| (x, y, 0xE3FF)))); // prio, flips, tile 1023
        let edits: Vec<Edit> = painted.iter().map(|&(x, y, w)| cell(2, x, y, w)).collect();
        let a = run("t.luau", "local function init()\n  bg[2].map_base = 0x4000\n  bg[2].screen_size = 2\nend\nreturn {init = init, frame = function(t, f) end}\n", &edits);
        assert!(a.source.contains("bg[2].map:fill(20, 5, 4, 3, {tile = 1023, pal = 0, prio = 1, flip_x = true, flip_y = true})"), "{}", a.source);
        let mut e = crate::LuaEngine::new();
        e.set_sources(&[("main.luau", "T = scenes.add(require(\"./t\"))\n"), ("t.luau", a.source.as_str())]).unwrap();
        e.frame(0.0, 0).unwrap();
        let mut want = vec![0u16; 0x800];
        for &(x, y, w) in &painted {
            want[(y as usize / 32) * 0x400 + (y as usize % 32) * 32 + x as usize] = w;
        }
        assert_eq!(&e.memory().vram[0x4000..0x4800], &want[..], "{}", a.source);
    }

    fn sprite(slot: u8, sheet: &str, cell: u16, x: i32, y: i32) -> Edit {
        drawn(slot, sheet, cell, x, y, 3, false, false)
    }

    #[allow(clippy::too_many_arguments)]
    fn drawn(
        slot: u8,
        sheet: &str,
        cell: u16,
        x: i32,
        y: i32,
        prio: u8,
        flip_x: bool,
        flip_y: bool,
    ) -> Edit {
        Edit::Sprite {
            slot,
            sheet: sheet.into(),
            cell,
            size: 16,
            x,
            y,
            prio,
            flip_x,
            flip_y,
        }
    }

    #[test]
    fn a_sprite_is_a_group_draw_in_frame_with_its_art_above() {
        let edits = [
            place("t-sprites-6000", Some(0x6000)),
            sprite(0, "t-sprites-6000", 0, 40, 50),
            drawn(1, "t-sprites-6000", 1, -4, 200, 1, true, false),
            sprite(2, "t-sprites-6000", 0, 90, 60),
        ];
        let a = run("t.luau", "return {}", &edits);
        assert_eq!(
            a.source,
            "local t_sprites_6000 = dma(\"t-sprites-6000\", {char = 0x6000})\n\
             local actors = sprites.group()\n\
             local sprite_0 = sprites.art({{size = 16, columns = 1, cells = {0}}}, t_sprites_6000)\n\
             local sprite_1 = sprites.art({{size = 16, columns = 1, cells = {1}}}, t_sprites_6000)\n\n\
             local function frame(t, f)\n  actors:draw(sprite_0, 40, 50)\n  actors:draw(sprite_1, -4, 200, {prio = 1, flip_x = true})\n  \
             actors:draw(sprite_0, 90, 60)\nend\n\nreturn {frame = frame}"
        );
        assert_eq!(a.written, [1, 2, 3, 4, 7, 8, 9]);
        assert!(!a.source.contains("obj["));
    }

    #[test]
    fn a_scene_handle_group_and_art_are_reused_and_new_lines_go_above_frame() {
        let src = "\
local hero = dma(\"hero\", {char = 0x6000})
local sprite_0 = 1
local crowd = sprites.group({slots = 4})
local actors = sprites.group()

local function init()
  mode = 1
end

local function frame(t, f)
  actors:draw(nil, 0, 0)
end

local idle = sprites.art({{size = 16, columns = 1, cells = {3}}}, hero)

return {init = init, frame = frame}
";
        let a = run(
            "t.luau",
            src,
            &[sprite(0, "hero", 0, 10, 20), sprite(1, "hero", 0, 30, 20)],
        );
        assert_eq!(
            a.source,
            src.replace(
                "local function frame(t, f)\n  actors:draw(nil, 0, 0)\n",
                "local sprite_0_2 = sprites.art({{size = 16, columns = 1, cells = {0}}}, hero)\n\n\
                 local function frame(t, f)\n  actors:draw(nil, 0, 0)\n  actors:draw(sprite_0_2, 10, 20)\n  actors:draw(sprite_0_2, 30, 20)\n"
            ),
            "a plain group is reused, not the one with slots; the art after frame is not"
        );
        let again = run("t.luau", &a.source, &[sprite(4, "hero", 0, 1, 2)]);
        assert!(
            again
                .source
                .contains("  actors:draw(sprite_0_2, 1, 2)\nend"),
            "the same art is reused: {}",
            again.source
        );
        assert_eq!(again.source.matches("sprites.art(").count(), 2);
    }

    #[test]
    fn a_bare_placement_gets_a_local_and_programs_get_a_global_frame() {
        let src = "dma(\"hero\", {char = 0x6000})\n\nfunction init()\n  mode = 1\nend\n";
        let a = run("main.luau", src, &[sprite(0, "hero", 2, 8, 8)]);
        assert_eq!(
            a.source,
            "local hero = dma(\"hero\", {char = 0x6000})\n\nfunction init()\n  mode = 1\nend\n\n\
             local actors = sprites.group()\nlocal sprite_2 = sprites.art({{size = 16, columns = 1, cells = {2}}}, hero)\n\n\
             function frame(t, f)\n  actors:draw(sprite_2, 8, 8)\nend\n"
        );
        for odd in [
            "local M = {}\nfunction M.frame(t, f)\nend\nreturn M\n",
            "local M = {}\nreturn M\n",
            "return {frame = function(t, f) end}\n",
        ] {
            let e = apply(
                "m.luau",
                odd,
                &[place("s", Some(0)), sprite(0, "s", 0, 1, 1)],
            )
            .unwrap_err();
            assert!(e.contains("local function frame(t, f)"), "{e}");
        }
        let named = run(
            "n.luau",
            "local function frame(t, f)\nend\nreturn {frame = frame}\n",
            &[place("s", Some(0)), sprite(0, "s", 0, 1, 1)],
        );
        assert!(
            named
                .source
                .starts_with("local s = dma(\"s\", {char = 0x0000})\n"),
            "{}",
            named.source
        );
        let both = run(
            "b.luau",
            "return {}",
            &[
                assign("mode", "1"),
                place("s", Some(0)),
                sprite(0, "s", 0, 1, 1),
            ],
        );
        assert!(
            both.source.ends_with("return {init = init, frame = frame}"),
            "{}",
            both.source
        );
        let more = run(
            "c.luau",
            "return {other = 1}",
            &[
                assign("mode", "1"),
                place("s", Some(0)),
                sprite(0, "s", 0, 1, 1),
            ],
        );
        assert!(
            more.source
                .ends_with("return {init = init, frame = frame, other = 1}"),
            "{}",
            more.source
        );
    }

    #[test]
    fn sprites_refuse_raw_slots_an_unplaced_sheet_and_a_placement_below_frame() {
        let raw = "local function frame(t, f)\n  obj[3].x = f\nend\nreturn {frame = frame}\n";
        assert!(apply("t.luau", raw, &[sprite(3, "s", 0, 0, 0)])
            .unwrap_err()
            .contains("t.luau:2"));
        let computed = raw.replace("obj[3]", "obj[i]");
        assert!(apply("t.luau", &computed, &[sprite(0, "s", 0, 0, 0)])
            .unwrap_err()
            .contains("writes obj[...]"));
        let other = raw.replace("obj[3]", "obj[4]");
        let ok = apply(
            "t.luau",
            &other,
            &[place("s", Some(0)), sprite(3, "s", 0, 0, 0)],
        );
        assert!(
            ok.is_ok(),
            "another constant slot is the program's own: {ok:?}"
        );
        assert!(apply("t.luau", "return {}", &[sprite(0, "s", 0, 0, 0)])
            .unwrap_err()
            .contains("does not place the sprite sheet"));
        let below =
            "local function frame(t, f) end\nlocal s = dma(\"s\")\nreturn {frame = frame}\n";
        assert!(apply("t.luau", below, &[sprite(0, "s", 0, 0, 0)])
            .unwrap_err()
            .contains("move it above frame"));
        assert!(apply(
            "t.luau",
            "return {}",
            &[drawn(0, "s", 0, 0, 0, 4, false, false)]
        )
        .is_err());
    }

    /// What Apply writes for a sprite runs on the engine as the poked OAM entry: the sheet's tile,
    /// the art's palette, the drawn point less half the cell.
    #[test]
    fn an_applied_sprite_draws_the_poked_oam_entry() {
        let words: Vec<u16> = (0..16 * 19).map(|i| 0x0F0F ^ i as u16).collect();
        let cells = crate::GeneratedCells {
            cell_w: 16,
            cell_h: 16,
            cells: vec![crate::import::obj::ObjCell {
                tile: 1,
                pal: 5,
                flip_x: false,
                flip_y: false,
            }],
        };
        let (payload, _) =
            crate::generated_source(crate::SourceKind::Obj, 4, &words, Some(&cells)).unwrap();
        let edits = [
            place("t-sprites-6000", Some(0x6000)),
            drawn(7, "t-sprites-6000", 0, 108, 58, 2, false, true),
        ];
        let a = run("t.luau", "return {}", &edits);
        let mut e = crate::LuaEngine::new();
        e.add_source("t-sprites-6000", &payload.encode()).unwrap();
        e.set_sources(&[
            ("main.luau", "T = scenes.add(require(\"./t\"))\n"),
            ("t.luau", a.source.as_str()),
        ])
        .unwrap();
        e.frame(0.0, 0).unwrap();
        let m = e.memory();
        assert_eq!(
            &m.vram[0x6000..0x6000 + words.len()],
            &words[..],
            "{}",
            a.source
        );
        let shown: Vec<_> = m.oam.iter().filter(|o| o.on).collect();
        assert_eq!(shown.len(), 1, "{}", a.source);
        let o = shown[0];
        assert_eq!(
            (o.x, o.y, o.tile, o.pal, o.prio, o.flip_y, o.large),
            (100, 50, 1, 5, 2, true, true),
            "{}",
            a.source
        );
    }

    #[test]
    fn no_edits_is_no_change() {
        assert_eq!(run("title.luau", TITLE, &[]).source, TITLE);
    }

    fn hook(first: u8, last: u8, lines: &[&str], bake: bool) -> Edit {
        Edit::Hdma {
            first,
            last,
            lines: lines.iter().map(|l| l.to_string()).collect(),
            bake,
        }
    }

    #[test]
    fn a_plain_hook_goes_at_the_end_of_frame() {
        let a = run(
            "title.luau",
            TITLE,
            &[
                hook(
                    10,
                    99,
                    &["cgram[0] = ramp_color({{0, 0}, {89, 31}}, y)"],
                    false,
                ),
                hook(100, 140, &["brightness = 8"], false),
            ],
        );
        assert!(
            a.source.contains(
                "  bg[1].scroll.x = f\n  hdma(10, 99, function(y)\n    cgram[0] = ramp_color({{0, 0}, {89, 31}}, y)\n  end)\n  hdma(100, 140, function(y)\n    brightness = 8\n  end)\nend\n"
            ),
            "{}",
            a.source
        );
        assert_eq!(a.written, [13, 14, 15, 16, 17, 18]);
    }

    #[test]
    fn a_baked_hook_is_a_file_level_bake_called_from_frame() {
        let a = run(
            "title.luau",
            TITLE,
            &[hook(
                81,
                223,
                &["m7.a = ramp({{81, 1}, {223, 4}}, y)"],
                true,
            )],
        );
        assert!(a.source.contains("local scanlines = bakeHdma({}, 81, 223, function(y)\n  m7.a = ramp({{81, 1}, {223, 4}}, y)\nend)\n\nlocal function frame(t, f)\n"), "{}", a.source);
        assert!(
            a.source
                .contains("  bg[1].scroll.x = f\n  scanlines()\nend\n"),
            "{}",
            a.source
        );
        assert_eq!(a.written, [11, 12, 13, 17]);
    }

    #[test]
    fn a_scene_without_a_frame_gets_one_with_the_hook() {
        let a = run(
            "a.luau",
            "local function init()\n  mode = 1\nend\nreturn {init = init}\n",
            &[hook(0, 223, &["brightness = 8"], false)],
        );
        assert_eq!(a.source, "local function init()\n  mode = 1\nend\nlocal function frame(t, f)\n  hdma(0, 223, function(y)\n    brightness = 8\n  end)\nend\n\nreturn {frame = frame, init = init}\n");
    }

    #[test]
    fn hooks_refuse_an_inline_frame_and_a_bad_range() {
        let inline = "return {frame = function(t, f) end}\n";
        let e = apply("a.luau", inline, &[hook(0, 9, &["brightness = 8"], false)]).unwrap_err();
        assert!(e.contains("local function frame(t, f)"), "{e}");
        assert!(apply(
            "title.luau",
            TITLE,
            &[hook(9, 8, &["brightness = 8"], false)]
        )
        .is_err());
        assert!(apply(
            "title.luau",
            TITLE,
            &[hook(0, 224, &["brightness = 8"], false)]
        )
        .is_err());
        assert!(apply(
            "title.luau",
            TITLE,
            &[hook(0, 9, &["brightness = ="], false)]
        )
        .is_err());
    }
}
