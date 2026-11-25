//! Static facts about a program's use of the `sprites` library (sprites.lua), shared by the preview
//! with every other consumer of the program, so all lay out the same slots:
//!
//! - which OAM slots it indexes directly through `obj[...]`: those stay the program's own, and the
//!   library assigns the rest. An `obj` index must be a literal, a constant, a numeric `for`
//!   variable with constant bounds, or arithmetic on those;
//! - its art (`sprites.art` calls, numbered in source order) and its groups (`sprites.group` calls,
//!   in source order, then the default group of plain `sprites.draw`), each with the slots its
//!   draws can fill in one frame: per draw, the times it can run a frame (its loops' constant trip
//!   counts, times its function's calls in the file) by the most cells its art can show;
//! - every group `draw` and `keep` call, with its group.
use full_moon::{
    ast::{self, Expression},
    node::Node,
    tokenizer::TokenReference,
    visitors::Visitor,
};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::ops::Range;

#[path = "sprite_flow.rs"]
mod flow;
#[path = "sprite_scope.rs"]
mod scope;
#[path = "sprite_helpers.rs"]
mod helpers;
pub use helpers::expand as expand_helpers;

/// The hosted ident's scene module, which the compiler puts in front of a game (games may not name a file `__ppu_`).
/// It runs only its own code, so the program's groups and draws never run in it.
pub const HOSTED_IDENT: &str = "__ppu_ident.luau";
pub const UNPROVABLE_SLOT: &str = "can't tell which sprite slots this obj write uses; give the index constant bounds or draw it with sprites.draw";
pub const UNSIZED: &str = "can't tell how many sprites this draw adds to its group: bound its loops with constants or a file-level table, draw art made at file level (or a file-level table of it), or give the group {slots = n}";
pub const NO_GROUP: &str = "can't tell which sprites group this is: call draw and keep on a group made with sprites.group in this file, or on one a module made and returns (`Art.ships:draw(...)`)";
pub const GROUP_PLACE: &str = "make sprites groups at file level or in init(), outside loops: each sprites.group call is one group";
pub const NO_SORT: &str = "sprites.sort() is gone: draw what sorts by depth into a group made with sprites.group({sort = true})";
pub const NO_LAYER: &str =
    "sprites.draw takes no layer: draw into groups, which draw in the order they are made";
pub const NO_SORTED_ROWS: &str = "sprites.rows clips as sprites are written, and a sorted group writes its sprites at the end of the frame: use one or the other";
pub const SIZE_SEL: &str = "with the sprites library, set obj.size_sel to one constant 0..7 (or leave it out: the library picks the size pair from the art)";
const DRAW_OPTIONS: &str = "draw takes (art, x, y) and an optional literal options table {size = , prio = , key = , depth = , flip_x = , flip_y = , pal = , at = }";
pub const ANIMATION_ARTS: &str = "sprites.animation takes a table of art made at file level: sprites.art calls, or names bound to them";
/// Where an art number stands for animation n (from 1) while files are walked: `ANIMATION + n`.
const ANIMATION: usize = 1 << 20;

/// Where the analysis rejects a program.
#[derive(Debug, Clone, PartialEq)]
pub struct Error {
    pub file: String,
    pub line: u32,
    pub message: String,
}

/// A call in a source file: its line, the bytes of its callee and open parenthesis, and of the whole call.
#[derive(Debug, Clone, PartialEq)]
pub struct Site {
    pub file: String,
    pub line: u32,
    pub head: Range<usize>,
    pub call: Range<usize>,
}

/// A group: its `sprites.group` call (none for the default group), whether it sorts by depth, its
/// `slots` option, and per draw into it the times it runs a frame and the art it can draw (art
/// numbers, from 1); `None` where the analysis cannot tell, which needs `slots`.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub site: Option<Site>,
    pub sort: bool,
    pub slots: Option<u32>,
    pub draws: Vec<(String, u32, Option<u64>, Option<Vec<usize>>)>,
    /// `{keep = true}`: its keyed sprites stay until drawn again, with no keep calls.
    pub keep: bool,
    /// The scenes that reach the module that made it (from 1, as `Plan::scenes`); empty: the program's, in every scene.
    pub scenes: Vec<usize>,
    /// Drawn in a file no scene owns (a module main.luau requires), or made there and never drawn: it counts in every scene.
    pub program_wide: bool,
    /// Where `program_wide` is because a draw sits in a file no scene owns: that first draw.
    pub program_draw: Option<(String, u32)>,
}

impl Group {
    /// Whether it draws in scene `k` (from 1).
    pub fn in_scene(&self, k: usize) -> bool {
        self.scenes.is_empty() || self.scenes.contains(&k)
    }
}

/// A `draw` or `keep` on a group (`sprites.draw`/`sprites.keep` on the default group): the call,
/// its group, its arguments and its option fields (name and value).
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    pub file: String,
    pub line: u32,
    pub call: Range<usize>,
    pub group: usize,
    pub keep: bool,
    pub args: Vec<Range<usize>>,
    pub options: Vec<(String, Range<usize>)>,
    /// A draw's art numbers (from 1), `None` when the analysis cannot tell (or a keep). An animation it can draw
    /// counts as its arts.
    pub arts: Option<Vec<usize>>,
    /// The animations (numbers, from 1) a draw can show; `None` when the analysis cannot tell which art it draws.
    pub animations: Option<Vec<usize>>,
    /// A draw on a parameter of a file-level function (`g:draw(...)` in `local function hearts(g)`), which every
    /// caller gives a group: its name. The draw then has one call here per group it can draw into.
    pub through: Option<String>,
}

/// An argument of a call of a file-level function that gives its parameter a group (`hearts(things)`,
/// `Groups.hearts(Groups.row)`): where it is and the group.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupArg {
    pub file: String,
    pub arg: Range<usize>,
    pub group: usize,
}

/// A `sprites.animation` call: its site and its art numbers (from 1).
#[derive(Debug, Clone, PartialEq)]
pub struct Animation {
    pub site: Site,
    pub arts: Vec<usize>,
}

/// A program that draws with the sprites library.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// Slots the program indexes through `obj[...]` outside every scene, ascending; the library never assigns them.
    pub reserved: Vec<u8>,
    /// A program of scenes: each scene's module file (in `scenes.add` order) and the slots its own modules index
    /// through `obj[...]`. Two scenes never run together, so each lays its groups out over the whole OAM.
    pub scenes: Vec<(String, Vec<u8>)>,
    /// Per scene (as `scenes`), where its first `obj[...]` write is, for the layout refusal.
    pub scene_writes: Vec<Option<(String, u32)>>,
    /// `sprites.art` calls in source order: art n is `arts[n - 1]`.
    pub arts: Vec<Site>,
    /// `sprites.animation` calls in source order: animation n is `animations[n - 1]`.
    pub animations: Vec<Animation>,
    /// The groups in draw order (the first on top), the default group last, which takes the free
    /// slots the others leave.
    pub groups: Vec<Group>,
    pub calls: Vec<Call>,
    /// The groups calls give to parameters that draws are made on (`Call::through`).
    pub group_args: Vec<GroupArg>,
    /// The OBJSEL size pair the program sets itself (`obj.size_sel = n`, one constant), which the library then
    /// draws with; `None` lets the library choose it from the art.
    pub size_sel: Option<u8>,
    /// The program calls `sprites.rows`.
    pub rows: bool,
    /// The program writes `obj.char_base` itself; otherwise the library sets it from its art's sheets.
    pub sets_char_base: bool,
    /// The program writes `screen.main.obj` or `screen.sub.obj` (or TM/TS) itself; otherwise the library shows the
    /// sprite layer on the main screen.
    pub sets_obj_layer: bool,
    /// Where an animation goes somewhere the plan can't follow it (`sprite_flow`): the cartridge refuses these.
    pub escapes: Vec<Error>,
}

impl Plan {
    /// The slots the library assigns, ascending.
    pub fn free(&self) -> Vec<u8> {
        (0..128u8).filter(|s| !self.reserved.contains(s)).collect()
    }

    /// Each group's first free position (from 0) and slot count, given the most cells each art
    /// (by number, from 1) shows: its `slots`, or the sum over its draws of times by cells. Groups
    /// take the free slots in order; past them is an error at the first group that does not fit.
    /// In a program of scenes each scene starts over past the program's groups (a scene's groups
    /// share positions with other scenes'), skipping the slots its own `obj[...]` writes take.
    pub fn layout(&self, cells: &[u64]) -> Result<Vec<(u32, u32)>, Error> {
        let free_slots = self.free();
        let free = free_slots.len() as u64;
        // Per scene (one for a program without scenes): where its groups end, and its own obj slots' positions.
        let n = self.scenes.len().max(1);
        let mut ends = vec![0u64; n];
        let taken: Vec<Vec<u64>> = (0..n)
            .map(|k| {
                let own = self.scenes.get(k).map_or(&[][..], |(_, r)| r.as_slice());
                (0..free)
                    .filter(|&p| own.contains(&free_slots[p as usize]))
                    .collect()
            })
            .collect();
        let mut out: Vec<(u32, u32)> = vec![];
        for g in &self.groups {
            let live: Vec<usize> = (0..n).filter(|k| g.in_scene(k + 1)).collect();
            let mut at = live.iter().map(|&k| ends[k]).max().unwrap_or(0);
            let size = match g.slots {
                Some(n) => n as u64,
                // The default group takes the slots the others leave.
                None if g.site.is_none() && self.scenes.is_empty() => free.saturating_sub(at),
                // In a program of scenes the default group takes what its scenes (the one its draws are in, else
                // every scene) leave past their groups, none when nothing draws into it.
                None if g.site.is_none() => {
                    let room = |&k: &usize| {
                        taken[k]
                            .iter()
                            .copied()
                            .filter(|&p| p >= at)
                            .min()
                            .unwrap_or(free)
                            - at
                    };
                    if g.draws.is_empty() {
                        0
                    } else {
                        live.iter().map(room).min().unwrap_or(0)
                    }
                }
                None => g
                    .draws
                    .iter()
                    .map(|(_, _, times, arts)| {
                        let most = arts
                            .iter()
                            .flatten()
                            .map(|a| cells.get(a - 1).copied().unwrap_or(1))
                            .max()
                            .unwrap_or(0);
                        times.unwrap_or(0) * most
                    })
                    .sum(),
            };
            // Past the scene's own obj slots: a group is one run of positions.
            while let Some(&p) = live
                .iter()
                .flat_map(|&k| &taken[k])
                .filter(|&&p| p >= at && p < at + size)
                .max()
            {
                at = p + 1;
            }
            if at + size > free {
                let (file, line) = match (&g.site, g.draws.first()) {
                    (Some(s), _) => (s.file.clone(), s.line),
                    (None, Some((f, l, _, _))) => (f.clone(), *l),
                    _ => (String::new(), 0),
                };
                // The scene with the least room (the most positions used by groups and its own obj writes).
                let scene = live
                    .iter()
                    .copied()
                    .max_by_key(|&k| ends[k] + taken[k].iter().filter(|&&p| p >= ends[k]).count() as u64);
                let own = scene.map_or(0, |k| taken[k].len() as u64);
                let before: Vec<(usize, u32)> = (0..out.len())
                    .filter(|&j| scene.is_some_and(|k| self.groups[j].in_scene(k + 1)))
                    .map(|j| (j, out[j].1))
                    .filter(|&(_, n)| n > 0)
                    .collect();
                let room = free - own - before.iter().map(|&(_, n)| n as u64).sum::<u64>();
                return Err(Error {
                    file,
                    line,
                    message: self.refusal(g, size, room, scene, own, &before),
                });
            }
            out.push((at as u32, size as u32));
            for &k in &live {
                ends[k] = at + size;
            }
        }
        Ok(out)
    }

    /// What `layout` says when a group does not fit: the group, the scene without room (none in a program without
    /// scenes), what filled it, why the group counts there and what to change. One line, for diagnostics.
    fn refusal(
        &self,
        g: &Group,
        size: u64,
        room: u64,
        scene: Option<usize>,
        own: u64,
        before: &[(usize, u32)],
    ) -> String {
        let name = |g: &Group| match (&g.site, g.draws.first()) {
            (Some(s), _) => format!("the sprites group at {}:{}", s.file, s.line),
            (None, Some((f, l, _, _))) => format!("the default sprites group drawn at {f}:{l}"),
            _ => "the default sprites group".to_string(),
        };
        let scene_file = scene.and_then(|k| self.scenes.get(k)).map(|(f, _)| f);
        let slots = |n: u64| format!("{n} slot{}", if n == 1 { "" } else { "s" });
        let mut text = format!("{} needs {} in a row", name(g), slots(size));
        match (scene_file, room >= size) {
            (Some(f), false) => text += &format!(" and {f} has {room} left"),
            (Some(f), true) => text += &format!(" and {f} has {room} left but not {size} in a row"),
            (None, false) => text += &format!(" and {room} are left"),
            (None, true) => text += &format!(" and {room} are left but not {size} in a row"),
        }
        let mut filled = vec![];
        if own > 0 {
            let first = scene
                .and_then(|k| self.scene_writes.get(k))
                .and_then(|w| w.as_ref())
                .map_or(String::new(), |(f, l)| format!(", the first at {f}:{l}"));
            filled.push(format!("its obj[] writes take {}{first}", slots(own)));
        } else if scene_file.is_none() && !self.reserved.is_empty() {
            filled.push(format!("the program's obj[] writes take {}", slots(self.reserved.len() as u64)));
        }
        for &(j, n) in before {
            filled.push(format!("{} takes {}", name(&self.groups[j]), slots(n as u64)));
        }
        if !filled.is_empty() {
            text += &format!(": {}", filled.join("; "));
        }
        // A program-wide group counts everywhere because of its first draw in a file no scene owns (a group only made in a scene counts there, whoever draws it).
        let program_draw = g.program_draw.as_ref().filter(|_| g.program_wide);
        let drawn = match (program_draw, g.draws.first(), &g.site) {
            (Some((f, l)), ..) if f == "main.luau" => Some(format!("drawn in {f}:{l}")),
            (Some((f, l)), ..) => Some(format!("drawn in {f}:{l}, which main.luau requires")),
            (None, Some((f, l, _, _)), _) => Some(format!("drawn in {f}:{l}")),
            (None, None, Some(s)) => Some(format!("made in {}:{}", s.file, s.line)),
            _ => None,
        };
        match (scene_file, g.program_wide, drawn) {
            (Some(f), true, Some(d)) => {
                text += &format!(
                    "; the group is {d}, so it counts in every scene: make the group in a module of groups that the scenes require (not main.luau) and draw it from the scenes' own files, e.g. `local Groups = require(\"./groups\")` and `Groups.name:draw(...)` in {f}; the art can stay in the module main.luau requires so it still loads in every scene"
                )
            }
            (Some(_), _, Some(d)) => {
                text += &format!("; the group is {d}: make fewer obj[] writes or smaller groups in that scene")
            }
            _ => text += "; make fewer obj[] writes or smaller groups",
        }
        text
    }
}

type Set = BTreeSet<i64>;

#[derive(Clone)]
enum Bound {
    Numbers(Set),
    Fields(HashMap<String, i64>),
    Unknown,
}

fn text(t: &TokenReference) -> String {
    t.token().to_string()
}

fn line(node: &impl Node) -> u32 {
    node.start_position().map_or(0, |p| p.line() as u32)
}

/// `a.b.c(...)` as "a.b.c" with its arguments, for a plain parenthesized call.
fn path(call: &ast::FunctionCall) -> Option<(String, Vec<&Expression>)> {
    let ast::Prefix::Name(root) = call.prefix() else {
        return None;
    };
    let mut name = text(root);
    let suffixes: Vec<_> = call.suffixes().collect();
    let (last, fields) = suffixes.split_last()?;
    for s in fields {
        let ast::Suffix::Index(ast::Index::Dot { name: field, .. }) = s else {
            return None;
        };
        name = format!("{name}.{}", text(field));
    }
    let ast::Suffix::Call(ast::Call::AnonymousCall(args)) = last else {
        return None;
    };
    let ast::FunctionArgs::Parentheses { arguments, .. } = args.as_ref() else {
        return None;
    };
    Some((name, arguments.iter().collect()))
}

fn combine(a: &Set, b: &Set, op: impl Fn(i64, i64) -> Option<i64>) -> Option<Set> {
    if a.len().saturating_mul(b.len()) > 1 << 14 {
        return None;
    }
    let mut out = Set::new();
    for x in a {
        for y in b {
            out.insert(op(*x, *y)?);
        }
    }
    Some(out)
}

/// The values a constant expression can take, given the names in scope.
fn values(e: &Expression, lookup: &dyn Fn(&str) -> Bound) -> Option<Set> {
    match e {
        Expression::Number(t) => {
            let v: f64 = text(t).parse().ok()?;
            (v.fract() == 0.0 && v.abs() < 1e9).then(|| Set::from([v as i64]))
        }
        Expression::Parentheses { expression, .. } => values(expression, lookup),
        Expression::UnaryOperator {
            unop: ast::UnOp::Minus(_),
            expression,
        } => Some(
            values(expression, lookup)?
                .into_iter()
                .map(|v| -v)
                .collect(),
        ),
        Expression::BinaryOperator { lhs, binop, rhs } => {
            let (a, b) = (values(lhs, lookup)?, values(rhs, lookup)?);
            match binop {
                ast::BinOp::Plus(_) => combine(&a, &b, |x, y| Some(x + y)),
                ast::BinOp::Minus(_) => combine(&a, &b, |x, y| Some(x - y)),
                ast::BinOp::Star(_) => combine(&a, &b, |x, y| Some(x * y)),
                ast::BinOp::DoubleSlash(_) => {
                    combine(&a, &b, |x, y| (y != 0).then(|| x.div_euclid(y)))
                }
                ast::BinOp::Percent(_) => combine(&a, &b, |x, y| (y != 0).then(|| x.rem_euclid(y))),
                _ => None,
            }
        }
        Expression::Var(ast::Var::Name(n)) => match lookup(&text(n)) {
            Bound::Numbers(s) => Some(s),
            _ => None,
        },
        Expression::Var(ast::Var::Expression(v)) => {
            let ast::Prefix::Name(root) = v.prefix() else {
                return None;
            };
            let [ast::Suffix::Index(ast::Index::Dot { name, .. })] =
                v.suffixes().collect::<Vec<_>>()[..]
            else {
                return None;
            };
            match lookup(&text(root)) {
                Bound::Fields(f) => f.get(&text(name)).map(|v| Set::from([*v])),
                _ => None,
            }
        }
        _ => None,
    }
}

/// `table.freeze({name = constant, ...})`: its numeric fields.
fn frozen(e: &Expression) -> Option<HashMap<String, i64>> {
    let Expression::FunctionCall(call) = e else {
        return None;
    };
    let (name, args) = path(call)?;
    let (true, [Expression::TableConstructor(t)]) = (name == "table.freeze", &args[..]) else {
        return None;
    };
    let mut out = HashMap::new();
    for field in t.fields() {
        if let ast::Field::NameKey { key, value, .. } = field {
            if let Some(s) = values(value, &|_| Bound::Unknown).filter(|s| s.len() == 1) {
                out.insert(text(key), *s.first().unwrap());
            }
        }
    }
    Some(out)
}

/// A file-level value as the plan reads art in it: an art's number (an animation's is `ANIMATION + n`), a table of
/// them (fields by name, positional ones by their index), nested to any depth, or anything else.
#[derive(Clone, Debug)]
enum Tree {
    Art(usize),
    Table(Vec<(String, Tree)>),
    Other,
}

/// One step of a path into a tree: a field (`.name`, `[2]`, `["name"]`), or any field (`[k]`).
#[derive(Clone, Debug)]
enum Step {
    Key(String),
    Any,
}

impl Tree {
    /// The nodes `steps` reach, none when one of its keys is missing or it indexes into something not a table.
    fn at<'t>(&'t self, steps: &[Step]) -> Option<Vec<&'t Tree>> {
        let Some((step, rest)) = steps.split_first() else {
            return Some(vec![self]);
        };
        let Tree::Table(fields) = self else { return None };
        match step {
            Step::Key(k) => fields.iter().find(|(f, _)| f == k)?.1.at(rest),
            Step::Any => fields.iter().try_fold(vec![], |mut all, (_, t)| {
                all.extend(t.at(rest)?);
                Some(all)
            }),
        }
    }
    /// Its art numbers when it is an art or a table of art (one level, as a draw's `T[k]` reads it).
    fn arts(&self) -> Option<Vec<usize>> {
        match self {
            Tree::Art(a) => Some(vec![*a]),
            Tree::Table(fields) => fields.iter().map(|(_, t)| match t {
                Tree::Art(a) => Some(*a),
                _ => None,
            }).collect(),
            Tree::Other => None,
        }
    }
    /// Whether any animation lies in it.
    fn animates(&self) -> bool {
        match self {
            Tree::Art(a) => *a > ANIMATION,
            Tree::Table(fields) => fields.iter().any(|(_, t)| t.animates()),
            Tree::Other => false,
        }
    }
    fn has_art(&self) -> bool {
        match self {
            Tree::Art(_) => true,
            Tree::Table(fields) => fields.iter().any(|(_, t)| t.has_art()),
            Tree::Other => false,
        }
    }
}

/// The art a path of nodes shows: every node an art (`None` otherwise).
fn arts_of(nodes: &[&Tree]) -> Option<Vec<usize>> {
    nodes.iter().map(|t| match t {
        Tree::Art(a) => Some(*a),
        _ => None,
    }).collect()
}

/// A var's suffixes as steps into a tree (none for a call).
fn steps(suffixes: &[&ast::Suffix]) -> Option<Vec<Step>> {
    suffixes
        .iter()
        .map(|s| match s {
            ast::Suffix::Index(ast::Index::Dot { name, .. }) => Some(Step::Key(text(name))),
            ast::Suffix::Index(ast::Index::Brackets { expression, .. }) => Some(match &**expression {
                Expression::Number(n) => match text(n).parse::<f64>() {
                    Ok(v) if v.fract() == 0.0 => Step::Key((v as i64).to_string()),
                    _ => Step::Any,
                },
                Expression::String(t) => Step::Key(text(t).trim_matches(|c| c == '"' || c == '\'').to_string()),
                _ => Step::Any,
            }),
            _ => None,
        })
        .collect()
}

/// A draw or keep found in a file, before its group's name is resolved.
struct Found {
    line: u32,
    call: Range<usize>,
    receiver: Option<String>,
    /// The module and field of a group the receiver names (`M.f`, or `g` from `local g = M.f`).
    module_group: Option<(String, String)>,
    keep: bool,
    args: Vec<Range<usize>>,
    options: Vec<(String, Range<usize>)>,
    /// Draws: the function it is in, its loops' trip count, and its art.
    function: usize,
    trips: Option<u64>,
    arts: Option<Vec<usize>>,
    /// Art a module returns, when the draw names it (`Art.ship`, `Art.SHIPS[k]`, either branch of a choice of them):
    /// resolved once every file is walked.
    art_parts: Option<Vec<ArtPart>>,
}

/// A group an expression names: one this file made, one a module returns, a parameter of a file-level function
/// (by its name and place), or none the walk knows.
#[derive(Clone, PartialEq)]
enum GroupRef {
    Known(usize),
    Module(String, String),
    Param(String, usize),
    Other,
}

/// A call of a function by name, `f(...)` or a module's `M.f(...)` (the module's stem), with what each argument is.
struct GroupCall {
    line: u32,
    module: Option<String>,
    name: String,
    args: Vec<(Range<usize>, GroupRef)>,
}

/// Art a draw can show: art numbers this file knows, or art a module returns.
#[derive(Clone)]
enum ArtPart {
    Known(Vec<usize>),
    Module(ArtRef),
}

/// Art a module returns: its stem, the field, and the path into it.
type ArtRef = (String, String, Vec<Step>);

struct Walk<'a> {
    file: &'a str,
    modules: &'a HashMap<String, HashMap<String, i64>>,
    /// The fields each module returns a group in, by stem: `M.f:draw(...)` is a group's draw only on one of these.
    group_fields: &'a HashMap<String, HashSet<String>>,
    /// The declaration each name reads: what the walk learns about a name it keys by declaration.
    scope: scope::Names,
    frames: Vec<Vec<(String, Bound)>>,
    pending: Vec<(String, Bound)>,
    functions: usize,
    loops: usize,
    draws: bool,
    art: bool,
    slots: Vec<(u32, Option<Set>)>,
    error: Option<Error>,
    /// Art numbers so far (all files), each art call's span to its number, file-level art bindings.
    arts: &'a mut Vec<Site>,
    /// Animations so far (all files); an animation's number stands in art lists as `ANIMATION + n`.
    animations: &'a mut Vec<Animation>,
    art_at: HashMap<Range<usize>, usize>,
    art_names: HashMap<usize, usize>,
    art_tables: HashMap<usize, Tree>,
    /// Each animation's arts (its number from 0, its line), as the file names them.
    deferred: Vec<(usize, Vec<ArtPart>, u32)>,
    /// Locals bound to art (`local a = ART[k]`) and never assigned again: the art they can be.
    art_locals: HashMap<usize, Vec<usize>>,
    /// File-level tables and their field counts (generic `for` trip counts), and `@ppu/pools` pools and their sizes
    /// (`for x in pool:live()` runs at most that many times).
    tables: HashMap<String, u64>,
    pools: HashMap<String, u64>,
    /// Groups so far (all files); this file's group names.
    groups: &'a mut Vec<Group>,
    group_names: HashMap<String, usize>,
    group_at: HashMap<Range<usize>, usize>,
    /// Function bodies: their names ("" anonymous) and parameters, the stack of open ones, the name the next body takes.
    names: Vec<String>,
    params: Vec<Vec<String>>,
    /// Calls of a function by name that pass a group, or a parameter of the function around them.
    group_calls: Vec<GroupCall>,
    open: Vec<usize>,
    next_name: Option<String>,
    /// Trip counts of the open loops (None: unknown).
    trips: Vec<Option<u64>>,
    /// Calls by name: (calling function, callee name, trip count of the call's loops).
    called: Vec<(usize, String, Option<u64>)>,
    found: Vec<Found>,
    /// `name = sprites.group(...)`: the call's span and the name it is assigned to.
    pending_group: Option<(Range<usize>, String)>,
    /// File-level `local M = require("./m")`: the module stem each name holds; `local g = M.f`: the field it holds.
    requires: HashMap<usize, String>,
    module_fields: HashMap<usize, (String, String)>,
    /// The values `obj.size_sel` is set to (None: one that is not a constant 0..7), with their lines.
    size_sel: Vec<(u32, Option<u8>)>,
    rows: bool,
    /// It writes `obj.char_base`, and the sprite layer (`screen.main.obj`, `screen.sub.obj`, TM or TS).
    sets: (bool, bool),
}

fn span(node: &impl Node) -> Range<usize> {
    let mut tokens = node
        .tokens()
        .map(|t| t.token().start_position().bytes()..t.token().end_position().bytes());
    let first = tokens.next().unwrap_or(0..0);
    tokens.fold(first, |a, b| a.start.min(b.start)..a.end.max(b.end))
}

/// `X:method(args)` or `X.f:method(args)`: the receiver's name (`X`, or `X.f`), the method and its arguments.
fn method(call: &ast::FunctionCall) -> Option<(String, String, Vec<&Expression>)> {
    let ast::Prefix::Name(root) = call.prefix() else {
        return None;
    };
    let (receiver, m) = match call.suffixes().collect::<Vec<_>>()[..] {
        [ast::Suffix::Call(ast::Call::MethodCall(m))] => (text(root), m),
        [ast::Suffix::Index(ast::Index::Dot { name, .. }), ast::Suffix::Call(ast::Call::MethodCall(m))] => {
            (format!("{}.{}", text(root), text(name)), m)
        }
        _ => return None,
    };
    let ast::FunctionArgs::Parentheses { arguments, .. } = m.args() else {
        return None;
    };
    Some((receiver, text(m.name()), arguments.iter().collect()))
}

/// A literal options table's fields: (name, value).
fn fields<'a>(e: Option<&&'a Expression>) -> Option<Vec<(String, &'a Expression)>> {
    match e {
        None => Some(vec![]),
        Some(Expression::TableConstructor(t)) => t
            .fields()
            .iter()
            .map(|f| match f {
                ast::Field::NameKey { key, value, .. } => Some((text(key), &**value)),
                _ => None,
            })
            .collect(),
        _ => None,
    }
}

impl Walk<'_> {
    fn lookup(&self, name: &str) -> Bound {
        for frame in self.frames.iter().rev() {
            if let Some((_, b)) = frame.iter().rev().find(|(n, _)| n == name) {
                return b.clone();
            }
        }
        Bound::Unknown
    }
    fn values(&self, e: &Expression) -> Option<Set> {
        values(e, &|n| self.lookup(n))
    }
    fn fail(&mut self, node: &impl Node, message: &str) {
        self.error.get_or_insert(Error {
            file: self.file.into(),
            line: line(node),
            message: message.into(),
        });
    }
    fn trip_product(&self) -> Option<u64> {
        self.trips
            .iter()
            .try_fold(1u64, |a, t| t.map(|t| a.saturating_mul(t)))
    }
    /// The stem of the module `M` a `M.f:method(...)` call is made on.
    fn receiver_module(&self, call: &ast::FunctionCall) -> Option<&String> {
        let ast::Prefix::Name(m) = call.prefix() else { return None };
        self.requires.get(&self.scope.of(m)?)
    }
    /// The module and field of the group a `M.f:method(...)` or `g:method(...)` (`local g = M.f`) call is made on.
    fn module_group(&self, call: &ast::FunctionCall) -> Option<(String, String)> {
        let (receiver, ..) = method(call)?;
        match receiver.split_once('.') {
            Some((_, field)) => self.receiver_module(call).map(|stem| (stem.clone(), field.to_string())),
            None => {
                let ast::Prefix::Name(g) = call.prefix() else { return None };
                self.module_fields.get(&self.scope.of(g)?).cloned()
            }
        }
    }
    /// The group an argument names: a parameter of the function around it first (as Luau scopes it), then a group
    /// this file made, then one a module returns (`M.f`, or `g` from `local g = M.f`).
    fn group_ref(&self, e: &Expression) -> GroupRef {
        match e {
            Expression::Parentheses { expression, .. } => self.group_ref(expression),
            Expression::Var(ast::Var::Name(n)) => {
                let name = text(n);
                if let Some(&f) = self.open.last() {
                    if let Some(p) = self.params[f].iter().position(|q| *q == name) {
                        return if self.names[f].is_empty() { GroupRef::Other } else { GroupRef::Param(self.names[f].clone(), p) };
                    }
                }
                if let Some(&g) = self.group_names.get(&name) {
                    return GroupRef::Known(g);
                }
                match self.scope.of(n).and_then(|d| self.module_fields.get(&d)) {
                    Some((stem, field)) => GroupRef::Module(stem.clone(), field.clone()),
                    None => GroupRef::Other,
                }
            }
            Expression::Var(ast::Var::Expression(v)) => {
                let (ast::Prefix::Name(m), [ast::Suffix::Index(ast::Index::Dot { name: field, .. })]) = (v.prefix(), &v.suffixes().collect::<Vec<_>>()[..]) else {
                    return GroupRef::Other;
                };
                match self.scope.of(m).and_then(|d| self.requires.get(&d)) {
                    Some(stem) => GroupRef::Module(stem.clone(), text(field)),
                    None => GroupRef::Other,
                }
            }
            _ => GroupRef::Other,
        }
    }
    /// The art numbers an art argument can be: a file-level art, or a field of a file-level table of art.
    fn art_of(&self, e: &Expression) -> Option<Vec<usize>> {
        match e {
            Expression::Parentheses { expression, .. } => self.art_of(expression),
            // Luau's `if c then a else b`, as the preview runs it: `((c) and (a) or (b))`, either branch.
            Expression::BinaryOperator {
                lhs,
                binop: ast::BinOp::And(_),
                rhs,
            } if self.art_of(lhs).is_none() => self.art_of(rhs),
            Expression::BinaryOperator {
                lhs,
                binop: ast::BinOp::Or(_),
                rhs,
            } => {
                let mut arts = self.art_of(lhs)?;
                arts.extend(self.art_of(rhs)?);
                Some(arts)
            }
            Expression::Var(ast::Var::Name(n)) => {
                let d = self.scope.of(n)?;
                match self.art_names.get(&d) {
                    Some(a) => Some(vec![*a]),
                    None => self.art_locals.get(&d).cloned(),
                }
            }
            Expression::Var(ast::Var::Expression(v)) => {
                let ast::Prefix::Name(root) = v.prefix() else {
                    return None;
                };
                let table = self.art_tables.get(&self.scope.of(root)?)?;
                arts_of(&table.at(&steps(&v.suffixes().collect::<Vec<_>>())?)?)
            }
            _ => None,
        }
    }
    /// The art an argument can be, as art_of reads it, where some of it is art a module returns.
    fn art_parts(&self, e: &Expression) -> Option<Vec<ArtPart>> {
        if let Some(arts) = self.art_of(e) {
            return Some(vec![ArtPart::Known(arts)]);
        }
        match e {
            Expression::Parentheses { expression, .. } => self.art_parts(expression),
            Expression::BinaryOperator {
                lhs,
                binop: ast::BinOp::And(_),
                rhs,
            } if self.art_parts(lhs).is_none() => self.art_parts(rhs),
            Expression::BinaryOperator {
                lhs,
                binop: ast::BinOp::Or(_),
                rhs,
            } => {
                let mut parts = self.art_parts(lhs)?;
                parts.extend(self.art_parts(rhs)?);
                Some(parts)
            }
            _ => self.module_art(e).map(|r| vec![ArtPart::Module(r)]),
        }
    }
    /// The module art an art argument names: `M.f` and any path into it (`M.f[k]`, `M.f.g[1]`), or the same through
    /// `local x = M.f`.
    fn module_art(&self, e: &Expression) -> Option<ArtRef> {
        match e {
            Expression::Parentheses { expression, .. } => self.module_art(expression),
            Expression::Var(ast::Var::Name(n)) => self
                .module_fields
                .get(&self.scope.of(n)?)
                .map(|(m, f)| (m.clone(), f.clone(), vec![])),
            Expression::Var(ast::Var::Expression(v)) => {
                let ast::Prefix::Name(root) = v.prefix() else {
                    return None;
                };
                let path = steps(&v.suffixes().collect::<Vec<_>>())?;
                let d = self.scope.of(root)?;
                if let Some((m, f)) = self.module_fields.get(&d) {
                    return Some((m.clone(), f.clone(), path));
                }
                let stem = self.requires.get(&d)?;
                match path.split_first()? {
                    (Step::Key(field), rest) => Some((stem.clone(), field.clone(), rest.to_vec())),
                    (Step::Any, _) => None,
                }
            }
            _ => None,
        }
    }
    /// A file-level value as a tree: art calls, names of art and of tables of it, paths into those, and tables of
    /// any of these.
    fn tree_of(&self, e: &Expression) -> Tree {
        if let Some(a) = self.art_call(e) {
            return Tree::Art(a);
        }
        match e {
            Expression::Parentheses { expression, .. } => self.tree_of(expression),
            Expression::Var(ast::Var::Name(n)) => {
                let d = self.scope.of(n);
                match (d.and_then(|d| self.art_names.get(&d)), d.and_then(|d| self.art_tables.get(&d))) {
                    (Some(a), _) => Tree::Art(*a),
                    (None, Some(t)) => t.clone(),
                    _ => Tree::Other,
                }
            }
            Expression::Var(ast::Var::Expression(v)) => {
                let ast::Prefix::Name(root) = v.prefix() else { return Tree::Other };
                let at = steps(&v.suffixes().collect::<Vec<_>>())
                    .and_then(|p| self.art_tables.get(&self.scope.of(root)?)?.at(&p));
                match at.as_deref() {
                    Some([one]) => (*one).clone(),
                    _ => Tree::Other,
                }
            }
            Expression::TableConstructor(t) => Tree::Table(
                t.fields()
                    .iter()
                    .enumerate()
                    .map(|(k, f)| match f {
                        ast::Field::NameKey { key, value, .. } => (text(key), self.tree_of(value)),
                        ast::Field::NoKey(value) => ((k + 1).to_string(), self.tree_of(value)),
                        _ => (String::new(), Tree::Other),
                    })
                    .collect(),
            ),
            _ => Tree::Other,
        }
    }
    fn art_call(&self, e: &Expression) -> Option<usize> {
        match e {
            Expression::FunctionCall(c) => self.art_at.get(&span(c)).copied(),
            _ => None,
        }
    }
    /// A group made by this call, with its options.
    fn group(&mut self, call: &ast::FunctionCall, args: &[&Expression], name: Option<String>) {
        let in_init = self.open.len() == 1 && self.names[self.open[0]] == "init";
        if !(self.functions == 0 || in_init) || self.loops > 0 {
            self.fail(call, GROUP_PLACE);
            return;
        }
        let Some(options) = fields(args.first()).filter(|_| args.len() <= 1) else {
            self.fail(call, "sprites.group takes an optional literal options table {sort = , slots = , keep = }");
            return;
        };
        let mut group = Group {
            site: Some(Site {
                file: self.file.into(),
                line: line(call),
                head: span(call).start..range_end(call),
                call: span(call),
            }),
            sort: false,
            slots: None,
            draws: vec![],
            keep: false,
            scenes: vec![],
            program_wide: false,
            program_draw: None,
        };
        for (k, v) in options {
            match (k.as_str(), v) {
                ("sort", Expression::Symbol(t)) => group.sort = text(t) == "true",
                ("keep", Expression::Symbol(t)) => group.keep = text(t) == "true",
                ("slots", v) => match self.values(v).filter(|s| s.len() == 1).and_then(|s| s.first().copied()) {
                    Some(n) if (0..=128).contains(&n) => group.slots = Some(n as u32),
                    _ => self.fail(call, "sprites.group slots must be a constant 0..128"),
                },
                _ => self.fail(call, "sprites.group takes an optional literal options table {sort = , slots = , keep = }"),
            }
        }
        let index = self.groups.len();
        self.groups.push(group);
        self.group_at.insert(span(call), index);
        if let Some(name) = name {
            self.group_names.insert(name, index);
        }
    }
    fn draw_or_keep(
        &mut self,
        call: &ast::FunctionCall,
        receiver: Option<String>,
        keep: bool,
        args: &[&Expression],
    ) {
        self.draws = true;
        let function = self.open.last().copied().unwrap_or(usize::MAX);
        let mut found = Found {
            line: line(call),
            call: span(call),
            module_group: self.module_group(call),
            receiver,
            keep,
            args: args.iter().map(|a| span(*a)).collect(),
            options: vec![],
            function,
            trips: self.trip_product(),
            arts: None,
            art_parts: None,
        };
        if keep {
            if args.len() != 1 {
                self.fail(call, "keep takes one key");
            }
        } else {
            let options = fields(args.get(3));
            let ok = options.as_ref().is_some_and(|o| {
                o.iter().all(|(k, _)| {
                    matches!(
                        k.as_str(),
                        "depth" | "size" | "prio" | "key" | "layer" | "flip_x" | "flip_y" | "pal" | "at"
                    )
                })
            });
            if !(3..=4).contains(&args.len()) || !ok {
                self.fail(call, DRAW_OPTIONS);
                return;
            }
            let options = options.unwrap();
            if options.iter().any(|(k, _)| k == "layer") {
                self.fail(call, NO_LAYER);
            }
            found.options = options.iter().map(|(k, v)| (k.clone(), span(*v))).collect();
            found.arts = self.art_of(args[0]);
            if found.arts.is_none() {
                found.art_parts = self.art_parts(args[0]);
            }
        }
        self.found.push(found);
    }
}

impl Visitor for Walk<'_> {
    fn visit_block(&mut self, _: &ast::Block) {
        self.frames.push(std::mem::take(&mut self.pending));
    }
    fn visit_block_end(&mut self, _: &ast::Block) {
        self.frames.pop();
    }
    fn visit_function_body(&mut self, node: &ast::FunctionBody) {
        self.functions += 1;
        self.names.push(self.next_name.take().unwrap_or_default());
        self.params.push(node.parameters().iter().map(|p| if let ast::Parameter::Name(n) = p { text(n) } else { String::new() }).collect());
        self.open.push(self.names.len() - 1);
        // A function body's loops are its own: calls multiply them in.
        self.trips.push(Some(1));
        for p in node.parameters() {
            if let ast::Parameter::Name(n) = p {
                self.pending.push((text(n), Bound::Unknown));
            }
        }
    }
    fn visit_function_body_end(&mut self, _: &ast::FunctionBody) {
        self.functions -= 1;
        self.open.pop();
        self.trips.pop();
    }
    fn visit_function_declaration(&mut self, node: &ast::FunctionDeclaration) {
        let n = node.name();
        let mut name = n.names().iter().map(text).collect::<Vec<_>>().join(".");
        if let Some(m) = n.method_name() {
            name = format!("{name}:{}", text(m));
        }
        self.next_name = Some(name);
    }
    fn visit_local_function(&mut self, node: &ast::LocalFunction) {
        if let Some(frame) = self.frames.last_mut() {
            frame.push((text(node.name()), Bound::Unknown));
        }
        self.next_name = Some(text(node.name()));
    }
    fn visit_local_assignment(&mut self, node: &ast::LocalAssignment) {
        // `local f = function ... end` names its body.
        if let ([n], [Expression::Function(_)]) = (
            &node.names().iter().collect::<Vec<_>>()[..],
            &node.expressions().iter().collect::<Vec<_>>()[..],
        ) {
            self.next_name = Some(text(n));
        }
    }
    fn visit_numeric_for(&mut self, node: &ast::NumericFor) {
        self.loops += 1;
        let step = node.step().map_or(Some(Set::from([1])), |s| self.values(s));
        let (start, end) = (self.values(node.start()), self.values(node.end()));
        let range = match (&start, &end, &step) {
            (Some(a), Some(b), Some(s)) if s.len() == 1 && *s.first().unwrap() != 0 => {
                let (lo, hi) = (
                    *a.first().unwrap().min(b.first().unwrap()),
                    *a.last().unwrap().max(b.last().unwrap()),
                );
                (hi - lo <= 1 << 14).then(|| (lo..=hi).collect::<Set>())
            }
            _ => None,
        };
        // Trip count: the most passes any start and end allow.
        let trips = match (start, end, step) {
            (Some(a), Some(b), Some(s)) if s.len() == 1 => {
                let s = *s.first().unwrap();
                if s > 0 {
                    Some(((*b.last().unwrap() - *a.first().unwrap()).max(-1) / s + 1).max(0) as u64)
                } else if s < 0 {
                    Some(
                        ((*a.last().unwrap() - *b.first().unwrap()).max(-1) / -s + 1).max(0) as u64,
                    )
                } else {
                    None
                }
            }
            _ => None,
        };
        self.trips.push(trips);
        let bound = range.map_or(Bound::Unknown, Bound::Numbers);
        self.pending.push((text(node.index_variable()), bound));
    }
    fn visit_numeric_for_end(&mut self, _: &ast::NumericFor) {
        self.loops -= 1;
        self.trips.pop();
    }
    fn visit_generic_for(&mut self, node: &ast::GenericFor) {
        self.loops += 1;
        // Over a file-level table (`t`, `ipairs(t)`, `pairs(t)`): as many passes as its fields.
        let exprs: Vec<_> = node.expressions().iter().collect();
        let table = match exprs[..] {
            [Expression::Var(ast::Var::Name(n))] => Some(text(n)),
            [Expression::FunctionCall(c)] => match path(c) {
                Some((f, args)) if f == "ipairs" || f == "pairs" => match args[..] {
                    [Expression::Var(ast::Var::Name(n))] => Some(text(n)),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        };
        let pool = match exprs[..] {
            [Expression::FunctionCall(c)] => method(c)
                .filter(|(_, m, args)| m == "live" && args.is_empty())
                .and_then(|(p, ..)| self.pools.get(&p).copied()),
            _ => None,
        };
        self.trips
            .push(pool.or_else(|| table.and_then(|t| self.tables.get(&t).copied())));
        for n in node.names() {
            self.pending.push((text(n), Bound::Unknown));
        }
    }
    fn visit_generic_for_end(&mut self, _: &ast::GenericFor) {
        self.loops -= 1;
        self.trips.pop();
    }
    fn visit_while(&mut self, _: &ast::While) {
        self.loops += 1;
        self.trips.push(None);
    }
    fn visit_while_end(&mut self, _: &ast::While) {
        self.loops -= 1;
        self.trips.pop();
    }
    fn visit_repeat(&mut self, _: &ast::Repeat) {
        self.loops += 1;
        self.trips.push(None);
    }
    fn visit_repeat_end(&mut self, _: &ast::Repeat) {
        self.loops -= 1;
        self.trips.pop();
    }
    fn visit_assignment(&mut self, node: &ast::Assignment) {
        // `name = sprites.group(...)` (in init): the group's name.
        let vars: Vec<_> = node.variables().iter().collect();
        let exprs: Vec<_> = node.expressions().iter().collect();
        if let ([ast::Var::Name(n)], [Expression::FunctionCall(c)]) = (&vars[..], &exprs[..]) {
            if path(c).is_some_and(|(p, _)| p == "sprites.group") {
                self.pending_group = Some((span(c), text(n)));
            }
        }
        // `obj.size_sel = n`: the program's own size pair; `obj.char_base` and the sprite layer: its own setup.
        for (i, v) in vars.iter().enumerate() {
            if let ast::Var::Name(n) = v {
                self.sets.1 |= matches!(text(n).as_str(), "TM" | "TS");
            }
            let ast::Var::Expression(v) = v else { continue };
            let dotted: Option<Vec<String>> = v
                .suffixes()
                .map(|s| match s {
                    ast::Suffix::Index(ast::Index::Dot { name, .. }) => Some(text(name)),
                    _ => None,
                })
                .collect();
            if let (ast::Prefix::Name(r), Some(d)) = (v.prefix(), dotted) {
                match (
                    text(r).as_str(),
                    &d.iter().map(String::as_str).collect::<Vec<_>>()[..],
                ) {
                    ("obj", ["char_base"]) => self.sets.0 = true,
                    ("screen", ["main" | "sub"] | ["main" | "sub", "obj"]) => self.sets.1 = true,
                    _ => {}
                }
            }
            let is_size_sel = matches!(v.prefix(), ast::Prefix::Name(r) if text(r) == "obj")
                && matches!(v.suffixes().collect::<Vec<_>>()[..], [ast::Suffix::Index(ast::Index::Dot { name, .. })] if text(name) == "size_sel");
            if is_size_sel {
                let value = exprs
                    .get(i)
                    .and_then(|e| self.values(e))
                    .filter(|s| s.len() == 1)
                    .and_then(|s| s.first().copied());
                self.size_sel.push((
                    line(node),
                    value.filter(|n| (0..8).contains(n)).map(|n| n as u8),
                ));
            }
        }
    }
    fn visit_local_assignment_end(&mut self, node: &ast::LocalAssignment) {
        // A file-level local never assigned again keeps its constant value.
        let top = self.frames.len() == 1;
        let exprs: Vec<_> = node.expressions().iter().collect();
        for (i, token) in node.names().iter().enumerate() {
            let name = text(token);
            // The declaration, when nothing assigns it after this initialiser.
            let constant = self.scope.constant(token);
            if let (Some(d), false, Some(arts)) = (constant, top, exprs.get(i).and_then(|e| self.art_of(e))) {
                self.art_locals.insert(d, arts);
            }
            if let Some(e) = exprs.get(i) {
                if let Some(&g) = match e {
                    Expression::FunctionCall(c) => self.group_at.get(&span(c)),
                    _ => None,
                } {
                    self.group_names.insert(name.clone(), g);
                }
            }
            if top {
                if let Some(Expression::TableConstructor(t)) = exprs.get(i) {
                    self.tables.insert(name.clone(), t.fields().len() as u64);
                }
                // Art, or a table holding some (to any depth), by name: only a name never assigned again holds it.
                match (constant, exprs.get(i).map(|e| self.tree_of(e))) {
                    (Some(d), Some(Tree::Art(a))) => {
                        self.art_names.insert(d, a);
                    }
                    (Some(d), Some(t @ Tree::Table(_))) if t.has_art() => {
                        self.art_tables.insert(d, t);
                    }
                    _ => {}
                }
            }
            if let (Some(d), true) = (constant, top) {
                match exprs.get(i) {
                    Some(Expression::FunctionCall(call)) => {
                        if let Some(stem) = required(call) {
                            self.requires.insert(d, stem);
                        }
                        // `local shots = Pools.new(8, make_shot)`: a pool of 8.
                        if let Some((f, args)) = path(call) {
                            let size = args
                                .first()
                                .and_then(|n| self.values(n))
                                .filter(|s| s.len() == 1)
                                .and_then(|s| s.first().copied());
                            let module = match call.prefix() {
                                ast::Prefix::Name(m) => self.scope.of(m).and_then(|m| self.requires.get(&m)),
                                _ => None,
                            };
                            if let (Some((_, "new")), Some(n)) = (f.split_once('.'), size) {
                                if module.is_some_and(|stem| stem == "@ppu/pools")
                                    && n >= 0
                                {
                                    self.pools.insert(name.clone(), n as u64);
                                }
                            }
                        }
                    }
                    Some(Expression::Var(ast::Var::Expression(v))) => {
                        if let (
                            ast::Prefix::Name(m),
                            [ast::Suffix::Index(ast::Index::Dot { name: field, .. })],
                        ) = (v.prefix(), &v.suffixes().collect::<Vec<_>>()[..])
                        {
                            if let Some(stem) = self.scope.of(m).and_then(|m| self.requires.get(&m)) {
                                self.module_fields.insert(d, (stem.clone(), text(field)));
                            }
                        } else if let Some(picked) = required_field(v) {
                            self.module_fields.insert(d, picked);
                        }
                    }
                    _ => {}
                }
            }
            let bound = match exprs.get(i) {
                Some(e) if top && constant.is_some() => {
                    if let Some(s) = self.values(e) {
                        Bound::Numbers(s)
                    } else if let Some(f) = frozen(e) {
                        Bound::Fields(f)
                    } else if let Expression::FunctionCall(call) = e {
                        required(call)
                            .and_then(|stem| self.modules.get(&stem).cloned())
                            .map_or(Bound::Unknown, Bound::Fields)
                    } else {
                        Bound::Unknown
                    }
                }
                _ => Bound::Unknown,
            };
            if let Some(frame) = self.frames.last_mut() {
                frame.push((name, bound));
            }
        }
    }
    fn visit_function_call(&mut self, call: &ast::FunctionCall) {
        let shadowed = self.frames.iter().flatten().any(|(n, _)| n == "sprites");
        if let Some((receiver, name, args)) = method(call) {
            let group_field = match receiver.split_once('.') {
                Some((_, field)) => self
                    .receiver_module(call)
                    .and_then(|stem| self.group_fields.get(stem))
                    .is_some_and(|g| g.contains(field)),
                None => true,
            };
            if !shadowed
                && group_field
                && (name == "draw" || name == "keep")
                && receiver != "sprites"
            {
                self.draw_or_keep(call, Some(receiver), name == "keep", &args);
            }
            return;
        }
        // A call by name: the call graph that multiplies a function's draws.
        if let ast::Prefix::Name(n) = call.prefix() {
            if let [ast::Suffix::Call(ast::Call::AnonymousCall(_))] =
                call.suffixes().collect::<Vec<_>>()[..]
            {
                let caller = self.open.last().copied().unwrap_or(usize::MAX);
                self.called.push((caller, text(n), self.trip_product()));
            }
        }
        // A call that gives a function a group (or passes on a parameter that may be one).
        if let (Some((callee, args)), ast::Prefix::Name(root)) = (path(call), call.prefix()) {
            let callee = match callee.split_once('.') {
                None => Some((None, callee.clone())),
                Some((_, f)) if !f.contains('.') => self.scope.of(root).and_then(|d| self.requires.get(&d)).map(|stem| (Some(stem.clone()), f.to_string())),
                _ => None,
            };
            if let Some((module, name)) = callee {
                // Every such call, so a parameter a draw is made on knows each thing its callers give it.
                let args: Vec<_> = args.iter().map(|a| (span(*a), self.group_ref(a))).collect();
                self.group_calls.push(GroupCall { line: line(call), module, name, args });
            }
        }
        let Some((name, args)) = path(call) else {
            return;
        };
        if shadowed {
            return;
        }
        match name.as_str() {
            "sprites.draw" => self.draw_or_keep(call, None, false, &args),
            "sprites.keep" => self.draw_or_keep(call, None, true, &args),
            "sprites.sort" => self.fail(call, NO_SORT),
            "sprites.group" => {
                let named = self
                    .pending_group
                    .take()
                    .filter(|(s, _)| *s == span(call))
                    .map(|(_, n)| n);
                self.group(call, &args, named);
            }
            "sprites.rows" => {
                self.rows = true;
                if args.len() != 2 {
                    self.fail(call, "sprites.rows takes a top and a bottom row");
                }
            }
            "sprites.hold" => {
                self.draws = true;
                if !args.is_empty() {
                    self.fail(call, "sprites.hold takes no arguments");
                }
            }
            "sprites.animation" => {
                self.art = true;
                if self.functions > 0 || self.loops > 0 {
                    self.fail(call, "sprites.animation must be called at file level, outside functions and loops, so each call makes one animation (a helper returning it is fine when every call of the helper is at file level with constant arguments)");
                }
                let head = span(call).start..range_end(call);
                self.animations.push(Animation {
                    site: Site { file: self.file.into(), line: line(call), head, call: span(call) },
                    arts: vec![],
                });
                self.art_at.insert(span(call), ANIMATION + self.animations.len());
            }
            "sprites.art" => {
                self.art = true;
                if self.functions > 0 || self.loops > 0 {
                    self.fail(call, "sprites.art must be called at file level, outside functions and loops, so each call makes one art (a helper returning it is fine when every call of the helper is at file level with constant arguments)");
                }
                let head = span(call).start..range_end(call);
                self.arts.push(Site {
                    file: self.file.into(),
                    line: line(call),
                    head,
                    call: span(call),
                });
                self.art_at.insert(span(call), self.arts.len());
            }
            _ => {}
        }
    }
    // An animation's arts, once the calls in its table are numbered.
    fn visit_function_call_end(&mut self, call: &ast::FunctionCall) {
        let Some(&n) = self.art_at.get(&span(call)).filter(|&&n| n > ANIMATION) else {
            return;
        };
        let args = path(call).map(|(_, a)| a).unwrap_or_default();
        // Each art this file names, or one a module returns (resolved once every file is walked).
        let one = |e: &Expression| match self.art_call(e).map(|a| vec![a]).or_else(|| self.art_of(e)) {
            Some(arts) => Some(ArtPart::Known(arts)),
            None => self.module_art(e).map(ArtPart::Module),
        };
        let parts: Option<Vec<ArtPart>> = match args.first() {
            Some(Expression::TableConstructor(t)) => t
                .fields()
                .iter()
                .map(|f| match f {
                    ast::Field::NoKey(value) => one(value),
                    _ => None,
                })
                .collect(),
            Some(Expression::Var(ast::Var::Name(t))) => match self.scope.of(t).and_then(|t| self.art_tables.get(&t)).and_then(Tree::arts) {
                Some(arts) => Some(vec![ArtPart::Known(arts)]),
                None => self.module_art(args[0]).map(|r| vec![ArtPart::Module(r)]),
            },
            Some(e) => self.module_art(e).map(|r| vec![ArtPart::Module(r)]),
            None => None,
        };
        match parts {
            Some(parts) if !parts.is_empty() => self.deferred.push((n - ANIMATION - 1, parts, line(call))),
            _ => self.fail(call, ANIMATION_ARTS),
        }
    }
    fn visit_var_expression(&mut self, v: &ast::VarExpression) {
        let ast::Prefix::Name(root) = v.prefix() else {
            return;
        };
        if text(root) != "obj" || self.frames.iter().flatten().any(|(n, _)| n == "obj") {
            return;
        }
        if let Some(ast::Suffix::Index(ast::Index::Brackets { expression, .. })) =
            v.suffixes().next()
        {
            let s = self.values(expression);
            self.slots.push((line(v), s));
        }
    }
}

/// `require("./m")`: the module's stem, `m`.
fn required(call: &ast::FunctionCall) -> Option<String> {
    let (n, args) = path(call)?;
    if n != "require" {
        return None;
    }
    required_path(&args)
}

/// `require("./m").f`, the field `f` of module `m` picked in one expression: the module's stem and the field.
fn required_field(v: &ast::VarExpression) -> Option<(String, String)> {
    let ast::Prefix::Name(r) = v.prefix() else {
        return None;
    };
    let suffixes: Vec<_> = v.suffixes().collect();
    let [ast::Suffix::Call(ast::Call::AnonymousCall(args)), ast::Suffix::Index(ast::Index::Dot { name: field, .. })] =
        suffixes[..]
    else {
        return None;
    };
    let ast::FunctionArgs::Parentheses { arguments, .. } = args.as_ref() else {
        return None;
    };
    if text(r) != "require" {
        return None;
    }
    Some((required_path(&arguments.iter().collect::<Vec<_>>())?, text(field)))
}

/// A require's one string argument as a module stem (`"./m"` is `m`).
fn required_path(args: &[&Expression]) -> Option<String> {
    let [Expression::String(s)] = args[..] else {
        return None;
    };
    let p = text(s);
    let p = p.trim_matches(|c| c == '"' || c == '\'');
    Some(p.strip_prefix("./").unwrap_or(p).to_string())
}

/// The fields a module returns a group in, read from its syntax alone: a file-level `local g = sprites.group(...)` it
/// returns as `{f = g}`, or a `sprites.group(...)` made in the returned table.
fn returned_group_fields(ast: &ast::Ast) -> HashSet<String> {
    let is_group = |e: &Expression| matches!(e, Expression::FunctionCall(c) if path(c).is_some_and(|(p, _)| p == "sprites.group"));
    let mut made = HashSet::new();
    for statement in ast.nodes().stmts() {
        if let ast::Stmt::LocalAssignment(a) = statement {
            for (n, e) in a.names().iter().zip(a.expressions().iter()) {
                if is_group(e) {
                    made.insert(text(n));
                }
            }
        }
    }
    let Some(ast::LastStmt::Return(r)) = ast.nodes().last_stmt() else {
        return HashSet::new();
    };
    let [Expression::TableConstructor(t)] = r.returns().iter().collect::<Vec<_>>()[..] else {
        return HashSet::new();
    };
    t.fields()
        .iter()
        .filter_map(|f| match f {
            ast::Field::NameKey { key, value, .. } => match &**value {
                Expression::Var(ast::Var::Name(n)) if made.contains(&text(n)) => Some(text(key)),
                e if is_group(e) => Some(text(key)),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// What a module returns that draws need, by field name: its groups (`return {ships = ships}`, or a group made in the
/// table), and its art, a single art (`None` key) or a file-level table of art.
#[derive(Default)]
struct Exports {
    groups: HashMap<String, usize>,
    arts: HashMap<String, Tree>,
    /// The fields that name a local (a function, when the file defines one by that name).
    names: HashMap<String, String>,
}

/// The group a reference names once every module is walked (a parameter's are its callers': not here).
fn known(exports: &HashMap<String, Exports>, r: &GroupRef) -> Option<usize> {
    match r {
        GroupRef::Known(g) => Some(*g),
        GroupRef::Module(stem, field) => exports.get(stem)?.groups.get(field).copied(),
        _ => None,
    }
}

fn module_exports(ast: &ast::Ast, walk: &Walk) -> Exports {
    let mut out = Exports::default();
    let Some(ast::LastStmt::Return(r)) = ast.nodes().last_stmt() else {
        return out;
    };
    let [Expression::TableConstructor(t)] = r.returns().iter().collect::<Vec<_>>()[..] else {
        return out;
    };
    for f in t.fields() {
        let ast::Field::NameKey { key, value, .. } = f else {
            continue;
        };
        let (key, value) = (text(key), &**value);
        if let Expression::Var(ast::Var::Name(n)) = value {
            out.names.insert(key.clone(), text(n));
        }
        let group = match value {
            Expression::Var(ast::Var::Name(n)) => walk.group_names.get(&text(n)).copied(),
            Expression::FunctionCall(c) => walk.group_at.get(&span(c)).copied(),
            _ => None,
        };
        if let Some(g) = group {
            out.groups.insert(key.clone(), g);
        }
        let tree = walk.tree_of(value);
        if tree.has_art() {
            out.arts.insert(key, tree);
        }
    }
    out
}

/// The end of `sprites.art(`: its open parenthesis.
fn range_end(call: &ast::FunctionCall) -> usize {
    for s in call.suffixes() {
        if let ast::Suffix::Call(ast::Call::AnonymousCall(args)) = s {
            if let ast::FunctionArgs::Parentheses { parentheses, .. } = args.as_ref() {
                return parentheses.tokens().0.token().end_position().bytes();
            }
        }
    }
    span(call).end
}

/// The value fields of `return table.freeze({...})` at the end of a module.
fn module_fields(ast: &ast::Ast) -> Option<HashMap<String, i64>> {
    let ast::LastStmt::Return(r) = ast.nodes().last_stmt()? else {
        return None;
    };
    let [e] = r.returns().iter().collect::<Vec<_>>()[..] else {
        return None;
    };
    frozen(e)
}

/// `source` (the file `file`) with each of the plan's `sprites.art(` and `sprites.group(` calls
/// passing its number, as `__ppu_sprites_art(n, ...)` and `__ppu_sprites_group(n, ...)`: the
/// library then knows each art's and group's place in the plan. Lines are unchanged.
pub fn number_sites(plan: &Plan, file: &str, source: &str) -> String {
    let mut edits: Vec<(Range<usize>, String)> = vec![];
    let sites = plan
        .arts
        .iter()
        .enumerate()
        .map(|(i, s)| (s, format!("__ppu_sprites_art({}", i + 1)))
        .chain(
            plan.animations
                .iter()
                .enumerate()
                .map(|(i, a)| (&a.site, format!("__ppu_sprites_animation({}", i + 1))),
        );
    let groups = plan.groups.iter().enumerate().filter_map(|(i, g)| {
        g.site
            .as_ref()
            .map(|s| (s, format!("__ppu_sprites_group({}", i + 1)))
    });
    for (site, head) in sites.chain(groups) {
        if site.file != file {
            continue;
        }
        // An empty argument list takes the number alone.
        let empty = source[site.head.end..site.call.end]
            .trim_start()
            .starts_with(')');
        edits.push((
            site.head.clone(),
            if empty { head } else { format!("{head}, ") },
        ));
    }
    edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    let mut out = source.to_string();
    for (r, text) in edits {
        out.replace_range(r, &text);
    }
    out
}

/// Each file a scene owns, by the scene's number (from 1, in `scenes.add` order): a module two scenes require is the
/// first's. Every other file is the program's.
pub fn scene_owners(files: &[(&str, &str)]) -> HashMap<String, usize> {
    scene_reach(files)
        .into_iter()
        .map(|(f, scenes)| (f, scenes[0]))
        .collect()
}

/// The scenes that reach each file, in order: its module and the modules it requires, transitively, unless the
/// program's own files (those no scene reaches) require them too. Every other file is the program's.
pub fn scene_reach(files: &[(&str, &str)]) -> HashMap<String, Vec<usize>> {
    let scenes = crate::frontend::scene_files(files);
    let source = |f: &str| files.iter().find(|(n, _)| *n == f).map(|(_, s)| *s);
    let resolve = |path: &str| {
        [format!("{path}.luau"), format!("{path}.lua")]
            .into_iter()
            .find(|f| source(f).is_some())
    };
    // Every file reached from `roots`, never entering a scene module other than a root.
    let reach = |roots: Vec<String>| {
        let mut seen: Vec<String> = vec![];
        let mut todo = roots.clone();
        while let Some(f) = todo.pop() {
            if seen.contains(&f) || (scenes.contains(&f) && !roots.contains(&f)) {
                continue;
            }
            todo.extend(
                crate::frontend::requires(source(&f).unwrap_or(""))
                    .into_iter()
                    .filter_map(|p| resolve(p)),
            );
            seen.push(f);
        }
        seen
    };
    let mut owners: HashMap<String, Vec<usize>> = HashMap::new();
    for (k, scene) in scenes.iter().enumerate() {
        for f in reach(vec![scene.clone()]) {
            owners.entry(f).or_default().push(k + 1);
        }
    }
    let program: Vec<String> = files
        .iter()
        .map(|(f, _)| f.to_string())
        .filter(|f| !owners.contains_key(f))
        .collect();
    for f in reach(program) {
        if !scenes.contains(&f) {
            owners.remove(&f);
        }
    }
    owners
}

/// The sprite plan of `files` (prepared sources, as the preview runs them): `None` for a program
/// that never draws with the library. Files that do not parse as Luau are skipped; they cannot
/// reach the library through this syntax.
pub fn plan(files: &[(&str, &str)]) -> Result<Option<Plan>, Error> {
    if !files
        .iter()
        .any(|(_, s)| s.contains("sprites.") || s.contains(":draw(") || s.contains(":keep("))
    {
        return Ok(None);
    }
    // Every file: one that never names sprites can still hand an animation to a draw (a main wiring modules together).
    let parsed: Vec<(&str, ast::Ast)> = files
        .iter()
        .filter_map(|(f, s)| {
            full_moon::parse_fallible(s, full_moon::LuaVersion::luau())
                .into_result()
                .ok()
                .map(|a| (*f, a))
        })
        .collect();
    let mut modules = HashMap::new();
    for (f, s) in files {
        if let Ok(ast) = full_moon::parse_fallible(s, full_moon::LuaVersion::luau()).into_result() {
            if let Some(fields) = module_fields(&ast) {
                let stem = f.trim_end_matches(".luau").trim_end_matches(".lua");
                modules.insert(stem.to_string(), fields);
            }
        }
    }
    let group_fields: HashMap<String, HashSet<String>> = parsed
        .iter()
        .map(|(f, ast)| {
            (
                f.trim_end_matches(".luau")
                    .trim_end_matches(".lua")
                    .to_string(),
                returned_group_fields(ast),
            )
        })
        .collect();
    let mut draws = false;
    let mut slots = vec![];
    let mut first_error = None;
    let mut arts = vec![];
    let mut animations = vec![];
    let mut groups = vec![];
    // The default group's draws, and every call with its group (`usize::MAX`: the default).
    let mut default = vec![];
    let mut calls = vec![];
    let mut size_sels = vec![];
    let mut rows = false;
    let mut sets = vec![];
    // Each module's returned groups and art, and every draw or keep: its group (or the module and field of one another
    // file made), the times it runs, its call, and the module art it names. They resolve once every file is walked.
    let mut exports: HashMap<String, Exports> = HashMap::new();
    let mut facts = vec![];
    let mut deferred = vec![];
    let mut group_calls: Vec<(String, String, GroupCall)> = vec![];
    let mut found: Vec<(
        GroupRef,
        Option<Option<u64>>,
        Call,
        Option<Vec<ArtPart>>,
        Option<String>,
    )> = vec![];
    for (file, ast) in &parsed {
        let mut walk = Walk {
            file,
            modules: &modules,
            group_fields: &group_fields,
            scope: scope::resolve(ast),
            frames: vec![],
            pending: vec![],
            functions: 0,
            loops: 0,
            draws: false,
            art: false,
            slots: vec![],
            error: None,
            arts: &mut arts,
            animations: &mut animations,
            art_at: HashMap::new(),
            art_names: HashMap::new(),
            art_tables: HashMap::new(),
            deferred: vec![],
            art_locals: HashMap::new(),
            tables: HashMap::new(),
            pools: HashMap::new(),
            groups: &mut groups,
            group_names: HashMap::new(),
            group_at: HashMap::new(),
            names: vec![],
            params: vec![],
            group_calls: vec![],
            open: vec![],
            next_name: None,
            trips: vec![],
            called: vec![],
            found: vec![],
            pending_group: None,
            requires: HashMap::new(),
            module_fields: HashMap::new(),
            size_sel: vec![],
            rows: false,
            sets: (false, false),
        };
        walk.visit_ast(ast);
        let stem = file.trim_end_matches(".luau").trim_end_matches(".lua").to_string();
        group_calls.extend(std::mem::take(&mut walk.group_calls).into_iter().map(|c| (file.to_string(), stem.clone(), c)));
        deferred.extend(walk.deferred.drain(..).map(|(n, parts, line)| (n, parts, file.to_string(), line)));
        exports.insert(
            file.trim_end_matches(".luau")
                .trim_end_matches(".lua")
                .to_string(),
            module_exports(ast, &walk),
        );
        facts.push(flow::Facts {
            names: std::mem::take(&mut walk.scope),
            art_names: walk.art_names.clone(),
            art_tables: walk.art_tables.clone(),
            requires: walk.requires.clone(),
            module_fields: walk.module_fields.clone(),
        });
        // How many times a function runs a frame: once when the file never calls it by name (an entry,
        // or a module's export), else the sum over its calls; recursion is unknown.
        fn times(
            f: usize,
            names: &[String],
            called: &[(usize, String, Option<u64>)],
            depth: usize,
        ) -> Option<u64> {
            if f == usize::MAX || names[f].is_empty() {
                return Some(1);
            }
            let calls: Vec<_> = called.iter().filter(|(_, n, _)| *n == names[f]).collect();
            if calls.is_empty() {
                return Some(1);
            }
            if depth > 16 {
                return None;
            }
            calls.iter().try_fold(0u64, |sum, (caller, _, trips)| {
                Some(sum.saturating_add(trips.as_ref()?.saturating_mul(times(
                    *caller,
                    names,
                    called,
                    depth + 1,
                )?)))
            })
        }
        for f in std::mem::take(&mut walk.found) {
            let need = (!f.keep).then(|| {
                f.trips.and_then(|t| {
                    Some(t.saturating_mul(times(f.function, &walk.names, &walk.called, 0)?))
                })
            });
            let call = Call {
                file: file.to_string(),
                line: f.line,
                call: f.call,
                group: usize::MAX,
                keep: f.keep,
                args: f.args,
                options: f.options,
                arts: f.arts,
                animations: None,
                through: None,
            };
            // A group this file made, one a module it requires returns, or a parameter of the file-level function the
            // draw is in, which its callers give groups (both resolved once every module is walked).
            let group = match &f.receiver {
                None => GroupRef::Known(usize::MAX),
                Some(r) => match (walk.group_names.get(r), &f.module_group) {
                    (Some(g), _) => GroupRef::Known(*g),
                    (None, Some((stem, field))) => GroupRef::Module(stem.clone(), field.clone()),
                    (None, None) => match walk.params.get(f.function).and_then(|ps| ps.iter().position(|p| p == r)) {
                        Some(p) if !walk.names[f.function].is_empty() => GroupRef::Param(walk.names[f.function].clone(), p),
                        _ => GroupRef::Other,
                    },
                },
            };
            found.push((group, need, call, f.art_parts, f.receiver.clone()));
        }
        if let Some(e) = walk.error {
            first_error.get_or_insert(e);
        }
        draws |= walk.draws;
        rows |= walk.rows;
        size_sels.extend(
            walk.size_sel
                .into_iter()
                .map(|(l, v)| (file.to_string(), l, v)),
        );
        sets.push((file.to_string(), walk.sets));
        slots.extend(
            walk.slots
                .into_iter()
                .map(|(l, s)| (file.to_string(), l, s)),
        );
    }
    let module_art = |(stem, field, path): &ArtRef| -> Option<Vec<usize>> {
        let nodes = exports.get(stem)?.arts.get(field)?.at(path)?;
        // One table of art (an animation's whole list) reads as its arts.
        match nodes[..] {
            [t @ Tree::Table(_)] => t.arts(),
            _ => arts_of(&nodes),
        }
    };
    // Each animation's arts, now that every module's are known.
    for (n, parts, file, line) in deferred {
        let arts = parts.iter().try_fold(vec![], |mut all, part| {
            all.extend(match part {
                ArtPart::Known(arts) => arts.clone(),
                ArtPart::Module(r) => module_art(r)?,
            });
            Some(all)
        });
        match arts.filter(|a| !a.is_empty() && a.iter().all(|&a| a < ANIMATION)) {
            Some(arts) => animations[n].arts = arts,
            None => {
                first_error.get_or_insert(Error { file, line, message: ANIMATION_ARTS.into() });
            }
        }
    }
    // Where the animations go: a draw the plan can't name shows plain art unless one can reach it.
    let (reached, escapes) = if animations.is_empty() {
        Default::default()
    } else {
        flow::follow(&flow::Program {
            files: parsed.iter().zip(facts).map(|((f, ast), facts)| (*f, ast, facts)).collect(),
            exports: exports.iter().map(|(stem, e)| (stem.clone(), e.arts.clone())).collect(),
        })
    };
    // What each parameter of a file-level function is given by its calls (by stem, function and place): the groups,
    // and the first call that gives it something else. Parameters pass on what they are given, to a fixed point.
    let callee_of = |stem: &str, c: &GroupCall| -> Option<(String, String)> {
        match &c.module {
            None => Some((stem.to_string(), c.name.clone())),
            Some(m) => Some((m.clone(), exports.get(m)?.names.get(&c.name)?.clone())),
        }
    };
    let mut given: HashMap<(String, String, usize), (BTreeSet<usize>, Option<(String, u32)>)> = HashMap::new();
    // A function no call names (an entry, frame()) gives its parameters what the walk cannot know.
    let called: HashSet<(String, String)> = group_calls.iter().filter_map(|(_, stem, c)| callee_of(stem, c)).collect();
    loop {
        let mut changed = false;
        for (file, stem, c) in &group_calls {
            let Some((cs, cf)) = callee_of(stem, c) else { continue };
            for (i, (_, r)) in c.args.iter().enumerate() {
                let (gs, other) = match r {
                    GroupRef::Param(f, _) if !called.contains(&(stem.clone(), f.clone())) => (BTreeSet::new(), Some((file.clone(), c.line))),
                    GroupRef::Param(f, p) => given.get(&(stem.clone(), f.clone(), *p)).cloned().unwrap_or_default(),
                    r => match known(&exports, r) {
                        Some(g) => (BTreeSet::from([g]), None),
                        None => (BTreeSet::new(), Some((file.clone(), c.line))),
                    },
                };
                let entry = given.entry((cs.clone(), cf.clone(), i)).or_default();
                let before = entry.clone();
                entry.0.extend(gs);
                if entry.1.is_none() {
                    entry.1 = other;
                }
                changed |= *entry != before;
            }
        }
        if !changed {
            break;
        }
    }
    // The parameters draws are made on, and those that pass a group on to one: their calls' group arguments.
    let mut drawn: HashSet<(String, String, usize)> = found.iter().filter_map(|(g, _, c, _, _)| match g {
        GroupRef::Param(f, p) => Some((c.file.trim_end_matches(".luau").trim_end_matches(".lua").to_string(), f.clone(), *p)),
        _ => None,
    }).collect();
    loop {
        let before = drawn.len();
        for (_, stem, c) in &group_calls {
            let Some((cs, cf)) = callee_of(stem, c) else { continue };
            for (i, (_, r)) in c.args.iter().enumerate() {
                if let GroupRef::Param(f, p) = r {
                    if drawn.contains(&(cs.clone(), cf.clone(), i)) {
                        drawn.insert((stem.clone(), f.clone(), *p));
                    }
                }
            }
        }
        if drawn.len() == before {
            break;
        }
    }
    let mut group_args = vec![];
    for (file, stem, c) in &group_calls {
        let Some((cs, cf)) = callee_of(stem, c) else { continue };
        for (i, (arg, r)) in c.args.iter().enumerate() {
            if let Some(group) = known(&exports, r).filter(|_| drawn.contains(&(cs.clone(), cf.clone(), i))) {
                group_args.push(GroupArg { file: file.clone(), arg: arg.clone(), group });
            }
        }
    }
    for (group, times, mut call, art_parts, receiver) in found {
        if let Some(parts) = art_parts {
            call.arts = parts.iter().try_fold(vec![], |mut all, part| {
                all.extend(match part {
                    ArtPart::Known(arts) => arts.clone(),
                    ArtPart::Module(r) => module_art(r)?,
                });
                Some(all)
            });
        }
        // An animation counts as its arts; the draw keeps which animations it can show.
        if let Some(arts) = call.arts.take() {
            let (shown, played): (Vec<usize>, Vec<usize>) = arts.into_iter().partition(|&a| a < ANIMATION);
            let played: Vec<usize> = played.into_iter().map(|a| a - ANIMATION).collect();
            let mut all = shown;
            all.extend(played.iter().flat_map(|&n| animations[n - 1].arts.clone()));
            call.arts = Some(all);
            call.animations = Some(played);
        } else if !call.keep && !reached.contains(&(call.file.clone(), call.call.clone())) {
            call.animations = Some(vec![]);
        }
        let stem = call.file.trim_end_matches(".luau").trim_end_matches(".lua").to_string();
        let (through, into) = match &group {
            GroupRef::Param(f, p) => match given.get(&(stem, f.clone(), *p)) {
                Some((_, Some((file, line)))) => {
                    let r = receiver.unwrap_or_default();
                    let message = format!("{r} is a parameter of `{f}`, and the call at {file}:{line} gives it something other than a sprites group the program makes: pass the group itself (a `sprites.group` local, or one a module returns) at every call, so each draw knows its group");
                    first_error.get_or_insert(Error { file: call.file, line: call.line, message });
                    continue;
                }
                g => (receiver, g.map(|g| g.0.clone())),
            },
            g => (None, known(&exports, g).map(|g| BTreeSet::from([g]))),
        };
        let Some(into) = into.filter(|g| !g.is_empty()) else {
            first_error.get_or_insert(Error { file: call.file, line: call.line, message: NO_GROUP.into() });
            continue;
        };
        // A draw on a parameter draws into each group its callers give it.
        for group in into {
            if let Some(times) = times {
                let need = (call.file.clone(), call.line, times, call.arts.clone());
                if group == usize::MAX {
                    default.push(need)
                } else {
                    groups[group].draws.push(need)
                }
            }
            calls.push(Call { group, through: through.clone(), ..call.clone() });
        }
    }
    if let Some(e) = first_error {
        return Err(e);
    }
    if !draws {
        return Ok(None);
    }
    if rows && groups.iter().any(|g| g.sort) {
        return Err(Error {
            file: String::new(),
            line: 0,
            message: NO_SORTED_ROWS.into(),
        });
    }
    let reach = scene_reach(files);
    let scene_list = crate::frontend::scene_files(files);
    let owner = |file: &str| reach.get(file).map_or(0, |s| s[0]);
    // What "every scene" means for the program's groups and draws: all scenes but the hosted ident's.
    let ident = scene_list
        .iter()
        .position(|f| f == HOSTED_IDENT)
        .map(|k| k + 1);
    let everywhere: Vec<usize> = match ident {
        Some(i) => (1..=scene_list.len()).filter(|&k| k != i).collect(),
        None => vec![],
    };
    // Per owner (0: the program, k: scene k) the slots its obj writes take.
    let mut reserved = vec![BTreeSet::new(); scene_list.len() + 1];
    for (i, g) in groups.iter_mut().enumerate() {
        // A group shared by two scenes' modules is in both. Its slots count in each.
        let made = g
            .site
            .as_ref()
            .and_then(|s| reach.get(&s.file))
            .cloned()
            .unwrap_or_default();
        // Like the default group, it is live only in the scenes that draw or keep into it, so a program-wide group
        // leaves a scene that never uses it (the hosted ident) the whole OAM. A call in a program file uses it in every scene but the hosted ident's.
        let used: Option<Vec<Vec<usize>>> = calls
            .iter()
            .filter(|c| c.group == i)
            .map(|c| reach.get(&c.file).cloned())
            .collect();
        g.program_wide = false;
        g.program_draw = calls
            .iter()
            .find(|c| c.group == i && !c.keep && !reach.contains_key(&c.file))
            .map(|c| (c.file.clone(), c.line));
        g.scenes = match used.map(|u| u.concat()) {
            Some(mut used) if !used.is_empty() => {
                used.retain(|k| made.is_empty() || made.contains(k));
                used.sort_unstable();
                used.dedup();
                used
            }
            _ if made.is_empty() => {
                g.program_wide = true;
                everywhere.clone()
            }
            _ => made,
        };
    }
    let mut scene_writes = vec![None; scene_list.len()];
    for (file, line, set) in slots {
        if let Some(k) = owner(&file).checked_sub(1) {
            scene_writes[k].get_or_insert((file.clone(), line));
        }
        // A scene that makes no group of its own may write any slot: nothing of its own competes for them.
        let set = match set {
            Some(set) => set,
            None if owner(&file) != 0
                && !groups
                    .iter()
                    .any(|g| !g.scenes.is_empty() && g.in_scene(owner(&file))) =>
            {
                (0..128).collect()
            }
            None => {
                return Err(Error {
                    file,
                    line,
                    message: UNPROVABLE_SLOT.into(),
                })
            }
        };
        reserved[owner(&file)].extend(
            set.into_iter()
                .filter(|s| (0..128).contains(s))
                .map(|s| s as u8),
        );
    }
    // The default group draws behind the others, in the scene all its draws are in (else the program's).
    let index = groups.len();
    let mut draw_owners = default.iter().map(|(f, ..)| owner(f));
    let scene = draw_owners
        .next()
        .filter(|&k| draw_owners.all(|o| o == k))
        .unwrap_or(0);
    groups.push(Group {
        site: None,
        sort: false,
        slots: None,
        draws: default,
        keep: false,
        scenes: if scene == 0 { everywhere } else { vec![scene] },
        program_wide: scene == 0,
        program_draw: None,
    });
    for c in &mut calls {
        if c.group == usize::MAX {
            c.group = index;
        }
    }
    // Every group without `slots` must bound its draws.
    for g in groups.iter().filter(|g| g.site.is_some()) {
        if g.slots.is_none() {
            if let Some((file, line, _, _)) = g
                .draws
                .iter()
                .find(|(_, _, t, a)| t.is_none() || a.is_none())
            {
                return Err(Error {
                    file: file.clone(),
                    line: *line,
                    message: UNSIZED.into(),
                });
            }
        }
    }
    // A scene that draws nothing with the library sets OBJSEL for itself alone: its writes pin no pair.
    let drawing: BTreeSet<usize> = calls.iter().map(|c| owner(&c.file)).collect();
    let pins = |f: &str| {
        (drawing.contains(&0) && Some(owner(f)) != ident)
            || drawing.contains(&owner(f))
            || owner(f) == 0
    };
    let sets_char_base = sets.iter().any(|(f, s)| s.0 && pins(f));
    let sets_obj_layer = sets.iter().any(|(f, s)| s.1 && pins(f));
    let mut size_sel = None;
    for (file, line, v) in size_sels.into_iter().filter(|(f, ..)| pins(f)) {
        match (v, size_sel) {
            (Some(v), None) => size_sel = Some(v),
            (Some(v), Some(w)) if v == w => {}
            _ => {
                return Err(Error {
                    file,
                    line,
                    message: SIZE_SEL.into(),
                })
            }
        }
    }
    let scenes = scene_list
        .into_iter()
        .zip(reserved.drain(1..))
        .map(|(f, r)| (f, r.into_iter().collect()))
        .collect();
    Ok(Some(Plan {
        reserved: reserved.remove(0).into_iter().collect(),
        scenes,
        scene_writes,
        arts,
        animations,
        groups,
        calls,
        group_args,
        size_sel,
        rows,
        sets_char_base,
        sets_obj_layer,
        escapes,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reserved(files: &[(&str, &str)]) -> Result<Option<Vec<u8>>, Error> {
        plan(files).map(|p| p.map(|p| p.reserved))
    }

    #[test]
    fn raw_only_programs_have_no_plan() {
        assert_eq!(
            reserved(&[("main.lua", "function frame() obj[0].x = 3 end")]),
            Ok(None)
        );
    }

    #[test]
    fn constant_indices_loops_offsets_and_frozen_modules_reserve_slots() {
        let c = "return table.freeze({first = 10, count = 3})";
        let main = r#"local C = require("./c")
local BASE = 20
function frame()
  sprites.draw(A, 0, 0)
  obj[0].x = 1
  for i = 1, 2 do obj[BASE + i - 1].on = true end
  for i = C.first, C.first + C.count - 1 do local s = obj[i]; s.on = true end
end"#;
        assert_eq!(
            reserved(&[("c.luau", c), ("main.luau", main)]),
            Ok(Some(vec![0, 10, 11, 12, 20, 21]))
        );
    }

    #[test]
    fn a_group_in_a_module_two_scenes_share_is_in_both() {
        let main = "ONE = scenes.add(require(\"./one\"))\nTWO = scenes.add(require(\"./two\"))\nTHREE = scenes.add(require(\"./three\"))\n";
        let boss = "local sheet = dma(\"s\", {char = 0x4000})\nlocal art = sprites.art({{size = 16, columns = 1, cells = {0}}}, sheet)\nlocal g = sprites.group({slots = 4})\nlocal function draw() g:draw(art, 1, 2) end\nreturn {draw = draw}\n";
        let one = "local Boss = require(\"./boss\")\nlocal function frame() Boss.draw() end\nreturn {frame = frame}\n";
        let two = one;
        let three = "local function frame() for i = 0, 127 do obj[i].on = true end end\nreturn {frame = frame}\n";
        let p = plan(&[
            ("main.luau", main),
            ("boss.luau", boss),
            ("one.luau", one),
            ("two.luau", two),
            ("three.luau", three),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(p.groups[0].scenes, vec![1, 2]);
        // The third scene's raw slots do not collide: the group is not in it.
        assert_eq!(p.layout(&[1]).unwrap()[0], (0, 4));
    }

    #[test]
    fn sprites_rows_and_a_sorted_group_are_refused_together() {
        let main = "local sheet = dma(\"s\", {char = 0x4000})\nlocal art = sprites.art({{size = 16, columns = 1, cells = {0}}}, sheet)\nlocal g = sprites.group({sort = true})\nfunction frame() sprites.rows(0, 100) g:draw(art, 1, 2) end\n";
        assert_eq!(
            plan(&[("main.luau", main)]).unwrap_err().message,
            NO_SORTED_ROWS
        );
    }

    #[test]
    fn unprovable_indices_and_misplaced_art_are_errors() {
        let main = "function frame(t, f)\n  sprites.draw(A, 0, 0)\n  obj[f % 200].x = 1\nend";
        let e = plan(&[("main.lua", main)]).unwrap_err();
        assert_eq!((e.line, e.message.as_str()), (3, UNPROVABLE_SLOT));
        // Reassigned file-level locals are not constants.
        let main =
            "local K = 1\nK = 2\nfunction frame()\n  sprites.draw(A, 0, 0)\n  obj[K].x = 1\nend";
        assert_eq!(plan(&[("main.lua", main)]).unwrap_err().line, 5);
        let main = "function init() A = sprites.art({}, nil) end\nfunction frame() sprites.draw(A, 0, 0) end";
        assert!(plan(&[("main.lua", main)])
            .unwrap_err()
            .message
            .contains("file level"));
    }

    #[test]
    fn a_constant_size_sel_pins_the_pair() {
        let size_sel = |main: &str| plan(&[("main.lua", main)]).map(|p| p.unwrap().size_sel);
        assert_eq!(
            size_sel("function frame() sprites.draw(A, 0, 0) end"),
            Ok(None)
        );
        let main = "local P = 3\nfunction init() obj.char_base, obj.size_sel = 0, P end\nfunction frame() obj.size_sel = 3 sprites.draw(A, 0, 0) end";
        assert_eq!(size_sel(main), Ok(Some(3)));
        for main in [
            "function init() obj.size_sel = 1 end\nfunction frame() obj.size_sel = 2 sprites.draw(A, 0, 0) end",
            "function frame(t, f) obj.size_sel = f % 2 sprites.draw(A, 0, 0) end",
            "function frame() obj.size_sel = 9 sprites.draw(A, 0, 0) end",
        ] {
            assert_eq!(plan(&[("main.lua", main)]).unwrap_err().message, SIZE_SEL, "{main}");
        }
    }

    #[test]
    fn each_scene_lays_out_the_whole_oam() {
        let main =
            "SWARM = scenes.add(require(\"./swarm\"))\nFLEET = scenes.add(require(\"./fleet\"))\n";
        // The swarm's obj slots are its own; the fleet's groups are in a module only it requires.
        let swarm = "local function frame() for i = 0, 127 do obj[i].on = true end end\nreturn {frame = frame}";
        let fleet = "local Ships = require(\"./ships\")\nlocal function frame() Ships.draw() end\nreturn {frame = frame}";
        let ships = "local A = sprites.art({{size = 16, cells = {0}}})\nlocal g = sprites.group()\nlocal h = sprites.group()\nlocal function draw()\n  for i = 1, 100 do g:draw(A, 0, 0) end\n  for i = 1, 28 do h:draw(A, 0, 0) end\nend\nreturn {draw = draw}";
        let files = |main: &str| {
            vec![
                ("main.luau", main.to_string()),
                ("swarm.luau", swarm.into()),
                ("fleet.luau", fleet.into()),
                ("ships.luau", ships.into()),
            ]
        };
        let plan_of = |files: &[(&str, String)]| {
            let refs: Vec<(&str, &str)> = files.iter().map(|(n, s)| (*n, s.as_str())).collect();
            plan(&refs).unwrap().unwrap()
        };
        let layout = |files: &[(&str, String)]| plan_of(files).layout(&[1]);
        let p = plan_of(&files(main));
        assert_eq!(p.reserved, Vec::<u8>::new());
        assert_eq!(p.scenes[0].1.len(), 128);
        assert_eq!(
            p.groups
                .iter()
                .map(|g| g.scenes.clone())
                .collect::<Vec<_>>(),
            [vec![2], vec![2], vec![]]
        );
        assert_eq!(
            layout(&files(main)),
            Ok(vec![(0, 100), (100, 28), (128, 0)])
        );
        // The program's own obj slot counts against every scene.
        let shared = format!("{main}function init() obj[127].on = true end\n");
        assert_eq!(
            layout(&files(&shared)).unwrap_err().message,
            "the sprites group at ships.luau:3 needs 28 slots in a row and fleet.luau has 27 left: the sprites group at ships.luau:2 takes 100 slots; the group is drawn in ships.luau:6: make fewer obj[] writes or smaller groups in that scene"
        );
    }

    #[test]
    fn a_scene_drawing_with_sprites_draw_owns_the_default_group() {
        // The hosted ident's shape (an ordinary scene named ident.luau here): one scene sets OBJSEL and writes every obj slot; the game draws plain sprites.draw.
        let main =
            "IDENT = scenes.add(require(\"./ident\"))\nGAME = scenes.add(require(\"./game\"))\n";
        let ident = "local function init() obj.char_base, obj.size_sel = 0x2000, 0 screen.main.obj = true end\nlocal function frame() for i = 0, 127 do obj[i].on = true end end\nreturn {init = init, frame = frame}";
        let game = "local A = sprites.art({{size = 32, cells = {0}}})\nlocal function frame() sprites.draw(A, 0, 0) end\nreturn {frame = frame}";
        let p = plan(&[
            ("main.luau", main),
            ("ident.luau", ident),
            ("game.luau", game),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(
            p.groups
                .iter()
                .map(|g| g.scenes.clone())
                .collect::<Vec<_>>(),
            [vec![2]]
        );
        assert_eq!(p.layout(&[4]), Ok(vec![(0, 128)]));
        assert_eq!(
            p.size_sel, None,
            "the ident's own OBJSEL pins nothing for the game's art"
        );
        assert!(
            !p.sets_char_base && !p.sets_obj_layer,
            "nor its sprite layer setup"
        );
    }

    #[test]
    fn a_program_group_is_live_only_where_it_is_drawn() {
        // A scene that writes every obj slot (as the hosted ident does) in front of a game of scenes: main requires the art module that makes the game's groups,
        // only the game's scene draws into them, and the ident writes every obj slot.
        let main = "local _Art = require(\"./art\")\nGAME = scenes.add(require(\"./game\"))\nIDENT = scenes.add(require(\"./ident\"))\n";
        let art = "local A = sprites.art({{size = 16, cells = {0}}})\nlocal g = sprites.group({slots = 8})\nreturn {A = A, g = g}";
        let game = "local Art = require(\"./art\")\nlocal function frame() Art.g:draw(Art.A, 0, 0) end\nreturn {frame = frame}";
        let ident = "local function frame() for i = 0, 127 do obj[i].on = true end end\nreturn {frame = frame}";
        let p = plan(&[
            ("main.luau", main),
            ("art.luau", art),
            ("game.luau", game),
            ("ident.luau", ident),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(p.groups[0].scenes, vec![1]);
        assert_eq!(p.layout(&[1]).unwrap()[0], (0, 8));
        // Drawn from a program file, it stays live in every scene (here no scene is the hosted ident), and ident.luau has no room.
        let main = format!("{main}function init() _Art.g:draw(_Art.A, 0, 0) end\n");
        let p = plan(&[
            ("main.luau", &main),
            ("art.luau", art),
            ("game.luau", game),
            ("ident.luau", ident),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(p.groups[0].scenes, Vec::<usize>::new());
        assert!(p.layout(&[1]).is_err());
    }

    #[test]
    fn a_group_made_in_a_module_only_the_scenes_require_is_live_in_those_scenes() {
        // The help's pattern: art in a module main requires, the group in a module of groups the scenes require.
        let main = "local _Art = require(\"./art\")\nGAME = scenes.add(require(\"./game\"))\nOTHER = scenes.add(require(\"./other\"))\n";
        let art = "return {A = sprites.art({{size = 16, cells = {0}}})}";
        let groups = "return {cars = sprites.group({slots = 1})}";
        let game = "local Art = require(\"./art\")\nlocal Groups = require(\"./groups\")\nlocal cars = Groups.cars\nlocal function frame() cars:draw(Art.A, 0, 0) end\nreturn {frame = frame}";
        let other = "local function frame() for i = 0, 127 do obj[i].on = true end end\nreturn {frame = frame}";
        let p = plan(&[("main.luau", main), ("art.luau", art), ("groups.luau", groups), ("game.luau", game), ("other.luau", other)])
            .unwrap()
            .unwrap();
        assert_eq!(p.groups[0].scenes, vec![1]);
        assert_eq!(p.layout(&[1]).unwrap()[0], (0, 1));
        // The same pick in one line.
        let game = "local Art = require(\"./art\")\nlocal cars = require(\"./groups\").cars\nlocal function frame() cars:draw(Art.A, 0, 0) end\nreturn {frame = frame}";
        let p = plan(&[("main.luau", main), ("art.luau", art), ("groups.luau", groups), ("game.luau", game), ("other.luau", other)])
            .unwrap()
            .unwrap();
        assert_eq!(p.groups[0].scenes, vec![1]);
        assert_eq!(p.layout(&[1]).unwrap()[0], (0, 1));
    }

    fn refusal_of(files: &[(&str, &str)], cells: &[u64]) -> String {
        plan(files).unwrap().unwrap().layout(cells).unwrap_err().message
    }

    #[test]
    fn the_refusal_of_a_program_drawn_group_names_the_scene_what_filled_it_and_why() {
        // PPU-1055's shape: a group made and drawn in a module main requires, a scene writing every obj slot.
        let main = "local Shared = require(\"./shared\")\nGAME = scenes.add(require(\"./game\"))\nfunction frame() Shared.draw() end\n";
        let shared = "local A = sprites.art({{size = 16, cells = {0}}})\nlocal cars = sprites.group({slots = 1})\nlocal function draw() cars:draw(A, 0, 0) end\nreturn {draw = draw}";
        let game = "local function frame() for i = 0, 127 do obj[i].on = true end end\nreturn {frame = frame}";
        assert_eq!(
            refusal_of(&[("main.luau", main), ("shared.luau", shared), ("game.luau", game)], &[1]),
            "the sprites group at shared.luau:2 needs 1 slot in a row and game.luau has 0 left: its obj[] writes take 128 slots, the first at game.luau:1; the group is drawn in shared.luau:3, which main.luau requires, so it counts in every scene: make the group in a module of groups that the scenes require (not main.luau) and draw it from the scenes' own files, e.g. `local Groups = require(\"./groups\")` and `Groups.name:draw(...)` in game.luau; the art can stay in the module main.luau requires so it still loads in every scene"
        );
    }

    #[test]
    fn the_refusal_of_groups_drawn_in_one_scene_names_the_groups_before_it() {
        let main = "GAME = scenes.add(require(\"./game\"))\n";
        let game = "local A = sprites.art({{size = 16, cells = {0}}})\nlocal a = sprites.group({slots = 100})\nlocal b = sprites.group({slots = 30})\nlocal function frame() a:draw(A, 0, 0) b:draw(A, 0, 0) end\nreturn {frame = frame}";
        assert_eq!(
            refusal_of(&[("main.luau", main), ("game.luau", game)], &[1]),
            "the sprites group at game.luau:3 needs 30 slots in a row and game.luau has 28 left: the sprites group at game.luau:2 takes 100 slots; the group is drawn in game.luau:4: make fewer obj[] writes or smaller groups in that scene"
        );
    }

    #[test]
    fn the_refusal_of_a_group_drawn_in_main_says_so_without_the_requires_clause() {
        let main = "local A = sprites.art({{size = 16, cells = {0}}})\nlocal g = sprites.group({slots = 1})\nGAME = scenes.add(require(\"./game\"))\nfunction frame() g:draw(A, 0, 0) end\n";
        let game = "local function frame() for i = 0, 127 do obj[i].on = true end end\nreturn {frame = frame}";
        let m = refusal_of(&[("main.luau", main), ("game.luau", game)], &[1]);
        assert!(m.contains("; the group is drawn in main.luau:4, so it counts in every scene: "), "{m}");
    }

    #[test]
    fn the_refusal_of_a_fragmented_scene_says_the_room_is_not_in_a_row() {
        let main = "GAME = scenes.add(require(\"./game\"))\n";
        let game = "local A = sprites.art({{size = 16, cells = {0}}})\nlocal g = sprites.group({slots = 100})\nlocal function frame() obj[64].on = true g:draw(A, 0, 0) end\nreturn {frame = frame}";
        let m = refusal_of(&[("main.luau", main), ("game.luau", game)], &[1]);
        assert!(m.contains("needs 100 slots in a row and game.luau has 127 left but not 100 in a row: its obj[] writes take 1 slot, "), "{m}");
    }

    #[test]
    fn the_refusal_of_a_program_without_scenes_leaves_the_scene_out() {
        let main = "local A = sprites.art({{size = 16, cells = {0}}})\nlocal a = sprites.group({slots = 100})\nlocal b = sprites.group({slots = 30})\nfunction frame() a:draw(A, 0, 0) b:draw(A, 0, 0) end\n";
        assert_eq!(refusal_of(&[("main.luau", main)], &[1]), "the sprites group at main.luau:3 needs 30 slots in a row and 28 are left: the sprites group at main.luau:2 takes 100 slots; make fewer obj[] writes or smaller groups");
    }

    #[test]
    fn the_hosted_ident_scene_runs_none_of_the_programs_sprites() {
        // A module main requires draws the game's 32 px car in a function the game's scenes call; the hosted ident
        // writes every obj slot and sets its own 8 px OBJSEL. The group and OBJSEL stay the game's.
        let main = format!("local _Art = require(\"./art\")\nGAME = scenes.add(require(\"./game\"))\nIDENT = scenes.add(require(\"./{}\"))\n", HOSTED_IDENT.trim_end_matches(".luau"));
        let art = "local A = sprites.art({{size = 32, cells = {0}}})\nlocal g = sprites.group({slots = 1})\nlocal function draw() g:draw(A, 0, 0) end\nreturn {draw = draw}";
        let game = "local Art = require(\"./art\")\nlocal function frame() Art.draw() end\nreturn {frame = frame}";
        let ident = "local function init() obj.char_base, obj.size_sel = 0x2000, 0 screen.main.obj = true end\nlocal function frame() for i = 0, 127 do obj[i].on = true end end\nreturn {init = init, frame = frame}";
        let p = plan(&[
            ("main.luau", &main),
            ("art.luau", art),
            ("game.luau", game),
            (HOSTED_IDENT, ident),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(p.groups[0].scenes, vec![1]);
        assert_eq!(p.layout(&[1, 0]).unwrap()[0], (0, 1));
        assert_eq!(
            p.size_sel, None,
            "the ident's OBJSEL pins nothing for the game's art"
        );
        assert!(
            !p.sets_char_base && !p.sets_obj_layer,
            "nor its sprite layer setup"
        );
        // The same files with the ident under another name keep the refusal.
        let main = main.replace(HOSTED_IDENT.trim_end_matches(".luau"), "ident");
        let p = plan(&[
            ("main.luau", &main),
            ("art.luau", art),
            ("game.luau", game),
            ("ident.luau", ident),
        ])
        .unwrap()
        .unwrap();
        assert!(p.layout(&[1, 0]).is_err());
    }
}
