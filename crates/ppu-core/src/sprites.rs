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

pub const UNPROVABLE_SLOT: &str = "can't tell which sprite slots this obj write uses; give the index constant bounds or draw it with sprites.draw";
pub const UNSIZED: &str = "can't tell how many sprites this draw adds to its group: bound its loops with constants or a file-level table, draw art made at file level (or a file-level table of it), or give the group {slots = n}";
pub const NO_GROUP: &str = "can't tell which sprites group this is: call draw and keep on a group made with sprites.group in this file";
pub const GROUP_PLACE: &str = "make sprites groups at file level or in init(), outside loops: each sprites.group call is one group";
pub const NO_SORT: &str = "sprites.sort() is gone: draw what sorts by depth into a group made with sprites.group({sort = true})";
pub const NO_LAYER: &str = "sprites.draw takes no layer: draw into groups, which draw in the order they are made";
pub const SIZE_SEL: &str = "with the sprites library, set obj.size_sel to one constant 0..7 (or leave it out: the library picks the size pair from the art)";
const DRAW_OPTIONS: &str = "draw takes (art, x, y) and an optional literal options table {size = , prio = , key = , depth = , flip_x = , flip_y = , pal = }";

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
    /// The scene whose module made it (from 1, as `Plan::scenes`), or 0: the program's, in every scene.
    pub scene: usize,
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
    /// A draw's art numbers (from 1), `None` when the analysis cannot tell (or a keep).
    pub arts: Option<Vec<usize>>,
}

/// A program that draws with the sprites library.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// Slots the program indexes through `obj[...]` outside every scene, ascending; the library never assigns them.
    pub reserved: Vec<u8>,
    /// A program of scenes: each scene's module file (in `scenes.add` order) and the slots its own modules index
    /// through `obj[...]`. Two scenes never run together, so each lays its groups out over the whole OAM.
    pub scenes: Vec<(String, Vec<u8>)>,
    /// `sprites.art` calls in source order: art n is `arts[n - 1]`.
    pub arts: Vec<Site>,
    /// The groups in draw order (the first on top), the default group last, which takes the free
    /// slots the others leave.
    pub groups: Vec<Group>,
    pub calls: Vec<Call>,
    /// The OBJSEL size pair the program sets itself (`obj.size_sel = n`, one constant), which the library then
    /// draws with; `None` lets the library choose it from the art.
    pub size_sel: Option<u8>,
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
                (0..free).filter(|&p| own.contains(&free_slots[p as usize])).collect()
            })
            .collect();
        let mut out = vec![];
        for g in &self.groups {
            let live: Vec<usize> = if g.scene == 0 { (0..n).collect() } else { vec![g.scene - 1] };
            let mut at = live.iter().map(|&k| ends[k]).max().unwrap_or(0);
            let size = match g.slots {
                Some(n) => n as u64,
                // The default group takes the slots the others leave.
                None if g.site.is_none() && self.scenes.is_empty() => free.saturating_sub(at),
                // shortcut: in a program of scenes the default group takes only what every scene leaves past all of
                // them (none when nothing draws into it); per-scene default groups if a scene draws with sprites.draw.
                None if g.site.is_none() => {
                    at = ends.iter().copied().max().unwrap_or(0);
                    let room = |k: usize| taken[k].iter().copied().filter(|&p| p >= at).min().unwrap_or(free) - at;
                    if g.draws.is_empty() { 0 } else { (0..n).map(room).min().unwrap_or(0) }
                }
                None => g
                    .draws
                    .iter()
                    .map(|(_, _, times, arts)| {
                        let most = arts.iter().flatten().map(|a| cells.get(a - 1).copied().unwrap_or(1)).max().unwrap_or(0);
                        times.unwrap_or(0) * most
                    })
                    .sum(),
            };
            // Past the scene's own obj slots: a group is one run of positions.
            while let Some(&p) = live.iter().flat_map(|&k| &taken[k]).filter(|&&p| p >= at && p < at + size).max() {
                at = p + 1;
            }
            if at + size > free {
                let (file, line) = match (&g.site, g.draws.first()) {
                    (Some(s), _) => (s.file.clone(), s.line),
                    (None, Some((f, l, _, _))) => (f.clone(), *l),
                    _ => (String::new(), 0),
                };
                let need: u64 = at + size;
                let free = free - live.iter().map(|&k| taken[k].len() as u64).max().unwrap_or(0);
                let need = need - live.iter().map(|&k| taken[k].iter().filter(|&&p| p < at).count() as u64).max().unwrap_or(0);
                return Err(Error { file, line, message: format!("sprites groups need {need} slots; {free} are free") });
            }
            out.push((at as u32, size as u32));
            for &k in &live {
                ends[k] = at + size;
            }
        }
        Ok(out)
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
    let ast::Prefix::Name(root) = call.prefix() else { return None };
    let mut name = text(root);
    let suffixes: Vec<_> = call.suffixes().collect();
    let (last, fields) = suffixes.split_last()?;
    for s in fields {
        let ast::Suffix::Index(ast::Index::Dot { name: field, .. }) = s else { return None };
        name = format!("{name}.{}", text(field));
    }
    let ast::Suffix::Call(ast::Call::AnonymousCall(args)) = last else { return None };
    let ast::FunctionArgs::Parentheses { arguments, .. } = args.as_ref() else { return None };
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
        Expression::UnaryOperator { unop: ast::UnOp::Minus(_), expression } => {
            Some(values(expression, lookup)?.into_iter().map(|v| -v).collect())
        }
        Expression::BinaryOperator { lhs, binop, rhs } => {
            let (a, b) = (values(lhs, lookup)?, values(rhs, lookup)?);
            match binop {
                ast::BinOp::Plus(_) => combine(&a, &b, |x, y| Some(x + y)),
                ast::BinOp::Minus(_) => combine(&a, &b, |x, y| Some(x - y)),
                ast::BinOp::Star(_) => combine(&a, &b, |x, y| Some(x * y)),
                ast::BinOp::DoubleSlash(_) => combine(&a, &b, |x, y| (y != 0).then(|| x.div_euclid(y))),
                ast::BinOp::Percent(_) => combine(&a, &b, |x, y| (y != 0).then(|| x.rem_euclid(y))),
                _ => None,
            }
        }
        Expression::Var(ast::Var::Name(n)) => match lookup(&text(n)) {
            Bound::Numbers(s) => Some(s),
            _ => None,
        },
        Expression::Var(ast::Var::Expression(v)) => {
            let ast::Prefix::Name(root) = v.prefix() else { return None };
            let [ast::Suffix::Index(ast::Index::Dot { name, .. })] = v.suffixes().collect::<Vec<_>>()[..] else {
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
    let Expression::FunctionCall(call) = e else { return None };
    let (name, args) = path(call)?;
    let (true, [Expression::TableConstructor(t)]) = (name == "table.freeze", &args[..]) else { return None };
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

/// Names assigned after their declaration anywhere in a file.
fn reassigned(ast: &ast::Ast) -> HashSet<String> {
    #[derive(Default)]
    struct Assigned(HashSet<String>);
    impl Visitor for Assigned {
        fn visit_assignment(&mut self, node: &ast::Assignment) {
            for v in node.variables() {
                if let ast::Var::Name(n) = v {
                    self.0.insert(text(n));
                }
            }
        }
        fn visit_compound_assignment(&mut self, node: &ast::CompoundAssignment) {
            if let ast::Var::Name(n) = node.lhs() {
                self.0.insert(text(n));
            }
        }
    }
    let mut a = Assigned::default();
    a.visit_ast(ast);
    a.0
}

/// A file-level table of art: its fields' art numbers, by field name (positional fields by index).
type ArtTable = Vec<(Option<String>, usize)>;

/// A draw or keep found in a file, before its group's name is resolved.
struct Found {
    line: u32,
    call: Range<usize>,
    receiver: Option<String>,
    keep: bool,
    args: Vec<Range<usize>>,
    options: Vec<(String, Range<usize>)>,
    /// Draws: the function it is in, its loops' trip count, and its art.
    function: usize,
    trips: Option<u64>,
    arts: Option<Vec<usize>>,
}

struct Walk<'a> {
    file: &'a str,
    modules: &'a HashMap<String, HashMap<String, i64>>,
    reassigned: HashSet<String>,
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
    art_at: HashMap<Range<usize>, usize>,
    art_names: HashMap<String, usize>,
    art_tables: HashMap<String, ArtTable>,
    /// Locals bound to art (`local a = ART[k]`): the art they can be, by name (the latest binding).
    art_locals: HashMap<String, Vec<usize>>,
    /// File-level tables and their field counts (generic `for` trip counts).
    tables: HashMap<String, u64>,
    /// Groups so far (all files); this file's group names.
    groups: &'a mut Vec<Group>,
    group_names: HashMap<String, usize>,
    group_at: HashMap<Range<usize>, usize>,
    /// Function bodies: their names ("" anonymous), the stack of open ones, the name the next body takes.
    names: Vec<String>,
    open: Vec<usize>,
    next_name: Option<String>,
    /// Trip counts of the open loops (None: unknown).
    trips: Vec<Option<u64>>,
    /// Calls by name: (calling function, callee name, trip count of the call's loops).
    called: Vec<(usize, String, Option<u64>)>,
    found: Vec<Found>,
    /// `name = sprites.group(...)`: the call's span and the name it is assigned to.
    pending_group: Option<(Range<usize>, String)>,
    /// The values `obj.size_sel` is set to (None: one that is not a constant 0..7), with their lines.
    size_sel: Vec<(u32, Option<u8>)>,
}

fn span(node: &impl Node) -> Range<usize> {
    let mut tokens = node.tokens().map(|t| t.token().start_position().bytes()..t.token().end_position().bytes());
    let first = tokens.next().unwrap_or(0..0);
    tokens.fold(first, |a, b| a.start.min(b.start)..a.end.max(b.end))
}

/// `X:method(args)`: the receiver's name, the method and its arguments.
fn method(call: &ast::FunctionCall) -> Option<(String, String, Vec<&Expression>)> {
    let ast::Prefix::Name(root) = call.prefix() else { return None };
    let [ast::Suffix::Call(ast::Call::MethodCall(m))] = call.suffixes().collect::<Vec<_>>()[..] else { return None };
    let ast::FunctionArgs::Parentheses { arguments, .. } = m.args() else { return None };
    Some((text(root), text(m.name()), arguments.iter().collect()))
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
        self.error.get_or_insert(Error { file: self.file.into(), line: line(node), message: message.into() });
    }
    fn trip_product(&self) -> Option<u64> {
        self.trips.iter().try_fold(1u64, |a, t| t.map(|t| a.saturating_mul(t)))
    }
    /// The art numbers an art argument can be: a file-level art, or a field of a file-level table of art.
    fn art_of(&self, e: &Expression) -> Option<Vec<usize>> {
        match e {
            Expression::Parentheses { expression, .. } => self.art_of(expression),
            // Luau's `if c then a else b`, as the preview runs it: `((c) and (a) or (b))`, either branch.
            Expression::BinaryOperator { lhs, binop: ast::BinOp::And(_), rhs } if self.art_of(lhs).is_none() => self.art_of(rhs),
            Expression::BinaryOperator { lhs, binop: ast::BinOp::Or(_), rhs } => {
                let mut arts = self.art_of(lhs)?;
                arts.extend(self.art_of(rhs)?);
                Some(arts)
            }
            Expression::Var(ast::Var::Name(n)) => match self.art_names.get(&text(n)) {
                Some(a) => Some(vec![*a]),
                None => self.art_locals.get(&text(n)).cloned(),
            },
            Expression::Var(ast::Var::Expression(v)) => {
                let ast::Prefix::Name(root) = v.prefix() else { return None };
                let table = self.art_tables.get(&text(root))?;
                match v.suffixes().collect::<Vec<_>>()[..] {
                    [ast::Suffix::Index(ast::Index::Dot { name, .. })] => {
                        table.iter().find(|(k, _)| k.as_deref() == Some(text(name).as_str())).map(|(_, a)| vec![*a])
                    }
                    [ast::Suffix::Index(ast::Index::Brackets { .. })] => Some(table.iter().map(|(_, a)| *a).collect()),
                    _ => None,
                }
            }
            _ => None,
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
            self.fail(call, "sprites.group takes an optional literal options table {sort = , slots = }");
            return;
        };
        let mut group = Group {
            site: Some(Site { file: self.file.into(), line: line(call), head: span(call).start..range_end(call), call: span(call) }),
            sort: false,
            slots: None,
            draws: vec![],
            scene: 0,
        };
        for (k, v) in options {
            match (k.as_str(), v) {
                ("sort", Expression::Symbol(t)) => group.sort = text(t) == "true",
                ("slots", v) => match self.values(v).filter(|s| s.len() == 1).and_then(|s| s.first().copied()) {
                    Some(n) if (0..=128).contains(&n) => group.slots = Some(n as u32),
                    _ => self.fail(call, "sprites.group slots must be a constant 0..128"),
                },
                _ => self.fail(call, "sprites.group takes an optional literal options table {sort = , slots = }"),
            }
        }
        let index = self.groups.len();
        self.groups.push(group);
        self.group_at.insert(span(call), index);
        if let Some(name) = name {
            self.group_names.insert(name, index);
        }
    }
    fn draw_or_keep(&mut self, call: &ast::FunctionCall, receiver: Option<String>, keep: bool, args: &[&Expression]) {
        self.draws = true;
        let function = self.open.last().copied().unwrap_or(usize::MAX);
        let mut found = Found {
            line: line(call),
            call: span(call),
            receiver,
            keep,
            args: args.iter().map(|a| span(*a)).collect(),
            options: vec![],
            function,
            trips: self.trip_product(),
            arts: None,
        };
        if keep {
            if args.len() != 1 {
                self.fail(call, "keep takes one key");
            }
        } else {
            let options = fields(args.get(3));
            let ok = options.as_ref().is_some_and(|o| o.iter().all(|(k, _)| matches!(k.as_str(), "depth" | "size" | "prio" | "key" | "layer" | "flip_x" | "flip_y" | "pal")));
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
        if let ([n], [Expression::Function(_)]) = (&node.names().iter().collect::<Vec<_>>()[..], &node.expressions().iter().collect::<Vec<_>>()[..]) {
            self.next_name = Some(text(n));
        }
    }
    fn visit_numeric_for(&mut self, node: &ast::NumericFor) {
        self.loops += 1;
        let step = node.step().map_or(Some(Set::from([1])), |s| self.values(s));
        let (start, end) = (self.values(node.start()), self.values(node.end()));
        let range = match (&start, &end, &step) {
            (Some(a), Some(b), Some(s)) if s.len() == 1 && *s.first().unwrap() != 0 => {
                let (lo, hi) = (*a.first().unwrap().min(b.first().unwrap()), *a.last().unwrap().max(b.last().unwrap()));
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
                    Some(((*a.last().unwrap() - *b.first().unwrap()).max(-1) / -s + 1).max(0) as u64)
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
        self.trips.push(table.and_then(|t| self.tables.get(&t).copied()));
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
        // `obj.size_sel = n`: the program's own size pair.
        for (i, v) in vars.iter().enumerate() {
            let ast::Var::Expression(v) = v else { continue };
            let is_size_sel = matches!(v.prefix(), ast::Prefix::Name(r) if text(r) == "obj")
                && matches!(v.suffixes().collect::<Vec<_>>()[..], [ast::Suffix::Index(ast::Index::Dot { name, .. })] if text(name) == "size_sel");
            if is_size_sel {
                let value = exprs.get(i).and_then(|e| self.values(e)).filter(|s| s.len() == 1).and_then(|s| s.first().copied());
                self.size_sel.push((line(node), value.filter(|n| (0..8).contains(n)).map(|n| n as u8)));
            }
        }
    }
    fn visit_local_assignment_end(&mut self, node: &ast::LocalAssignment) {
        // A file-level local never assigned again keeps its constant value.
        let top = self.frames.len() == 1;
        let exprs: Vec<_> = node.expressions().iter().collect();
        let names: Vec<_> = node.names().iter().map(text).collect();
        for (i, name) in names.into_iter().enumerate() {
            match exprs.get(i).and_then(|e| self.art_of(e)) {
                Some(arts) if !top => {
                    self.art_locals.insert(name.clone(), arts);
                }
                _ => {
                    self.art_locals.remove(&name);
                }
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
                match exprs.get(i) {
                    Some(e @ Expression::FunctionCall(_)) => {
                        if let Some(a) = self.art_call(e) {
                            self.art_names.insert(name.clone(), a);
                        }
                    }
                    Some(Expression::TableConstructor(t)) => {
                        self.tables.insert(name.clone(), t.fields().len() as u64);
                        let mut arts = vec![];
                        let mut all = true;
                        for (k, f) in t.fields().iter().enumerate() {
                            let (key, value) = match f {
                                ast::Field::NameKey { key, value, .. } => (Some(text(key)), &**value),
                                ast::Field::NoKey(value) => (Some((k + 1).to_string()), value),
                                _ => {
                                    all = false;
                                    continue;
                                }
                            };
                            match self.art_call(value) {
                                Some(a) => arts.push((key, a)),
                                None => all = false,
                            }
                        }
                        if all && !arts.is_empty() {
                            self.art_tables.insert(name.clone(), arts);
                        }
                    }
                    _ => {}
                }
            }
            let bound = match exprs.get(i) {
                Some(e) if top && !self.reassigned.contains(&name) => {
                    if let Some(s) = self.values(e) {
                        Bound::Numbers(s)
                    } else if let Some(f) = frozen(e) {
                        Bound::Fields(f)
                    } else if let Expression::FunctionCall(call) = e {
                        match path(call) {
                            Some((n, args)) if n == "require" => match args[..] {
                                [Expression::String(s)] => {
                                    let p = text(s);
                                    let p = p.trim_matches(|c| c == '"' || c == '\'');
                                    let stem = p.strip_prefix("./").unwrap_or(p);
                                    self.modules.get(stem).cloned().map_or(Bound::Unknown, Bound::Fields)
                                }
                                _ => Bound::Unknown,
                            },
                            _ => Bound::Unknown,
                        }
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
            if !shadowed && (name == "draw" || name == "keep") && receiver != "sprites" {
                self.draw_or_keep(call, Some(receiver), name == "keep", &args);
            }
            return;
        }
        // A call by name: the call graph that multiplies a function's draws.
        if let ast::Prefix::Name(n) = call.prefix() {
            if let [ast::Suffix::Call(ast::Call::AnonymousCall(_))] = call.suffixes().collect::<Vec<_>>()[..] {
                let caller = self.open.last().copied().unwrap_or(usize::MAX);
                self.called.push((caller, text(n), self.trip_product()));
            }
        }
        let Some((name, args)) = path(call) else { return };
        if shadowed {
            return;
        }
        match name.as_str() {
            "sprites.draw" => self.draw_or_keep(call, None, false, &args),
            "sprites.keep" => self.draw_or_keep(call, None, true, &args),
            "sprites.sort" => self.fail(call, NO_SORT),
            "sprites.group" => {
                let named = self.pending_group.take().filter(|(s, _)| *s == span(call)).map(|(_, n)| n);
                self.group(call, &args, named);
            }
            "sprites.hold" => {
                self.draws = true;
                if !args.is_empty() {
                    self.fail(call, "sprites.hold takes no arguments");
                }
            }
            "sprites.art" => {
                self.art = true;
                if self.functions > 0 || self.loops > 0 {
                    self.fail(call, "sprites.art must be called at file level, outside functions and loops, so each call makes one art");
                }
                let head = span(call).start..range_end(call);
                self.arts.push(Site { file: self.file.into(), line: line(call), head, call: span(call) });
                self.art_at.insert(span(call), self.arts.len());
            }
            _ => {}
        }
    }
    fn visit_var_expression(&mut self, v: &ast::VarExpression) {
        let ast::Prefix::Name(root) = v.prefix() else { return };
        if text(root) != "obj" || self.frames.iter().flatten().any(|(n, _)| n == "obj") {
            return;
        }
        if let Some(ast::Suffix::Index(ast::Index::Brackets { expression, .. })) = v.suffixes().next() {
            let s = self.values(expression);
            self.slots.push((line(v), s));
        }
    }
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
    let ast::LastStmt::Return(r) = ast.nodes().last_stmt()? else { return None };
    let [e] = r.returns().iter().collect::<Vec<_>>()[..] else { return None };
    frozen(e)
}

/// `source` (the file `file`) with each of the plan's `sprites.art(` and `sprites.group(` calls
/// passing its number, as `__ppu_sprites_art(n, ...)` and `__ppu_sprites_group(n, ...)`: the
/// library then knows each art's and group's place in the plan. Lines are unchanged.
pub fn number_sites(plan: &Plan, file: &str, source: &str) -> String {
    let mut edits: Vec<(Range<usize>, String)> = vec![];
    let sites = plan.arts.iter().enumerate().map(|(i, s)| (s, format!("__ppu_sprites_art({}", i + 1)));
    let groups = plan.groups.iter().enumerate().filter_map(|(i, g)| g.site.as_ref().map(|s| (s, format!("__ppu_sprites_group({}", i + 1))));
    for (site, head) in sites.chain(groups) {
        if site.file != file {
            continue;
        }
        // An empty argument list takes the number alone.
        let empty = source[site.head.end..site.call.end].trim_start().starts_with(')');
        edits.push((site.head.clone(), if empty { head } else { format!("{head}, ") }));
    }
    edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    let mut out = source.to_string();
    for (r, text) in edits {
        out.replace_range(r, &text);
    }
    out
}

/// Each file a scene owns, by the scene's number (from 1, in `scenes.add` order): its module and the modules it
/// requires, transitively, unless the program's own files (those no scene reaches) require them too; a module two
/// scenes require is the first's. Every other file is the program's.
pub fn scene_owners(files: &[(&str, &str)]) -> HashMap<String, usize> {
    let scenes = crate::frontend::scene_files(files);
    let source = |f: &str| files.iter().find(|(n, _)| *n == f).map(|(_, s)| *s);
    let resolve = |path: &str| [format!("{path}.luau"), format!("{path}.lua")].into_iter().find(|f| source(f).is_some());
    // Every file reached from `roots`, never entering a scene module other than a root.
    let reach = |roots: Vec<String>| {
        let mut seen: Vec<String> = vec![];
        let mut todo = roots.clone();
        while let Some(f) = todo.pop() {
            if seen.contains(&f) || (scenes.contains(&f) && !roots.contains(&f)) {
                continue;
            }
            todo.extend(crate::frontend::requires(source(&f).unwrap_or("")).into_iter().filter_map(|p| resolve(p)));
            seen.push(f);
        }
        seen
    };
    let mut owners = HashMap::new();
    for (k, scene) in scenes.iter().enumerate() {
        for f in reach(vec![scene.clone()]) {
            owners.entry(f).or_insert(k + 1);
        }
    }
    let program: Vec<String> = files.iter().map(|(f, _)| f.to_string()).filter(|f| !owners.contains_key(f)).collect();
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
    if !files.iter().any(|(_, s)| s.contains("sprites.") || s.contains(":draw(") || s.contains(":keep(")) {
        return Ok(None);
    }
    let parsed: Vec<(&str, ast::Ast)> = files
        .iter()
        .filter(|(_, s)| s.contains("sprites") || s.contains("obj") || s.contains(":draw(") || s.contains(":keep("))
        .filter_map(|(f, s)| full_moon::parse_fallible(s, full_moon::LuaVersion::luau()).into_result().ok().map(|a| (*f, a)))
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
    let mut draws = false;
    let mut slots = vec![];
    let mut first_error = None;
    let mut arts = vec![];
    let mut groups = vec![];
    // The default group's draws, and every call with its group (`usize::MAX`: the default).
    let mut default = vec![];
    let mut calls = vec![];
    let mut size_sels = vec![];
    for (file, ast) in &parsed {
        let mut walk = Walk {
            file,
            modules: &modules,
            reassigned: reassigned(ast),
            frames: vec![],
            pending: vec![],
            functions: 0,
            loops: 0,
            draws: false,
            art: false,
            slots: vec![],
            error: None,
            arts: &mut arts,
            art_at: HashMap::new(),
            art_names: HashMap::new(),
            art_tables: HashMap::new(),
            art_locals: HashMap::new(),
            tables: HashMap::new(),
            groups: &mut groups,
            group_names: HashMap::new(),
            group_at: HashMap::new(),
            names: vec![],
            open: vec![],
            next_name: None,
            trips: vec![],
            called: vec![],
            found: vec![],
            pending_group: None,
            size_sel: vec![],
        };
        walk.visit_ast(ast);
        // How many times a function runs a frame: once when the file never calls it by name (an entry,
        // or a module's export), else the sum over its calls; recursion is unknown.
        fn times(f: usize, names: &[String], called: &[(usize, String, Option<u64>)], depth: usize) -> Option<u64> {
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
                Some(sum.saturating_add(trips.as_ref()?.saturating_mul(times(*caller, names, called, depth + 1)?)))
            })
        }
        for f in std::mem::take(&mut walk.found) {
            let group = match &f.receiver {
                None => usize::MAX,
                Some(r) => match walk.group_names.get(r) {
                    Some(g) => *g,
                    None => {
                        walk.error.get_or_insert(Error { file: file.to_string(), line: f.line, message: NO_GROUP.into() });
                        continue;
                    }
                },
            };
            if !f.keep {
                let times = f.trips.and_then(|t| Some(t.saturating_mul(times(f.function, &walk.names, &walk.called, 0)?)));
                let need = (file.to_string(), f.line, times, f.arts.clone());
                if group == usize::MAX {
                    default.push(need);
                } else {
                    walk.groups[group].draws.push(need);
                }
            }
            calls.push(Call { file: file.to_string(), line: f.line, call: f.call, group, keep: f.keep, args: f.args, options: f.options, arts: f.arts });
        }
        if let Some(e) = walk.error {
            first_error.get_or_insert(e);
        }
        draws |= walk.draws;
        size_sels.extend(walk.size_sel.into_iter().map(|(l, v)| (file.to_string(), l, v)));
        slots.extend(walk.slots.into_iter().map(|(l, s)| (file.to_string(), l, s)));
    }
    if let Some(e) = first_error {
        return Err(e);
    }
    if !draws {
        return Ok(None);
    }
    let owners = scene_owners(files);
    let scene_list = crate::frontend::scene_files(files);
    let owner = |file: &str| owners.get(file).copied().unwrap_or(0);
    // Per owner (0: the program, k: scene k) the slots its obj writes take.
    let mut reserved = vec![BTreeSet::new(); scene_list.len() + 1];
    for g in &mut groups {
        g.scene = g.site.as_ref().map_or(0, |s| owner(&s.file));
    }
    for (file, line, set) in slots {
        // A scene that makes no group of its own may write any slot: nothing of its own competes for them.
        let set = match set {
            Some(set) => set,
            None if owner(&file) != 0 && !groups.iter().any(|g| g.scene == owner(&file)) => (0..128).collect(),
            None => return Err(Error { file, line, message: UNPROVABLE_SLOT.into() }),
        };
        reserved[owner(&file)].extend(set.into_iter().filter(|s| (0..128).contains(s)).map(|s| s as u8));
    }
    // The default group draws behind the others.
    let index = groups.len();
    groups.push(Group { site: None, sort: false, slots: None, draws: default, scene: 0 });
    for c in &mut calls {
        if c.group == usize::MAX {
            c.group = index;
        }
    }
    // Every group without `slots` must bound its draws.
    for g in groups.iter().filter(|g| g.site.is_some()) {
        if g.slots.is_none() {
            if let Some((file, line, _, _)) = g.draws.iter().find(|(_, _, t, a)| t.is_none() || a.is_none()) {
                return Err(Error { file: file.clone(), line: *line, message: UNSIZED.into() });
            }
        }
    }
    let mut size_sel = None;
    for (file, line, v) in size_sels {
        match (v, size_sel) {
            (Some(v), None) => size_sel = Some(v),
            (Some(v), Some(w)) if v == w => {}
            _ => return Err(Error { file, line, message: SIZE_SEL.into() }),
        }
    }
    let scenes = scene_list.into_iter().zip(reserved.drain(1..)).map(|(f, r)| (f, r.into_iter().collect())).collect();
    Ok(Some(Plan { reserved: reserved.remove(0).into_iter().collect(), scenes, arts, groups, calls, size_sel }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reserved(files: &[(&str, &str)]) -> Result<Option<Vec<u8>>, Error> {
        plan(files).map(|p| p.map(|p| p.reserved))
    }

    #[test]
    fn raw_only_programs_have_no_plan() {
        assert_eq!(reserved(&[("main.lua", "function frame() obj[0].x = 3 end")]), Ok(None));
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
        assert_eq!(reserved(&[("c.luau", c), ("main.luau", main)]), Ok(Some(vec![0, 10, 11, 12, 20, 21])));
    }

    #[test]
    fn unprovable_indices_and_misplaced_art_are_errors() {
        let main = "function frame(t, f)\n  sprites.draw(A, 0, 0)\n  obj[f % 200].x = 1\nend";
        let e = plan(&[("main.lua", main)]).unwrap_err();
        assert_eq!((e.line, e.message.as_str()), (3, UNPROVABLE_SLOT));
        // Reassigned file-level locals are not constants.
        let main = "local K = 1\nK = 2\nfunction frame()\n  sprites.draw(A, 0, 0)\n  obj[K].x = 1\nend";
        assert_eq!(plan(&[("main.lua", main)]).unwrap_err().line, 5);
        let main = "function init() A = sprites.art({}, nil) end\nfunction frame() sprites.draw(A, 0, 0) end";
        assert!(plan(&[("main.lua", main)]).unwrap_err().message.contains("file level"));
    }

    #[test]
    fn a_constant_size_sel_pins_the_pair() {
        let size_sel = |main: &str| plan(&[("main.lua", main)]).map(|p| p.unwrap().size_sel);
        assert_eq!(size_sel("function frame() sprites.draw(A, 0, 0) end"), Ok(None));
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
        let main = "SWARM = scenes.add(require(\"./swarm\"))\nFLEET = scenes.add(require(\"./fleet\"))\n";
        // The swarm's obj slots are its own; the fleet's groups are in a module only it requires.
        let swarm = "local function frame() for i = 0, 127 do obj[i].on = true end end\nreturn {frame = frame}";
        let fleet = "local Ships = require(\"./ships\")\nlocal function frame() Ships.draw() end\nreturn {frame = frame}";
        let ships = "local A = sprites.art({{size = 16, cells = {0}}})\nlocal g = sprites.group()\nlocal h = sprites.group()\nlocal function draw()\n  for i = 1, 100 do g:draw(A, 0, 0) end\n  for i = 1, 28 do h:draw(A, 0, 0) end\nend\nreturn {draw = draw}";
        let files = |main: &str| vec![("main.luau", main.to_string()), ("swarm.luau", swarm.into()), ("fleet.luau", fleet.into()), ("ships.luau", ships.into())];
        let plan_of = |files: &[(&str, String)]| {
            let refs: Vec<(&str, &str)> = files.iter().map(|(n, s)| (*n, s.as_str())).collect();
            plan(&refs).unwrap().unwrap()
        };
        let layout = |files: &[(&str, String)]| plan_of(files).layout(&[1]);
        let p = plan_of(&files(main));
        assert_eq!(p.reserved, Vec::<u8>::new());
        assert_eq!(p.scenes[0].1.len(), 128);
        assert_eq!(p.groups.iter().map(|g| g.scene).collect::<Vec<_>>(), [2, 2, 0]);
        assert_eq!(layout(&files(main)), Ok(vec![(0, 100), (100, 28), (128, 0)]));
        // The program's own obj slot counts against every scene.
        let shared = format!("{main}function init() obj[127].on = true end\n");
        assert_eq!(layout(&files(&shared)).unwrap_err().message, "sprites groups need 128 slots; 127 are free");
    }
}
