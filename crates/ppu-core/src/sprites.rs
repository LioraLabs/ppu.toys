//! Static facts about a program's use of the `sprites` library (sprites.lua): whether it draws
//! managed sprites, and which OAM slots it indexes directly through `obj[...]`. Those slots stay
//! the program's own; the library assigns managed sprites to the rest. The analysis is
//! deliberately conservative: an `obj` index must be a literal, a constant, a numeric `for`
//! variable with constant bounds, or arithmetic on those.
use full_moon::{
    ast::{self, Expression},
    node::Node,
    tokenizer::TokenReference,
    visitors::Visitor,
};
use std::collections::{BTreeSet, HashMap, HashSet};

/// A keyed draw with a layer or depth: its block's slots fix where it draws.
pub const KEYED_ORDER: &str = "a keyed sprites.draw has its own slots, so layer and depth do not apply to it";
pub const UNPROVABLE_SLOT: &str = "can't tell which sprite slots this obj write uses; give the index constant bounds or draw it with sprites.draw";

/// Where the analysis rejects a program.
#[derive(Debug, Clone, PartialEq)]
pub struct Error {
    pub file: String,
    pub line: u32,
    pub message: String,
}

/// A program that calls `sprites.draw`.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// Slots the program indexes through `obj[...]`, ascending; the library never assigns them.
    pub reserved: Vec<u8>,
}

impl Plan {
    /// The slots the library assigns, ascending.
    pub fn free(&self) -> Vec<u8> {
        (0..128u8).filter(|s| !self.reserved.contains(s)).collect()
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
        for p in node.parameters() {
            if let ast::Parameter::Name(n) = p {
                self.pending.push((text(n), Bound::Unknown));
            }
        }
    }
    fn visit_function_body_end(&mut self, _: &ast::FunctionBody) {
        self.functions -= 1;
    }
    fn visit_numeric_for(&mut self, node: &ast::NumericFor) {
        self.loops += 1;
        let step = node.step().map_or(Some(Set::from([1])), |s| self.values(s));
        let range = match (self.values(node.start()), self.values(node.end()), step) {
            (Some(a), Some(b), Some(s)) if s.len() == 1 && *s.first().unwrap() != 0 => {
                let (lo, hi) = (*a.first().unwrap().min(b.first().unwrap()), *a.last().unwrap().max(b.last().unwrap()));
                (hi - lo <= 1 << 14).then(|| (lo..=hi).collect::<Set>())
            }
            _ => None,
        };
        let bound = range.map_or(Bound::Unknown, Bound::Numbers);
        self.pending.push((text(node.index_variable()), bound));
    }
    fn visit_numeric_for_end(&mut self, _: &ast::NumericFor) {
        self.loops -= 1;
    }
    fn visit_generic_for(&mut self, node: &ast::GenericFor) {
        self.loops += 1;
        for n in node.names() {
            self.pending.push((text(n), Bound::Unknown));
        }
    }
    fn visit_generic_for_end(&mut self, _: &ast::GenericFor) {
        self.loops -= 1;
    }
    fn visit_while(&mut self, _: &ast::While) {
        self.loops += 1;
    }
    fn visit_while_end(&mut self, _: &ast::While) {
        self.loops -= 1;
    }
    fn visit_repeat(&mut self, _: &ast::Repeat) {
        self.loops += 1;
    }
    fn visit_repeat_end(&mut self, _: &ast::Repeat) {
        self.loops -= 1;
    }
    fn visit_local_function(&mut self, node: &ast::LocalFunction) {
        if let Some(frame) = self.frames.last_mut() {
            frame.push((text(node.name()), Bound::Unknown));
        }
    }
    fn visit_local_assignment_end(&mut self, node: &ast::LocalAssignment) {
        // A file-level local never assigned again keeps its constant value.
        let top = self.frames.len() == 1;
        let exprs: Vec<_> = node.expressions().iter().collect();
        let names: Vec<_> = node.names().iter().map(text).collect();
        for (i, name) in names.into_iter().enumerate() {
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
        let Some((name, args)) = path(call) else { return };
        if self.frames.iter().flatten().any(|(n, _)| n == "sprites") {
            return;
        }
        match name.as_str() {
            "sprites.draw" => {
                self.draws = true;
                let names: Option<Vec<String>> = match args.get(3) {
                    None => Some(vec![]),
                    Some(Expression::TableConstructor(t)) => t
                        .fields()
                        .iter()
                        .map(|f| match f {
                            ast::Field::NameKey { key, .. } => Some(text(key)),
                            _ => None,
                        })
                        .collect(),
                    _ => None,
                };
                let options_ok = names.as_ref().is_some_and(|n| n.iter().all(|k| matches!(k.as_str(), "layer" | "depth" | "size" | "prio" | "key")));
                if !(3..=4).contains(&args.len()) || !options_ok {
                    self.fail(call, "sprites.draw takes (art, x, y) and an optional literal options table {layer = , depth = , size = , prio = , key = }");
                } else if names.is_some_and(|n| n.iter().any(|k| k == "key") && n.iter().any(|k| k == "layer" || k == "depth")) {
                    self.fail(call, KEYED_ORDER);
                }
            }
            "sprites.keep" => {
                self.draws = true;
                if args.len() != 1 {
                    self.fail(call, "sprites.keep takes one key");
                }
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

/// The value fields of `return table.freeze({...})` at the end of a module.
fn module_fields(ast: &ast::Ast) -> Option<HashMap<String, i64>> {
    let ast::LastStmt::Return(r) = ast.nodes().last_stmt()? else { return None };
    let [e] = r.returns().iter().collect::<Vec<_>>()[..] else { return None };
    frozen(e)
}

/// The sprite plan of `files` (prepared sources, as the preview runs them): `None` for a program
/// that never calls `sprites.draw`. Files that do not parse as Luau are skipped; they cannot
/// reach the library through this syntax.
pub fn plan(files: &[(&str, &str)]) -> Result<Option<Plan>, Error> {
    if !files.iter().any(|(_, s)| s.contains("sprites.")) {
        return Ok(None);
    }
    let parsed: Vec<(&str, ast::Ast)> = files
        .iter()
        .filter(|(_, s)| s.contains("sprites") || s.contains("obj"))
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
        };
        walk.visit_ast(ast);
        if let Some(e) = walk.error {
            first_error.get_or_insert(e);
        }
        draws |= walk.draws;
        slots.extend(walk.slots.into_iter().map(|(l, s)| (file.to_string(), l, s)));
    }
    if let Some(e) = first_error {
        return Err(e);
    }
    if !draws {
        return Ok(None);
    }
    let mut reserved = BTreeSet::new();
    for (file, line, set) in slots {
        let Some(set) = set else {
            return Err(Error { file, line, message: UNPROVABLE_SLOT.into() });
        };
        reserved.extend(set.into_iter().filter(|s| (0..128).contains(s)).map(|s| s as u8));
    }
    Ok(Some(Plan { reserved: reserved.into_iter().collect() }))
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
}
