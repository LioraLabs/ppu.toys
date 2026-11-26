//! Which declaration each name in a file reads, as Luau scopes it: file-level and block locals, `local function`s,
//! parameters and loop names. The sprite plan and the flow pass both key what they know by declaration, so a name that
//! shadows, outlives or reuses another is never read as it.
use full_moon::ast;
use full_moon::tokenizer::TokenReference;
use full_moon::visitors::Visitor;
use std::collections::{HashMap, HashSet};

/// A declaration: made at file level, and assigned after it is declared (a plain or compound assignment, or a
/// `function name()` statement): one never assigned keeps its initialiser's value.
pub(super) struct Decl {
    pub top: bool,
    pub assigned: bool,
}

/// Every name token (a declaration's own, or a use) by its start, to its declaration; a global has none.
#[derive(Default)]
pub(super) struct Names {
    at: HashMap<usize, usize>,
    decls: Vec<Decl>,
}

impl Names {
    pub fn of(&self, t: &TokenReference) -> Option<usize> {
        self.at.get(&t.token().start_position().bytes()).copied()
    }
    pub fn top(&self, d: usize) -> bool {
        self.decls[d].top
    }
    pub fn decl_constant(&self, d: usize) -> bool {
        !self.decls[d].assigned
    }
    /// The declaration `t` reads, when it is never assigned after its initialiser.
    pub fn constant(&self, t: &TokenReference) -> Option<usize> {
        self.of(t).filter(|&d| self.decl_constant(d))
    }
}

pub(super) fn resolve(ast: &ast::Ast) -> Names {
    let mut r = Resolve::default();
    r.visit_ast(ast);
    r.names
}

#[derive(Default)]
struct Resolve {
    names: Names,
    scopes: Vec<HashMap<String, usize>>,
    /// Names the next block opens with (a function's parameters, a loop's names).
    pending: Vec<TokenReference>,
    /// `repeat` bodies, whose locals the `until` condition still reads: their scope closes with the repeat.
    repeats: HashSet<*const ast::Block>,
}

impl Resolve {
    fn declare(&mut self, t: &TokenReference) {
        let d = self.names.decls.len();
        self.names.decls.push(Decl {
            top: self.scopes.len() == 1,
            assigned: false,
        });
        self.names.at.insert(t.token().start_position().bytes(), d);
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(t.token().to_string(), d);
        }
    }
    fn use_(&mut self, t: &TokenReference) -> Option<usize> {
        let name = t.token().to_string();
        let d = self
            .scopes
            .iter()
            .rev()
            .find_map(|s| s.get(&name).copied())?;
        self.names.at.insert(t.token().start_position().bytes(), d);
        Some(d)
    }
    fn assign(&mut self, t: &TokenReference) {
        if let Some(d) = self.use_(t) {
            self.names.decls[d].assigned = true;
        }
    }
}

impl Visitor for Resolve {
    fn visit_block(&mut self, _: &ast::Block) {
        self.scopes.push(HashMap::new());
        for t in std::mem::take(&mut self.pending) {
            self.declare(&t);
        }
    }
    fn visit_block_end(&mut self, b: &ast::Block) {
        if !self.repeats.contains(&(b as *const _)) {
            self.scopes.pop();
        }
    }
    fn visit_repeat(&mut self, r: &ast::Repeat) {
        self.repeats.insert(r.block() as *const _);
    }
    fn visit_repeat_end(&mut self, _: &ast::Repeat) {
        self.scopes.pop();
    }
    fn visit_function_body(&mut self, body: &ast::FunctionBody) {
        for p in body.parameters() {
            if let ast::Parameter::Name(n) = p {
                self.pending.push(n.clone());
            }
        }
    }
    fn visit_numeric_for(&mut self, f: &ast::NumericFor) {
        self.pending.push(f.index_variable().clone());
    }
    fn visit_generic_for(&mut self, f: &ast::GenericFor) {
        self.pending.extend(f.names().iter().cloned());
    }
    // `local function f` is in scope in its own body; `local x = e` only after e.
    fn visit_local_function(&mut self, f: &ast::LocalFunction) {
        self.declare(f.name());
    }
    fn visit_local_assignment_end(&mut self, a: &ast::LocalAssignment) {
        for n in a.names() {
            self.declare(n);
        }
    }
    fn visit_function_declaration(&mut self, f: &ast::FunctionDeclaration) {
        let n = f.name();
        let names: Vec<_> = n.names().iter().collect();
        match names[..] {
            [one] if n.method_name().is_none() => self.assign(one),
            [first, ..] => {
                self.use_(first);
            }
            [] => {}
        }
    }
    fn visit_assignment(&mut self, a: &ast::Assignment) {
        for v in a.variables() {
            if let ast::Var::Name(n) = v {
                self.assign(n);
            }
        }
    }
    fn visit_compound_assignment(&mut self, a: &ast::CompoundAssignment) {
        if let ast::Var::Name(n) = a.lhs() {
            self.assign(n);
        }
    }
    fn visit_var(&mut self, v: &ast::Var) {
        if let ast::Var::Name(n) = v {
            self.use_(n);
        }
    }
    fn visit_prefix(&mut self, p: &ast::Prefix) {
        if let ast::Prefix::Name(n) = p {
            self.use_(n);
        }
    }
}
