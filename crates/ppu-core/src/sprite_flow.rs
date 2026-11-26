//! Where a program's animations can go. An animation is made at file level and reaches a draw through names: file-level
//! locals and tables of them (the plan's trees), the tables modules return, locals and assignments, the parameters of the
//! program's own functions, and generic `for` loops over them. A draw whose art reads none of these can never show an
//! animation, so the cartridge draws it as plain art. Any other way out (a runtime table, a return, a global, another
//! function) is one the plan can't follow: an escape, which the cartridge refuses by name.
use super::{required, required_field, span, steps, text, Error, Step, Tree, ANIMATION};
use full_moon::ast::{self, Expression};
use full_moon::node::Node;
use full_moon::tokenizer::TokenReference;
use full_moon::visitors::Visitor;
use std::collections::{HashMap, HashSet};
use std::ops::Range;

pub(super) const ESCAPE: &str = "an animation can't be followed from here on the cartridge: keep animations in file-level locals and tables (or a module's returned table), and pass them only to this program's own functions or a draw";

/// What the walk learned about one file, by declaration: the declaration each name reads, its art, the modules its
/// names require (stems), and `local g = M.f`.
pub(super) struct Facts {
    pub names: super::scope::Names,
    pub art_names: HashMap<usize, usize>,
    pub art_tables: HashMap<usize, Tree>,
    pub requires: HashMap<usize, String>,
    pub module_fields: HashMap<usize, (String, String)>,
}

/// Per file (by stem): its source name, tree, facts; and each module's returned art by field.
pub(super) struct Program<'a> {
    pub files: Vec<(&'a str, &'a ast::Ast, Facts)>,
    pub exports: HashMap<String, HashMap<String, Tree>>,
}

/// The draws (file, call span) whose art can be an animation the plan could not name, and the escapes.
pub(super) fn follow(program: &Program) -> (HashSet<(String, Range<usize>)>, Vec<Error>) {
    let stem = |f: &str| {
        f.trim_end_matches(".luau")
            .trim_end_matches(".lua")
            .to_string()
    };
    // Each file's functions by name (their parameters) and the functions its returned table exports by field.
    let mut functions: HashMap<String, HashMap<String, Vec<String>>> = HashMap::new();
    let mut exported: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut methods = HashSet::new();
    for (file, ast, facts) in &program.files {
        let mut f = Functions::default();
        f.visit_ast(ast);
        functions.insert(stem(file), f.0);
        methods.extend(f.1);
        exported.insert(stem(file), exported_functions(ast, &facts.names));
    }
    let mut state = State::default();
    loop {
        let before = (
            state.decls.values().map(HashSet::len).sum::<usize>(),
            state.params.len(),
        );
        for (file, ast, facts) in &program.files {
            let mut v = Flow::new(
                program, &functions, &exported, &methods, &mut state, file, ast, facts, false,
            );
            v.visit_ast(ast);
        }
        if before
            == (
                state.decls.values().map(HashSet::len).sum::<usize>(),
                state.params.len(),
            )
        {
            break;
        }
    }
    let (mut draws, mut escapes) = (HashSet::new(), vec![]);
    for (file, ast, facts) in &program.files {
        let mut v = Flow::new(
            program, &functions, &exported, &methods, &mut state, file, ast, facts, true,
        );
        v.visit_ast(ast);
        draws.extend(v.draws.into_iter().map(|d| (file.to_string(), d)));
        escapes.extend(v.escapes);
    }
    (draws, escapes)
}

#[derive(Default)]
struct State {
    /// Per file, the declarations (as `scope` numbers them) that can hold an animation.
    decls: HashMap<String, HashSet<usize>>,
    /// (stem, function, parameter index) that can be passed one.
    params: HashSet<(String, String, usize)>,
}

#[derive(Default)]
struct Functions(HashMap<String, Vec<String>>, HashSet<String>);
impl Visitor for Functions {
    fn visit_local_function(&mut self, f: &ast::LocalFunction) {
        self.0.insert(text(f.name()), params(f.body()));
    }
    fn visit_function_declaration(&mut self, f: &ast::FunctionDeclaration) {
        if let Some(n) = declared(f) {
            self.0.insert(n, params(f.body()));
        }
        if let Some(m) = f.name().method_name() {
            self.1.insert(text(m));
        }
    }
    fn visit_local_assignment(&mut self, a: &ast::LocalAssignment) {
        for (n, e) in a.names().iter().zip(a.expressions().iter()) {
            if let Expression::Function(f) = e {
                self.0.insert(text(n), params(f.body()));
            }
        }
    }
    fn visit_assignment(&mut self, a: &ast::Assignment) {
        if let Some((n, body)) = assigned_function(a) {
            self.0.insert(n, params(body));
        }
    }
}

/// `f = function ... end` or `M.f = function ... end`: the name it is keyed by, as `declared`, and its body.
fn assigned_function(a: &ast::Assignment) -> Option<(String, &ast::FunctionBody)> {
    let (vars, exprs): (Vec<_>, Vec<_>) = (
        a.variables().iter().collect(),
        a.expressions().iter().collect(),
    );
    let ([v], [Expression::Function(f)]) = (&vars[..], &exprs[..]) else {
        return None;
    };
    let name = match v {
        ast::Var::Name(n) => text(n),
        ast::Var::Expression(e) => match (e.prefix(), &e.suffixes().collect::<Vec<_>>()[..]) {
            (ast::Prefix::Name(m), [ast::Suffix::Index(ast::Index::Dot { name, .. })]) => {
                format!("{}.{}", text(m), text(name))
            }
            _ => return None,
        },
        _ => return None,
    };
    Some((name, f.body()))
}

/// A declaration's name as the pass keys functions: `f`, or `M.f` for a field of a table (methods are not followed).
fn declared(f: &ast::FunctionDeclaration) -> Option<String> {
    let n = f.name();
    let names: Vec<_> = n.names().iter().map(text).collect();
    (n.method_name().is_none() && names.len() <= 2).then(|| names.join("."))
}

fn params(body: &ast::FunctionBody) -> Vec<String> {
    body.parameters()
        .iter()
        .map(|p| match p {
            ast::Parameter::Name(n) => text(n),
            _ => String::new(),
        })
        .collect()
}

/// `return {field = name, ...}`: the fields that are names (of functions, the caller checks). `return M`, M a
/// file-level `local M = {...}` never assigned again: its `function M.f` declarations, by field.
fn exported_functions(ast: &ast::Ast, names: &super::scope::Names) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Some(ast::LastStmt::Return(r)) = ast.nodes().last_stmt() else {
        return out;
    };
    match r.returns().iter().collect::<Vec<_>>()[..] {
        [Expression::TableConstructor(t)] => {
            for f in t.fields() {
                if let ast::Field::NameKey { key, value, .. } = f {
                    if let Expression::Var(ast::Var::Name(n)) = &**value {
                        out.insert(text(key), text(n));
                    }
                }
            }
        }
        [Expression::Var(ast::Var::Name(returned))] => {
            let Some(d) = names.constant(returned).filter(|&d| names.top(d)) else {
                return out;
            };
            let m = text(returned);
            let table = ast.nodes().stmts().any(|s| matches!(s, ast::Stmt::LocalAssignment(l)
                if l.names().len() == 1 && l.names().iter().all(|n| names.of(n) == Some(d))
                    && matches!(l.expressions().iter().collect::<Vec<_>>()[..], [Expression::TableConstructor(_)])));
            if table {
                for s in ast.nodes().stmts() {
                    // The statement's table name, which must read the returned declaration.
                    let (root, name) = match s {
                        ast::Stmt::FunctionDeclaration(f) => {
                            (f.name().names().iter().next(), declared(f))
                        }
                        ast::Stmt::Assignment(a) => (
                            a.variables().iter().next().and_then(|v| match v {
                                ast::Var::Expression(e) => match e.prefix() {
                                    ast::Prefix::Name(n) => Some(n),
                                    _ => None,
                                },
                                _ => None,
                            }),
                            assigned_function(a).map(|(n, _)| n),
                        ),
                        _ => (None, None),
                    };
                    if root.and_then(|r| names.of(r)) != Some(d) {
                        continue;
                    }
                    if let Some((_, field)) = name.as_deref().and_then(|n| n.split_once('.')) {
                        out.insert(field.to_string(), format!("{m}.{field}"));
                    }
                }
            }
        }
        _ => {}
    }
    out
}

struct Flow<'a> {
    program: &'a Program<'a>,
    functions: &'a HashMap<String, HashMap<String, Vec<String>>>,
    exported: &'a HashMap<String, HashMap<String, String>>,
    /// The names of the program's own methods (`function T:m`), which the plan doesn't follow.
    methods: &'a HashSet<String>,
    state: &'a mut State,
    file: &'a str,
    stem: String,
    facts: &'a Facts,
    /// The declarations that are functions (`local function f`, `local f = function`): a call by a name that reads
    /// any other declaration (a parameter named like a function) is not the program's function of that name.
    function_decls: HashSet<usize>,
    /// File-level `local T = {...}` never assigned again: its constructor, which a returned `T` is checked through.
    tables: HashMap<usize, Expression>,
    last: bool,
    /// The name the next function body takes (its parameters can be passed an animation).
    next: Option<String>,
    functions_open: usize,
    /// Calls already judged (a draw's art, a `for` header's ipairs): their spans.
    judged: HashSet<Range<usize>>,
    draws: Vec<Range<usize>>,
    escapes: Vec<Error>,
}

impl<'a> Flow<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        program: &'a Program<'a>,
        functions: &'a HashMap<String, HashMap<String, Vec<String>>>,
        exported: &'a HashMap<String, HashMap<String, String>>,
        methods: &'a HashSet<String>,
        state: &'a mut State,
        file: &'a str,
        ast: &ast::Ast,
        facts: &'a Facts,
        last: bool,
    ) -> Self {
        let stem = file
            .trim_end_matches(".luau")
            .trim_end_matches(".lua")
            .to_string();
        let mut tables = HashMap::new();
        for s in ast.nodes().stmts() {
            if let ast::Stmt::LocalAssignment(l) = s {
                if let ([n], [e @ Expression::TableConstructor(_)]) = (
                    &l.names().iter().collect::<Vec<_>>()[..],
                    &l.expressions().iter().collect::<Vec<_>>()[..],
                ) {
                    if let Some(d) = facts.names.constant(n) {
                        tables.insert(d, (*e).clone());
                    }
                }
            }
        }
        Flow {
            program,
            functions,
            exported,
            methods,
            state,
            file,
            stem,
            facts,
            function_decls: HashSet::new(),
            tables,
            last,
            next: None,
            functions_open: 0,
            judged: HashSet::new(),
            draws: vec![],
            escapes: vec![],
        }
    }

    fn carry(&mut self, d: usize) {
        self.state
            .decls
            .entry(self.file.into())
            .or_default()
            .insert(d);
    }
    fn carries(&self, d: usize) -> bool {
        self.state
            .decls
            .get(self.file)
            .is_some_and(|s| s.contains(&d))
    }
    /// The declaration a name reads, and whether it is a file-level one.
    fn resolve(&self, name: &TokenReference) -> Option<(usize, bool)> {
        self.facts
            .names
            .of(name)
            .map(|d| (d, self.facts.names.top(d)))
    }
    fn escape(&mut self, node: &impl Node) {
        if self.last {
            self.escapes.push(Error {
                file: self.file.into(),
                line: super::line(node),
                message: ESCAPE.into(),
            });
        }
    }

    /// A module's returned art at `field` then `path`: whether an animation lies there (any field for `None`).
    fn exports_animate(&self, stem: &str, field: Option<&str>, path: &[Step]) -> bool {
        let Some(fields) = self.program.exports.get(stem) else {
            return false;
        };
        match field {
            None => fields.values().any(Tree::animates),
            Some(f) => fields.get(f).is_some_and(|t| within(t, path)),
        }
    }

    /// Whether the value of `e` can be (or hold) an animation.
    fn may(&self, e: &Expression) -> bool {
        match e {
            Expression::Parentheses { expression, .. } => self.may(expression),
            Expression::TypeAssertion { expression, .. } => self.may(expression),
            Expression::BinaryOperator { lhs, binop, rhs } => match binop {
                ast::BinOp::And(_) => self.may(rhs),
                // Comparisons and `..` make booleans and strings; arithmetic on an animation's number might still be one.
                ast::BinOp::TwoEqual(_)
                | ast::BinOp::TildeEqual(_)
                | ast::BinOp::LessThan(_)
                | ast::BinOp::LessThanEqual(_)
                | ast::BinOp::GreaterThan(_)
                | ast::BinOp::GreaterThanEqual(_)
                | ast::BinOp::TwoDots(_) => false,
                _ => self.may(lhs) || self.may(rhs),
            },
            Expression::UnaryOperator { unop, expression } => match unop {
                ast::UnOp::Not(_) | ast::UnOp::Hash(_) => false,
                _ => self.may(expression),
            },
            Expression::IfExpression(i) => {
                self.may(i.if_expression())
                    || self.may(i.else_expression())
                    || i.else_if_expressions()
                        .is_some_and(|ei| ei.iter().any(|x| self.may(x.expression())))
            }
            // `require` anywhere: the module's table, holding whatever animations it returns.
            Expression::FunctionCall(c) => {
                is_animation_call(c)
                    || required(c).is_some_and(|m| self.exports_animate(&m, None, &[]))
            }
            Expression::TableConstructor(t) => t.fields().iter().any(|f| match f {
                ast::Field::NameKey { value, .. } | ast::Field::ExpressionKey { value, .. } => {
                    self.may(value)
                }
                ast::Field::NoKey(value) => self.may(value),
                _ => false,
            }),
            Expression::Var(ast::Var::Name(n)) => self.name_may(n, &[]),
            Expression::Var(ast::Var::Expression(v)) => {
                if let Some((m, f)) = required_field(v) {
                    return self.exports_animate(&m, Some(&f), &[]);
                }
                // `(e)[k]`: whatever e can hold.
                let root = match v.prefix() {
                    ast::Prefix::Name(root) => root,
                    ast::Prefix::Expression(e) => return self.may(e),
                    _ => return true,
                };
                let suffixes: Vec<_> = v.suffixes().collect();
                match steps(&suffixes) {
                    Some(path) => self.name_may(root, &path),
                    // `require("./m").A.hop`: what the module returns there.
                    None if text(root) == "require" => {
                        let module = match suffixes.first() {
                            Some(ast::Suffix::Call(ast::Call::AnonymousCall(a))) => match &**a {
                                ast::FunctionArgs::Parentheses { arguments, .. } => {
                                    super::required_path(&arguments.iter().collect::<Vec<_>>())
                                }
                                _ => None,
                            },
                            _ => None,
                        };
                        match (module, steps(&suffixes[1..])) {
                            (Some(m), Some(path)) => match path.split_first() {
                                Some((Step::Key(f), rest)) => {
                                    self.exports_animate(&m, Some(f), rest)
                                }
                                _ => self.exports_animate(&m, None, &[]),
                            },
                            _ => true,
                        }
                    }
                    // A call's result: a function that returns an animation escaped at its return.
                    None => false,
                }
            }
            Expression::Number(_)
            | Expression::String(_)
            | Expression::Symbol(_)
            | Expression::InterpolatedString(_)
            | Expression::Function(_) => false,
            // A shape the pass doesn't model: it may hold one (the draw keeps the machinery, a call escapes).
            _ => true,
        }
    }
    /// Whether a returned value loses an animation from its export tree: leaf by leaf, any leaf that may hold one where
    /// the tree doesn't name it exactly (importers trust the tree, which reads a leaf it can't name as no animation).
    fn lost(&self, e: &Expression, tree: Option<&Tree>) -> bool {
        match (e, tree) {
            (Expression::Parentheses { expression, .. }, _) => self.lost(expression, tree),
            // tree_of builds a constructor's table field for field, in order.
            (Expression::TableConstructor(t), Some(Tree::Table(fields))) => {
                t.fields().iter().zip(fields).any(|(f, (_, sub))| match f {
                    ast::Field::NameKey { value, .. } => self.lost(value, Some(sub)),
                    ast::Field::NoKey(value) => self.lost(value, Some(sub)),
                    ast::Field::ExpressionKey { value, .. } => self.may(value),
                    _ => false,
                })
            }
            (Expression::Var(ast::Var::Name(n)), Some(Tree::Table(_)))
                if self
                    .resolve(n)
                    .is_some_and(|(d, _)| self.tables.contains_key(&d)) =>
            {
                self.lost(&self.tables[&self.resolve(n).unwrap().0], tree)
            }
            (_, Some(t)) if exact(t) => false,
            _ => self.may(e),
        }
    }
    /// Whether `name` then `path` can reach an animation.
    fn name_may(&self, name: &TokenReference, path: &[Step]) -> bool {
        match self.resolve(name) {
            Some((d, false)) => self.carries(d),
            Some((d, true)) => {
                // A module or a field of one (names never assigned again): exactly what lies at the path.
                if let Some(m) = self.facts.requires.get(&d) {
                    return match path.split_first() {
                        Some((Step::Key(f), rest)) => self.exports_animate(m, Some(f), rest),
                        _ => self.exports_animate(m, None, &[]),
                    };
                }
                if let Some((m, f)) = self.facts.module_fields.get(&d) {
                    return self.exports_animate(m, Some(f), path);
                }
                // Art by name, or anything an assignment handed it.
                self.carries(d)
                    || self.facts.art_names.get(&d).is_some_and(|&a| a > ANIMATION)
                    || self
                        .facts
                        .art_tables
                        .get(&d)
                        .is_some_and(|t| within(t, path))
            }
            // A method's `self` is whatever its receiver was.
            None => text(name) == "self",
        }
    }

    /// A call's arguments: a draw's art is followed, an own function's parameter takes the animation, else escape.
    fn call(&mut self, call: &ast::FunctionCall) {
        if self.judged.contains(&span(call)) {
            return;
        }
        let suffixes: Vec<_> = call.suffixes().collect();
        let Some(ast::Suffix::Call(last)) = suffixes.last() else {
            return;
        };
        let a = match last {
            ast::Call::AnonymousCall(a) => a,
            ast::Call::MethodCall(m) => m.args(),
            _ => return,
        };
        // `f{...}` is one argument, the table; `f"..."` one string.
        let table;
        let args: Vec<&Expression> = match a {
            ast::FunctionArgs::Parentheses { arguments, .. } => arguments.iter().collect(),
            ast::FunctionArgs::TableConstructor(t) => {
                table = Expression::TableConstructor(t.clone());
                vec![&table]
            }
            _ => vec![],
        };
        // `t:m(...)`: a method the plan doesn't follow, handed whatever t holds as `self` (a group's `:draw` judges
        // only its art, below), or one of the program's own methods handed an animation at all.
        let own =
            matches!(last, ast::Call::MethodCall(m) if self.methods.contains(&text(m.name())));
        if let ast::Call::MethodCall(m) = last {
            let receiver = match call.prefix() {
                ast::Prefix::Name(n) => {
                    steps(&suffixes[..suffixes.len() - 1]).is_some_and(|p| self.name_may(n, &p))
                }
                ast::Prefix::Expression(e) => self.may(e),
                _ => true,
            };
            if (receiver && (own || text(m.name()) != "draw"))
                || (own && args.iter().any(|a| self.may(a)))
            {
                self.escape(call);
                return;
            }
        }
        let callee = callee(call);
        let draw = (!own && matches!(last, ast::Call::MethodCall(m) if text(m.name()) == "draw"))
            || callee.as_deref() == Some("sprites.draw");
        if draw {
            if args.first().is_some_and(|a| self.may(a)) {
                self.draws.push(span(call));
            }
            return;
        }
        if matches!(callee.as_deref(), Some("sprites.animation" | "sprites.art")) {
            return;
        }
        let carried: Vec<usize> = (0..args.len()).filter(|&i| self.may(args[i])).collect();
        if carried.is_empty() {
            return;
        }
        // The program's own function: by name in this file, or a field a required module returns.
        let root = match call.prefix() {
            ast::Prefix::Name(r) => self.resolve(r),
            _ => None,
        };
        let target = match (&callee, matches!(last, ast::Call::AnonymousCall(_))) {
            (Some(c), true) => match c.split_once('.') {
                None => match root.and_then(|(d, _)| self.facts.module_fields.get(&d)) {
                    Some((m, f)) => self
                        .exported
                        .get(m)
                        .and_then(|e| e.get(f))
                        .map(|n| (m.clone(), n.clone())),
                    None if root.is_some_and(|(d, _)| !self.function_decls.contains(&d)) => None,
                    None => Some((self.stem.clone(), c.clone())),
                },
                Some((_, f)) if !f.contains('.') => {
                    match root.and_then(|(d, _)| self.facts.requires.get(&d)) {
                        Some(s) => self
                            .exported
                            .get(s)
                            .and_then(|e| e.get(f))
                            .map(|n| (s.clone(), n.clone())),
                        // `M.f` of a file-level table here, never assigned again: its `function M.f`.
                        None if root
                            .is_some_and(|(d, top)| top && self.facts.names.decl_constant(d)) =>
                        {
                            Some((self.stem.clone(), c.clone()))
                        }
                        None => None,
                    }
                }
                _ => None,
            },
            _ => None,
        };
        let target =
            target.filter(|(s, n)| self.functions.get(s).is_some_and(|f| f.contains_key(n)));
        match target {
            Some((s, n)) => {
                // A named parameter takes it (`...` is one the plan can't follow).
                let params = &self.functions[&s][&n];
                for i in carried {
                    if params.get(i).is_some_and(|p| !p.is_empty()) {
                        self.state.params.insert((s.clone(), n.clone(), i));
                    } else {
                        self.escape(call);
                    }
                }
            }
            None => self.escape(call),
        }
    }
}

/// A tree with no leaf it couldn't name.
fn exact(t: &Tree) -> bool {
    match t {
        Tree::Art(_) => true,
        Tree::Table(fields) => fields.iter().all(|(_, t)| exact(t)),
        Tree::Other => false,
    }
}

/// Whether an animation lies at `path` in `t` (anywhere under it when the path leaves the tree's known keys).
fn within(t: &Tree, path: &[Step]) -> bool {
    match t.at(path) {
        Some(nodes) => nodes.iter().any(|n| n.animates()),
        None => t.animates(),
    }
}

fn is_animation_call(c: &ast::FunctionCall) -> bool {
    callee(c).as_deref() == Some("sprites.animation")
}

/// A call's callee as a dotted name (`f`, `M.f`, `sprites.draw`), without its arguments.
fn callee(c: &ast::FunctionCall) -> Option<String> {
    let ast::Prefix::Name(root) = c.prefix() else {
        return None;
    };
    let mut name = text(root);
    let suffixes: Vec<_> = c.suffixes().collect();
    for s in &suffixes[..suffixes.len().saturating_sub(1)] {
        match s {
            ast::Suffix::Index(ast::Index::Dot { name: n, .. }) => {
                name = format!("{name}.{}", text(n))
            }
            _ => return None,
        }
    }
    Some(name)
}

impl Visitor for Flow<'_> {
    fn visit_function_body(&mut self, body: &ast::FunctionBody) {
        self.functions_open += 1;
        let name = self.next.take();
        for (i, p) in body.parameters().iter().enumerate() {
            let ast::Parameter::Name(p) = p else { continue };
            let carries = name.as_ref().is_some_and(|n| {
                self.state
                    .params
                    .contains(&(self.stem.clone(), n.clone(), i))
            });
            if let (true, Some((d, _))) = (carries, self.resolve(p)) {
                self.carry(d);
            }
        }
    }
    fn visit_function_body_end(&mut self, _: &ast::FunctionBody) {
        self.functions_open -= 1;
    }
    fn visit_local_function(&mut self, f: &ast::LocalFunction) {
        if let Some((d, _)) = self.resolve(f.name()) {
            self.function_decls.insert(d);
        }
        self.next = Some(text(f.name()));
    }
    fn visit_function_declaration(&mut self, f: &ast::FunctionDeclaration) {
        self.next = declared(f);
        // `local f` then `function f(...)`: the forward-declared local is the function.
        let names: Vec<_> = f.name().names().iter().collect();
        if let ([n], None) = (&names[..], f.name().method_name()) {
            if let Some((d, _)) = self.resolve(n) {
                self.function_decls.insert(d);
            }
        }
    }
    fn visit_local_assignment(&mut self, a: &ast::LocalAssignment) {
        if let ([n], [Expression::Function(_)]) = (
            &a.names().iter().collect::<Vec<_>>()[..],
            &a.expressions().iter().collect::<Vec<_>>()[..],
        ) {
            self.next = Some(text(n));
        }
    }
    fn visit_local_assignment_end(&mut self, a: &ast::LocalAssignment) {
        let exprs: Vec<_> = a.expressions().iter().collect();
        for (i, n) in a.names().iter().enumerate() {
            let carries = exprs.get(i).is_some_and(|e| self.may(e));
            let Some((d, _)) = self.resolve(n) else {
                continue;
            };
            if matches!(exprs.get(i), Some(Expression::Function(_))) {
                self.function_decls.insert(d);
            }
            if carries {
                self.carry(d);
            }
        }
    }
    fn visit_assignment(&mut self, a: &ast::Assignment) {
        if let Some((n, _)) = assigned_function(a) {
            self.next = Some(n);
        }
        let exprs: Vec<_> = a.expressions().iter().collect();
        for (i, v) in a.variables().iter().enumerate() {
            if !exprs.get(i).is_some_and(|e| self.may(e)) {
                continue;
            }
            match v {
                ast::Var::Name(n) => match self.resolve(n) {
                    Some((d, _)) => self.carry(d),
                    None => self.escape(a),
                },
                _ => self.escape(a),
            }
        }
    }
    fn visit_compound_assignment(&mut self, a: &ast::CompoundAssignment) {
        // `a += walk`: arithmetic on an animation's number might still be one, as `may` reads `a + walk`.
        if !self.may(a.rhs()) {
            return;
        }
        match a.lhs() {
            ast::Var::Name(n) => match self.resolve(n) {
                Some((d, _)) => self.carry(d),
                None => self.escape(a),
            },
            _ => self.escape(a),
        }
    }
    fn visit_generic_for(&mut self, f: &ast::GenericFor) {
        // `for k, v in t`, `ipairs(t)`, `pairs(t)`: the loop's names take what t holds.
        let mut carries = false;
        for e in f.expressions().iter() {
            match e {
                Expression::FunctionCall(c)
                    if matches!(callee(c).as_deref(), Some("ipairs" | "pairs")) =>
                {
                    if let Some(ast::Suffix::Call(ast::Call::AnonymousCall(args))) =
                        c.suffixes().last()
                    {
                        match &**args {
                            ast::FunctionArgs::Parentheses { arguments, .. } => {
                                carries |= arguments.iter().any(|a| self.may(a))
                            }
                            ast::FunctionArgs::TableConstructor(t) => {
                                carries |= self.may(&Expression::TableConstructor(t.clone()))
                            }
                            _ => {}
                        }
                    }
                    self.judged.insert(span(c));
                }
                e => carries |= self.may(e),
            }
        }
        if carries {
            for n in f.names() {
                if let Some((d, _)) = self.resolve(n) {
                    self.carry(d);
                }
            }
        }
    }
    fn visit_function_call(&mut self, c: &ast::FunctionCall) {
        self.call(c);
    }
    fn visit_return(&mut self, r: &ast::Return) {
        let returns: Vec<_> = r.returns().iter().collect();
        if self.functions_open > 0 {
            if returns.iter().any(|e| self.may(e)) {
                self.escape(r);
            }
            return;
        }
        // A module's returned table: its fields are followed as trees; a field the tree loses escapes.
        let lost = match returns[..] {
            [Expression::TableConstructor(t)] => t.fields().iter().any(|f| match f {
                ast::Field::NameKey { key, value, .. } => self.lost(
                    value,
                    self.program
                        .exports
                        .get(&self.stem)
                        .and_then(|e| e.get(&text(key))),
                ),
                ast::Field::ExpressionKey { value, .. } => self.may(value),
                ast::Field::NoKey(value) => self.may(value),
                _ => false,
            }),
            _ => returns.iter().any(|e| self.may(e)),
        };
        if lost {
            self.escape(r);
        }
    }
}
