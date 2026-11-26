//! Helpers that cut art, expanded at their calls before the sprite plan reads the program.
//!
//! The plan numbers each `sprites.art`, `sprites.animation` and `sprites.group` call site once, so those calls sit at
//! file level. A helper whose whole body is `return E` (E a call or a table, naming one of those constructors) is
//! replaced at each of its calls by E, its parameters by the call's arguments, when every reference to it is a
//! file-level call with constant arguments (literals, names, `a.b` fields, tables of those): each call then makes its
//! own art, as if the author had spelled it out, and the helper's definition is blanked. The expansion keeps every
//! line, so the plan's lines and the program's errors are the author's. A helper anything else references stays as
//! written, and the plan refuses its constructor as before.
//!
//! A name E reads besides its parameters must mean the same at the call: the same file-level local (a call in the
//! helper's file) or a global. The helpers are `local function f(...)`, called by name in its file or, exported as
//! `return {name = f, ...}`, as `Module.name(...)` in a file that requires it; and, in a module that ends `return M`,
//! `function M.f(...)`. Helpers calling helpers expand over rounds.
use full_moon::{
    ast::{self, Expression},
    node::Node,
    tokenizer::{TokenReference, TokenType},
    visitors::{Visit, Visitor},
};
use std::collections::{HashMap, HashSet};
use std::ops::Range;

use super::{path, required, scope, span};

/// Rounds of expansion: one per level of helpers calling helpers.
const ROUNDS: usize = 8;

/// The program's files with every art helper expanded, or `None` when there is none.
pub fn expand(files: &[(&str, &str)]) -> Option<Vec<(String, String)>> {
    if !files.iter().any(|(_, s)| s.contains("sprites.")) {
        return None;
    }
    let mut out: Vec<(String, String)> = files
        .iter()
        .map(|(f, s)| (f.to_string(), s.to_string()))
        .collect();
    let mut changed = false;
    for _ in 0..ROUNDS {
        match round(&out) {
            Some(next) => {
                out = next;
                changed = true;
            }
            None => break,
        }
    }
    changed.then_some(out)
}

fn stem(file: &str) -> &str {
    file.trim_end_matches(".luau").trim_end_matches(".lua")
}

fn start(t: &TokenReference) -> usize {
    t.token().start_position().bytes()
}

/// A helper: its file, how it is called (the `local function`'s declaration, or `M.f`'s field), and E.
struct Helper {
    file: usize,
    local: Option<usize>,
    field: Option<String>,
    /// The module table's declaration, for a `function M.f`.
    table: Option<usize>,
    /// The definition statement, blanked once every call is expanded.
    def: Range<usize>,
    /// The `f` token of `function M.f`, which is no reference.
    field_at: Option<usize>,
    /// The `f` value of a returned `{name = f}`, which becomes `nil` with the definition.
    export: Option<Range<usize>>,
    /// E's tokens: (trivia before, text, trivia after, the parameter it names).
    tokens: Vec<(bool, String, bool, Option<usize>)>,
    /// E's other names: (name, its declaration in the helper's file).
    free: Vec<(String, Option<usize>)>,
}

/// One parsed file: its syntax, scopes, file-level declarations in scope order (name, where its scope opens, its
/// declaration), and which declarations are `local X = require("./m")`.
struct File<'a> {
    source: &'a str,
    ast: ast::Ast,
    names: scope::Names,
    tops: Vec<(String, usize, Option<usize>)>,
    requires: HashMap<usize, String>,
}

impl File<'_> {
    /// The file-level declaration `name` reads at `at` (a file-level place): none for a global.
    fn at(&self, name: &str, at: usize) -> Option<usize> {
        self.tops
            .iter()
            .rev()
            .find(|(n, open, _)| n == name && *open <= at)
            .and_then(|t| t.2)
    }
}

/// A call to expand: the file, the call's span, its helper and its arguments' text.
struct Site {
    file: usize,
    call: Range<usize>,
    prefix: usize,
    helper: usize,
    args: Vec<String>,
}

fn constant(e: &Expression) -> bool {
    match e {
        Expression::Number(_) | Expression::String(_) | Expression::Symbol(_) => true,
        Expression::UnaryOperator {
            unop: ast::UnOp::Minus(_),
            expression,
        } => matches!(**expression, Expression::Number(_)),
        Expression::Parentheses { expression, .. } => constant(expression),
        Expression::Var(ast::Var::Name(_)) => true,
        Expression::Var(ast::Var::Expression(v)) => {
            matches!(v.prefix(), ast::Prefix::Name(_))
                && v.suffixes()
                    .all(|s| matches!(s, ast::Suffix::Index(ast::Index::Dot { .. })))
        }
        Expression::TableConstructor(t) => t.fields().iter().all(|f| match f {
            ast::Field::NoKey(v) => constant(v),
            ast::Field::NameKey { value, .. } => constant(value),
            ast::Field::ExpressionKey { key, value, .. } => constant(key) && constant(value),
            _ => false,
        }),
        _ => false,
    }
}

/// A node's text on one line: comments and line breaks become spaces, parameter tokens their argument.
fn one_line(tokens: impl Iterator<Item = (bool, String, bool)>) -> String {
    let mut s = String::new();
    for (before, text, after) in tokens {
        if before && !s.is_empty() && !s.ends_with(' ') {
            s.push(' ');
        }
        s.push_str(&text);
        if after {
            s.push(' ');
        }
    }
    s.trim_end().to_string()
}

/// A node's tokens in source order (full_moon yields a bracket pair together).
fn ordered(node: &impl Node) -> Vec<&TokenReference> {
    let mut tokens: Vec<_> = node.tokens().collect();
    tokens.sort_by_key(|t| start(t));
    tokens
}

fn flat(node: &impl Node) -> String {
    one_line(ordered(node).into_iter().map(|t| {
        (
            t.leading_trivia().next().is_some(),
            t.token().to_string(),
            t.trailing_trivia().next().is_some(),
        )
    }))
}

/// An argument's text where its parameter stood: a negative number in parentheses (`a-n` must not become `a--1`);
/// names stay bare, so the plan still reads art bound to them.
fn arg(e: &Expression) -> String {
    match e {
        Expression::UnaryOperator { .. } => format!("({})", flat(e)),
        _ => flat(e),
    }
}

/// The helper a top-level statement defines, if its body is one `return E` naming a sprites constructor.
fn helper(
    file: usize,
    f: &File,
    stmt: &ast::Stmt,
    module: Option<usize>,
    exports: &HashMap<usize, (String, Range<usize>)>,
) -> Option<Helper> {
    let mut export = None;
    let (body, local, field, table, field_at) = match stmt {
        ast::Stmt::LocalFunction(l) => {
            let d = f.names.of(l.name())?;
            let field = exports.get(&d).map(|(name, at)| {
                export = Some(at.clone());
                name.clone()
            });
            (l.body(), Some(d), field, None, None)
        }
        ast::Stmt::FunctionDeclaration(d) => {
            let m = module?;
            let names: Vec<_> = d.name().names().iter().collect();
            let [t, name] = names[..] else { return None };
            if d.name().method_name().is_some() || f.names.of(t) != Some(m) {
                return None;
            }
            (
                d.body(),
                None,
                Some(name.token().to_string()),
                Some(m),
                Some(start(name)),
            )
        }
        _ => return None,
    };
    if !body
        .parameters()
        .iter()
        .all(|p| matches!(p, ast::Parameter::Name(_)))
    {
        return None;
    }
    if body.block().stmts().next().is_some() {
        return None;
    }
    let Some(ast::LastStmt::Return(r)) = body.block().last_stmt() else {
        return None;
    };
    let [e] = r.returns().iter().collect::<Vec<_>>()[..] else {
        return None;
    };
    if !matches!(
        e,
        Expression::FunctionCall(_) | Expression::TableConstructor(_)
    ) {
        return None;
    }
    let text = &f.source[span(e)];
    if !["sprites.art", "sprites.animation", "sprites.group"]
        .iter()
        .any(|c| text.contains(c))
    {
        return None;
    }
    // Name tokens E reads as variables, and whether it opens a function (whose locals would need renaming).
    #[derive(Default)]
    struct Reads(Vec<TokenReference>, bool);
    impl Visitor for Reads {
        fn visit_var(&mut self, v: &ast::Var) {
            if let ast::Var::Name(n) = v {
                self.0.push(n.clone());
            }
        }
        fn visit_prefix(&mut self, p: &ast::Prefix) {
            if let ast::Prefix::Name(n) = p {
                self.0.push(n.clone());
            }
        }
        fn visit_function_body(&mut self, _: &ast::FunctionBody) {
            self.1 = true;
        }
    }
    let mut reads = Reads::default();
    e.visit(&mut reads);
    if reads.1 {
        return None;
    }
    let param_decls: Vec<Option<usize>> = body
        .parameters()
        .iter()
        .map(|p| match p {
            ast::Parameter::Name(n) => f.names.of(n),
            _ => None,
        })
        .collect();
    let mut by_start: HashMap<usize, usize> = HashMap::new();
    let mut free = vec![];
    for t in &reads.0 {
        let d = f.names.of(t);
        match param_decls.iter().position(|p| p.is_some() && *p == d) {
            Some(i) => {
                by_start.insert(start(t), i);
            }
            None => free.push((t.token().to_string(), d)),
        }
    }
    let mut tokens = vec![];
    for t in ordered(e) {
        let s = t.token().to_string();
        // A long string over lines would move the lines after the call (comments are dropped).
        if s.contains('\n') {
            return None;
        }
        tokens.push((
            t.leading_trivia().next().is_some(),
            s,
            t.trailing_trivia().next().is_some(),
            by_start.get(&start(t)).copied(),
        ));
    }
    Some(Helper {
        file,
        local,
        field,
        table,
        def: span(stmt),
        field_at,
        export,
        tokens,
        free,
    })
}

fn round(files: &[(String, String)]) -> Option<Vec<(String, String)>> {
    let parsed: Vec<File> = files
        .iter()
        .map(|(_, s)| {
            let ast = full_moon::parse_fallible(s, full_moon::LuaVersion::luau())
                .into_result()
                .ok()?;
            let names = scope::resolve(&ast);
            let mut tops = vec![];
            let mut requires = HashMap::new();
            for stmt in ast.nodes().stmts() {
                match stmt {
                    ast::Stmt::LocalFunction(l) => tops.push((
                        l.name().token().to_string(),
                        start(l.name()),
                        names.of(l.name()),
                    )),
                    ast::Stmt::LocalAssignment(a) => {
                        let open = span(a).end;
                        let exprs: Vec<_> = a.expressions().iter().collect();
                        for (i, n) in a.names().iter().enumerate() {
                            let d = names.of(n);
                            if let (Some(d), Some(Expression::FunctionCall(c))) = (d, exprs.get(i))
                            {
                                if let Some(m) = required(c) {
                                    requires.insert(d, m);
                                }
                            }
                            tops.push((n.token().to_string(), open, d));
                        }
                    }
                    _ => {}
                }
            }
            Some(File {
                source: s,
                ast,
                names,
                tops,
                requires,
            })
        })
        .collect::<Option<_>>()?;
    // Every helper, by file.
    let mut helpers = vec![];
    for (i, f) in parsed.iter().enumerate() {
        // The module's table (`return M`), or the locals its returned table exports (`return {f = f}`).
        let returned = match f.ast.nodes().last_stmt() {
            Some(ast::LastStmt::Return(r)) => r.returns().iter().collect::<Vec<_>>(),
            _ => vec![],
        };
        let module = match returned[..] {
            [Expression::Var(ast::Var::Name(m))] => f.names.of(m).filter(|&d| f.names.top(d)),
            _ => None,
        };
        let mut exports = HashMap::new();
        if let [Expression::TableConstructor(t)] = returned[..] {
            for field in t.fields() {
                if let ast::Field::NameKey { key, value, .. } = field {
                    if let Expression::Var(ast::Var::Name(v)) = &**value {
                        if let Some(d) = f.names.of(v) {
                            exports.insert(d, (key.token().to_string(), span(v)));
                        }
                    }
                }
            }
        }
        for stmt in f.ast.nodes().stmts() {
            if let Some(h) = helper(i, f, stmt, module, &exports) {
                helpers.push(h);
            }
        }
    }
    if helpers.is_empty() {
        return None;
    }
    // Every file-level call of a helper that can expand.
    let mut sites = vec![];
    for (i, f) in parsed.iter().enumerate() {
        #[derive(Default)]
        struct Calls<'c>(Vec<&'c ast::FunctionCall>);
        impl<'c> Calls<'c> {
            fn walk(&mut self, e: &'c Expression) {
                // Calls outside function bodies only: a visitor cannot lend the nodes out, so walk by hand.
                match e {
                    Expression::FunctionCall(c) => {
                        self.0.push(c);
                        self.suffixes(c.suffixes());
                        if let ast::Prefix::Expression(p) = c.prefix() {
                            self.walk(p);
                        }
                    }
                    Expression::Parentheses { expression, .. } => self.walk(expression),
                    Expression::UnaryOperator { expression, .. } => self.walk(expression),
                    Expression::BinaryOperator { lhs, rhs, .. } => {
                        self.walk(lhs);
                        self.walk(rhs);
                    }
                    Expression::TableConstructor(t) => self.table(t),
                    Expression::Var(ast::Var::Expression(v)) => self.suffixes(v.suffixes()),
                    _ => {}
                }
            }
            fn table(&mut self, t: &'c ast::TableConstructor) {
                for field in t.fields() {
                    match field {
                        ast::Field::NoKey(v) => self.walk(v),
                        ast::Field::NameKey { value, .. } => self.walk(value),
                        ast::Field::ExpressionKey { key, value, .. } => {
                            self.walk(key);
                            self.walk(value);
                        }
                        _ => {}
                    }
                }
            }
            fn suffixes(&mut self, s: impl Iterator<Item = &'c ast::Suffix>) {
                for s in s {
                    match s {
                        ast::Suffix::Call(call) => {
                            let args = match call {
                                ast::Call::AnonymousCall(a) => a.as_ref(),
                                ast::Call::MethodCall(m) => m.args(),
                                _ => continue,
                            };
                            match args {
                                ast::FunctionArgs::Parentheses { arguments, .. } => {
                                    arguments.iter().for_each(|a| self.walk(a))
                                }
                                ast::FunctionArgs::TableConstructor(t) => self.table(t),
                                _ => {}
                            }
                        }
                        ast::Suffix::Index(ast::Index::Brackets { expression, .. }) => {
                            self.walk(expression)
                        }
                        _ => {}
                    }
                }
            }
        }
        let mut calls = Calls::default();
        for stmt in f.ast.nodes().stmts() {
            match stmt {
                ast::Stmt::LocalAssignment(a) => a.expressions().iter().for_each(|e| calls.walk(e)),
                ast::Stmt::Assignment(a) => a.expressions().iter().for_each(|e| calls.walk(e)),
                _ => {}
            }
        }
        if let Some(ast::LastStmt::Return(r)) = f.ast.nodes().last_stmt() {
            r.returns().iter().for_each(|e| calls.walk(e));
        }
        for c in calls.0 {
            let Some((name, args)) = path(c) else {
                continue;
            };
            let ast::Prefix::Name(p) = c.prefix() else {
                continue;
            };
            let d = f.names.of(p);
            let at = start(p);
            let h = helpers
                .iter()
                .position(|h| match (h.local, &h.field, name.split_once('.')) {
                    (Some(l), _, None) => h.file == i && d == Some(l),
                    (_, Some(field), Some((_, n))) if n == field && !n.contains('.') => {
                        (h.file == i && h.table.is_some() && d == h.table)
                            || d.and_then(|d| f.requires.get(&d))
                                .is_some_and(|m| m == stem(&files[h.file].0))
                    }
                    _ => false,
                });
            let Some(k) = h else { continue };
            let h = &helpers[k];
            let hygienic = h.free.iter().all(|(n, decl)| match decl {
                Some(_) => h.file == i && f.at(n, at) == *decl,
                None => f.at(n, at).is_none(),
            });
            if hygienic && args.iter().all(|a| constant(a)) {
                sites.push(Site {
                    file: i,
                    call: span(c),
                    prefix: at,
                    helper: k,
                    args: args.iter().map(|a| arg(a)).collect(),
                });
            }
        }
    }
    // A helper expands only when every reference to it is a site: else it stays, and the plan refuses it as before.
    let site_at: HashSet<(usize, usize)> = sites.iter().map(|s| (s.file, s.prefix)).collect();
    let mut whole = vec![false; helpers.len()];
    for (k, h) in helpers.iter().enumerate() {
        // (file, token start, whether it is a `.f` field): a field's site is keyed by the prefix before the dot.
        let mut refs = vec![];
        for (i, f) in parsed.iter().enumerate() {
            let mut previous_dot = false;
            for t in f.ast.tokens() {
                let ident = matches!(t.token().token_type(), TokenType::Identifier { .. });
                let s = start(t);
                let local = h.local.is_some_and(|l| {
                    ident
                        && i == h.file
                        && f.names.of(t) == Some(l)
                        && !h.def.contains(&s)
                        && !h.export.as_ref().is_some_and(|e| e.contains(&s))
                });
                let field = h.field.as_ref().is_some_and(|field| {
                    ident
                        && previous_dot
                        && t.token().to_string() == *field
                        && Some(s) != h.field_at
                });
                if local || field {
                    refs.push((i, s, field));
                }
                previous_dot = t.token().to_string() == ".";
            }
        }
        let sited = |&(i, s, field): &(usize, usize, bool)| match field {
            false => site_at.contains(&(i, s)),
            true => sites
                .iter()
                .any(|x| x.helper == k && x.file == i && x.call.contains(&s)),
        };
        whole[k] = !refs.is_empty() && refs.iter().all(sited);
    }
    let mut edits: Vec<Vec<(Range<usize>, String)>> = vec![vec![]; files.len()];
    for s in sites.iter().filter(|s| whole[s.helper]) {
        let h = &helpers[s.helper];
        let text = one_line(h.tokens.iter().map(|(before, t, after, p)| {
            let t = match p {
                Some(i) => s.args.get(*i).cloned().unwrap_or_else(|| "nil".into()),
                None => t.clone(),
            };
            (*before, t, *after)
        }));
        edits[s.file].push((s.call.clone(), text));
    }
    for h in helpers
        .iter()
        .zip(&whole)
        .filter(|(_, w)| **w)
        .map(|(h, _)| h)
    {
        let blank: String = files[h.file].1[h.def.clone()]
            .chars()
            .map(|c| if c == '\n' { '\n' } else { ' ' })
            .collect();
        edits[h.file].push((h.def.clone(), blank));
        if let Some(e) = &h.export {
            edits[h.file].push((e.clone(), "nil".into()));
        }
    }
    if edits.iter().all(|e| e.is_empty()) {
        return None;
    }
    Some(
        files
            .iter()
            .zip(edits)
            .map(|((f, s), mut e)| {
                e.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
                let mut out = s.clone();
                for (r, t) in e {
                    out.replace_range(r, &t);
                }
                (f.clone(), out)
            })
            .collect(),
    )
}
