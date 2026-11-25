//! Numeric constructors declare ordinary bindings with checked stores.
use crate::precision::PreviewError;
use piccolo::compiler::{self, interning::BasicInterner, parser as ast};
use std::{collections::HashMap, rc::Rc};

type Text = Rc<[u8]>;
type Expr = ast::Expression<Text>;
type Scope = HashMap<String, Option<Declaration>>;
const HELP: &str = "Use makeBounded(initial, minimum, maximum) for integers or makeFixed(initial, minimum, maximum[, denominator]) for Q10 arithmetic and binary storage accuracy. Bounds are ordered integer literals; the denominator is a power of two from 1 through 1024 (default 1024). Later stores are checked without clamping. Declare each new contract in a separate binding.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declaration {
    pub file: String,
    pub line: u64,
    pub name: String,
    pub min: i32,
    pub max: i32,
    pub fixed: bool,
    pub denominator: Option<u16>,
}
impl Declaration {
    pub fn annotation(&self) -> crate::precision::Annotation {
        crate::precision::Annotation {
            line: self.line,
            comment: (
                self.line,
                format!("constructor {} {} {}", self.name, self.min, self.max),
            ),
            name: self.name.clone(),
            min: self.min,
            max: self.max,
            capacity: None,
            fixed: self.fixed,
            precision: self.denominator,
        }
    }
}

pub struct Prepared {
    pub chunks: Vec<(String, ast::Chunk<Text>)>,
    pub declarations: Vec<Declaration>,
}

/// Strip constructors for native analysis, or instrument their stores for preview.
/// Both paths validate exactly the same declarations and lexical scopes.
pub fn prepare(
    files: &[(&str, &str)],
    preview: bool,
) -> Result<Option<Prepared>, (String, PreviewError)> {
    prepare_with(files, preview, false)
}

/// `fixed_bindings`: the program runs makeFixed's law without canonical precision (the
/// Luau surface), so fixed bindings are instrumented too, rounding each store to its step.
pub fn prepare_with(
    files: &[(&str, &str)],
    preview: bool,
    fixed_bindings: bool,
) -> Result<Option<Prepared>, (String, PreviewError)> {
    if !files
        .iter()
        .any(|(_, s)| s.contains("makeBounded") || s.contains("makeFixed"))
    {
        return Ok(None);
    }
    let mut chunks = Vec::new();
    for &(file, source) in files {
        let chunk =
            compiler::parse_chunk(source.as_bytes(), BasicInterner::default()).map_err(|e| {
                (
                    file.into(),
                    PreviewError {
                        line: e.line_number.0 + 1,
                        message: e.to_string(),
                        help: "Fix the Lua syntax at this location.".into(),
                    },
                )
            })?;
        chunks.push((file.to_string(), chunk));
    }
    // Helpers may assign a global whose declaration appears in a later file.
    let mut globals = Scope::new();
    let mut declarations = Vec::new();
    let mut rewritten = Vec::new();
    let mut fixed = false;
    for pass_index in 0..2 {
        let mut pass = chunks.clone();
        declarations.clear();
        for (file, chunk) in &mut pass {
            let mut walk = Walk {
                file,
                line: 1,
                scopes: vec![globals.clone(), Scope::new()],
                declarations: &mut declarations,
                depth: 0,
                instrument: preview && pass_index == 1 && (!fixed || fixed_bindings),
            };
            walk.block(&mut chunk.block, false)
                .map_err(|e| (file.clone(), e))?;
            globals = walk.scopes.remove(0);
        }
        if pass_index == 0 {
            fixed = declarations.iter().any(|d| d.fixed);
            for (name, is_fixed) in [("makeBounded", false), ("makeFixed", true)] {
                if let Some(d) = declarations
                    .iter()
                    .find(|d| d.fixed == is_fixed && globals.contains_key(name))
                {
                    return Err((d.file.clone(), PreviewError {line:d.line,message:format!("A program using the built-in {name} cannot replace its global function."),help:"Rename the replacement helper or use a local function with a separate name.".into()}));
                }
            }
        }
        rewritten = pass;
    }
    if declarations.is_empty() {
        return Ok(None);
    }
    for &(file, source) in files {
        let mut lexer = compiler::lexer::Lexer::new(source.as_bytes(), BasicInterner::default());
        while let Ok(Some(token)) = lexer.read_token() {
            if matches!(token, compiler::lexer::Token::Name(n) if n.starts_with(b"__ppu_bounded_"))
            {
                return Err((
                    file.into(),
                    PreviewError {
                        line: lexer.line_number().0 + 1,
                        message: "Identifiers beginning __ppu_bounded_ are reserved.".into(),
                        help: "Rename this identifier.".into(),
                    },
                ));
            }
        }
    }
    Ok(Some(Prepared {
        chunks: rewritten,
        declarations,
    }))
}

fn text(s: &Text) -> String {
    String::from_utf8_lossy(s).into_owned()
}
fn simple(e: ast::SimpleExpression<Text>) -> Expr {
    Expr {
        head: Box::new(ast::HeadExpression::Simple(e)),
        tail: vec![],
    }
}
fn integer(n: i64) -> Expr {
    simple(ast::SimpleExpression::Integer(n))
}
fn string(s: &str) -> Expr {
    simple(ast::SimpleExpression::String(Rc::from(s.as_bytes())))
}
fn literal(e: &Expr) -> Option<i32> {
    if !e.tail.is_empty() {
        return None;
    }
    match e.head.as_ref() {
        ast::HeadExpression::Simple(ast::SimpleExpression::Integer(n)) => i32::try_from(*n).ok(),
        ast::HeadExpression::UnaryOperator(ast::UnaryOperator::Minus, e) => {
            if e.tail.is_empty() {
                if let ast::HeadExpression::Simple(ast::SimpleExpression::Integer(n)) =
                    e.head.as_ref()
                {
                    return i32::try_from(n.checked_neg()?).ok();
                }
            }
            None
        }
        ast::HeadExpression::Simple(ast::SimpleExpression::Suffixed(s))
            if s.suffixes.is_empty() =>
        {
            if let ast::PrimaryExpression::GroupedExpression(e) = &s.primary {
                literal(e)
            } else {
                None
            }
        }
        _ => None,
    }
}

struct Walk<'a> {
    file: &'a str,
    line: u64,
    scopes: Vec<Scope>,
    declarations: &'a mut Vec<Declaration>,
    depth: usize,
    instrument: bool,
}
impl Walk<'_> {
    fn error(&self, message: &str) -> PreviewError {
        PreviewError {
            line: self.line,
            message: message.into(),
            help: HELP.into(),
        }
    }
    fn binding(&self, n: &str) -> Option<Option<Declaration>> {
        self.scopes.iter().rev().find_map(|s| s.get(n).cloned())
    }
    fn builtin(&self, name: &str) -> bool {
        matches!(name, "makeBounded" | "makeFixed") && self.binding(name).is_none()
    }
    fn block(&mut self, b: &mut ast::Block<Text>, scoped: bool) -> Result<(), PreviewError> {
        if scoped {
            self.scopes.push(Scope::new());
        }
        for s in &mut b.statements {
            self.line = s.line_number.0 + 1;
            self.statement(&mut s.inner)?;
        }
        if let Some(r) = &mut b.return_statement {
            self.line = r.line_number.0 + 1;
            for e in &mut r.inner.returns {
                self.expr(e)?;
            }
        }
        if scoped {
            self.scopes.pop();
        }
        Ok(())
    }
    fn function(
        &mut self,
        f: &mut ast::FunctionDefinition<Text>,
        method: bool,
    ) -> Result<(), PreviewError> {
        let line = self.line;
        self.scopes
            .push(f.parameters.iter().map(|n| (text(n), None)).collect());
        if method {
            self.scopes.last_mut().unwrap().insert("self".into(), None);
        }
        self.depth += 1;
        self.block(&mut f.body, false)?;
        self.depth -= 1;
        self.scopes.pop();
        self.line = line;
        Ok(())
    }
    fn constructor(
        &mut self,
        e: &mut Expr,
        n: &str,
        local: bool,
    ) -> Result<Option<Declaration>, PreviewError> {
        if !e.tail.is_empty() {
            return Ok(None);
        }
        let ast::HeadExpression::Simple(ast::SimpleExpression::Suffixed(s)) = e.head.as_mut()
        else {
            return Ok(None);
        };
        let ast::PrimaryExpression::Name(callee) = &s.primary else {
            return Ok(None);
        };
        let callee = text(callee);
        if !self.builtin(&callee) {
            return Ok(None);
        }
        let [ast::SuffixPart::Call(ast::CallSuffix::Function(args))] = s.suffixes.as_slice() else {
            return Err(self.error(&format!(
                "{callee} must directly initialize a named binding."
            )));
        };
        let fixed = callee == "makeFixed";
        let (initial, min, max, denominator) = match args.as_slice() {
            [initial, min, max] => (initial, min, max, fixed.then_some(1024)),
            [initial, min, max, step] if fixed => {
                let denominator = literal(step)
                    .and_then(|n| u16::try_from(n).ok())
                    .filter(|n| n.is_power_of_two() && *n <= 1024)
                    .ok_or_else(|| {
                        self.error(
                            "makeFixed denominator must be a power of two from 1 through 1024.",
                        )
                    })?;
                (initial, min, max, Some(denominator))
            }
            _ => {
                return Err(self.error(&format!(
                    "{callee} needs an initial value and two integer bounds{}.",
                    if fixed {
                        ", optionally followed by a binary denominator"
                    } else {
                        ""
                    }
                )))
            }
        };
        let (Some(min), Some(max)) = (literal(min), literal(max)) else {
            return Err(self.error(&format!(
                "{callee} bounds must be signed 32-bit integer literals."
            )));
        };
        if min > max {
            return Err(self.error(&format!("{callee} minimum exceeds its maximum.")));
        }
        if fixed && (min.checked_mul(1024).is_none() || max.checked_mul(1024).is_none()) {
            return Err(self.error("makeFixed bounds exceed Q21.10."));
        }
        let d = Declaration {
            file: self.file.into(),
            line: self.line,
            name: n.into(),
            min,
            max,
            fixed,
            denominator,
        };
        if !local
            && (self.depth != 0
                || self.scopes.len() != 2
                || self.binding(n).is_some_and(|old| old.as_ref() != Some(&d)))
        {
            return Err(self.error(&format!("{callee} initializes a new binding; declare globals unconditionally at file level and use a new local elsewhere.")));
        }
        if self
            .declarations
            .iter()
            .any(|old| old.file == d.file && old.line == d.line && old.name == d.name)
        {
            return Err(self.error("Put same-named bounded declarations on separate source lines."));
        }
        // A constructor returns exactly one value even when initial is a multi-return call.
        *e = simple(ast::SimpleExpression::Suffixed(ast::SuffixedExpression {
            primary: ast::PrimaryExpression::GroupedExpression(initial.clone()),
            suffixes: vec![],
        }));
        self.declarations.push(d.clone());
        Ok(Some(d))
    }
    fn stores(&self, values: &mut Vec<Expr>, contracts: &[Option<Declaration>]) {
        if !self.instrument || !contracts.iter().any(Option::is_some) {
            return;
        }
        let mut args = vec![
            string(self.file),
            integer(self.line as i64),
            integer(contracts.len() as i64),
        ];
        for d in contracts {
            if let Some(d) = d {
                args.extend([
                    string(&d.name),
                    integer(d.min as i64),
                    integer(d.max as i64),
                    integer(if d.fixed {
                        d.denominator.unwrap_or(1024) as i64
                    } else {
                        0
                    }),
                ]);
            } else {
                args.extend([
                    simple(ast::SimpleExpression::Nil),
                    integer(0),
                    integer(0),
                    integer(0),
                ]);
            }
        }
        args.append(values);
        values.push(simple(ast::SimpleExpression::Suffixed(
            ast::SuffixedExpression {
                primary: ast::PrimaryExpression::Name(Rc::from(b"__ppu_bounded_assign".as_slice())),
                suffixes: vec![ast::SuffixPart::Call(ast::CallSuffix::Function(args))],
            },
        )));
    }
    fn statement(&mut self, s: &mut ast::Statement<Text>) -> Result<(), PreviewError> {
        use ast::Statement as S;
        match s {
            S::LocalStatement(s) => {
                let mut contracts = vec![];
                for (i, n) in s.names.iter().enumerate() {
                    contracts.push(match s.values.get_mut(i) {
                        Some(e) => self.constructor(e, &text(n), true)?,
                        None => None,
                    });
                }
                for e in &mut s.values {
                    self.expr(e)?;
                }
                self.stores(&mut s.values, &contracts);
                self.scopes
                    .last_mut()
                    .unwrap()
                    .extend(s.names.iter().zip(contracts).map(|(n, d)| (text(n), d)));
            }
            S::Assignment(s) => {
                let mut contracts = vec![];
                for (i, t) in s.targets.iter_mut().enumerate() {
                    let d = match t {
                        ast::AssignmentTarget::Name(n) => {
                            let n = text(n);
                            let made = match s.values.get_mut(i) {
                                Some(e) => self.constructor(e, &n, false)?,
                                None => None,
                            };
                            if let Some(d) = made {
                                self.scopes[0].insert(n.clone(), Some(d));
                            }
                            let old = self.binding(&n);
                            if old.is_none() && self.depth == 0 {
                                self.scopes[0].insert(n, None);
                            }
                            old.flatten()
                        }
                        ast::AssignmentTarget::Field(head, field) => {
                            self.suffixed(head)?;
                            self.field(field)?;
                            None
                        }
                    };
                    contracts.push(d);
                }
                for e in &mut s.values {
                    self.expr(e)?;
                }
                self.stores(&mut s.values, &contracts);
            }
            S::Function(s) => {
                if s.fields.is_empty() && s.method.is_none() {
                    if self.binding(&text(&s.name)).flatten().is_some() {
                        return Err(
                            self.error("A bounded integer cannot be replaced by a function.")
                        );
                    }
                    if self.binding(&text(&s.name)).is_none() {
                        self.scopes[0].insert(text(&s.name), None);
                    }
                }
                self.function(&mut s.definition, s.method.is_some())?;
            }
            S::LocalFunction(s) => {
                self.scopes.last_mut().unwrap().insert(text(&s.name), None);
                self.function(&mut s.definition, false)?;
            }
            S::Do(b) => self.block(b, true)?,
            S::If(s) => {
                self.expr(&mut s.if_part.0)?;
                self.block(&mut s.if_part.1, true)?;
                for (e, b) in &mut s.else_if_parts {
                    self.expr(e)?;
                    self.block(b, true)?;
                }
                if let Some(b) = &mut s.else_part {
                    self.block(b, true)?;
                }
            }
            S::While(s) => {
                self.expr(&mut s.condition)?;
                self.block(&mut s.block, true)?;
            }
            S::Repeat(s) => {
                self.scopes.push(Scope::new());
                self.block(&mut s.body, false)?;
                self.expr(&mut s.until)?;
                self.scopes.pop();
            }
            S::For(ast::ForStatement::Numeric {
                name,
                initial,
                limit,
                step,
                body,
            }) => {
                self.expr(initial)?;
                self.expr(limit)?;
                if let Some(e) = step {
                    self.expr(e)?;
                }
                self.scopes.push(HashMap::from([(text(name), None)]));
                self.block(body, false)?;
                self.scopes.pop();
            }
            S::For(ast::ForStatement::Generic {
                names,
                arguments,
                body,
            }) => {
                for e in arguments {
                    self.expr(e)?;
                }
                self.scopes
                    .push(names.iter().map(|n| (text(n), None)).collect());
                self.block(body, false)?;
                self.scopes.pop();
            }
            S::FunctionCall(s) => {
                self.suffixed(&mut s.head)?;
                self.call(&mut s.call)?;
            }
            S::Label(_) | S::Break | S::Goto(_) => {}
        }
        Ok(())
    }
    fn field(&mut self, f: &mut ast::FieldSuffix<Text>) -> Result<(), PreviewError> {
        if let ast::FieldSuffix::Indexed(e) = f {
            self.expr(e)?;
        }
        Ok(())
    }
    fn call(&mut self, c: &mut ast::CallSuffix<Text>) -> Result<(), PreviewError> {
        let (ast::CallSuffix::Function(args) | ast::CallSuffix::Method(_, args)) = c;
        for e in args {
            self.expr(e)?;
        }
        Ok(())
    }
    fn suffixed(&mut self, s: &mut ast::SuffixedExpression<Text>) -> Result<(), PreviewError> {
        match &mut s.primary {
            ast::PrimaryExpression::Name(n) if self.builtin(&text(n))=>return Err(self.error(&format!("{} must directly initialize a named binding; it cannot be aliased or nested inside another expression.",text(n)))),
            ast::PrimaryExpression::GroupedExpression(e)=>self.expr(e)?,
            _=>{},
        }
        for s in &mut s.suffixes {
            match s {
                ast::SuffixPart::Field(f) => self.field(f)?,
                ast::SuffixPart::Call(c) => self.call(c)?,
            }
        }
        Ok(())
    }
    fn expr(&mut self, e: &mut Expr) -> Result<(), PreviewError> {
        use ast::SimpleExpression as S;
        match e.head.as_mut() {
            ast::HeadExpression::UnaryOperator(_, e) => self.expr(e)?,
            ast::HeadExpression::Simple(s) => match s {
                S::Function(f) => self.function(f, false)?,
                S::Suffixed(s) => self.suffixed(s)?,
                S::TableConstructor(t) => {
                    for f in &mut t.fields {
                        match f {
                            ast::ConstructorField::Array(e) => self.expr(e)?,
                            ast::ConstructorField::Record(k, e) => {
                                if let ast::RecordKey::Indexed(i) = k {
                                    self.expr(i)?;
                                }
                                self.expr(e)?;
                            }
                        }
                    }
                }
                _ => {}
            },
        }
        for (_, e) in &mut e.tail {
            self.expr(e)?;
        }
        Ok(())
    }
}

/// A store that broke its makeBounded or makeFixed contract. The preview keeps running and
/// keeps the value as written; the cartridge stops on it.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Violation {
    /// The binding or record field.
    pub store: String,
    pub value: f64,
    pub min: i32,
    pub max: i32,
    pub file: String,
    pub line: u32,
    /// The running scene's file (a program without scenes: its main file).
    pub scene: String,
    /// The video frame of the update that stored it; `None` during a load (file level, `init`).
    pub frame: Option<u32>,
    /// Stores that broke it at this file:line since the first.
    pub count: u32,
}

/// Where the VM's contract checks put what they find; the engine stamps scene and frame.
pub(crate) type Violations = Rc<std::cell::RefCell<Vec<Violation>>>;

/// One store into a contract: the value kept (makeFixed's on its grid) and whether it is out of
/// bounds. `None`: not a number, which no cartridge store can hold (the run cannot go on).
fn check<'gc>(
    value: piccolo::Value<'gc>,
    min: i64,
    max: i64,
    denominator: i64,
) -> Option<(piccolo::Value<'gc>, bool)> {
    use piccolo::Value;
    let n = match value {
        Value::Integer(i) => i as f64,
        Value::Number(n) => n,
        _ => return None,
    };
    if denominator > 0 {
        // makeFixed's store: Q10 (nearest), then down to the step; bounds check the result.
        let raw = (n * 1024.).round() as i64;
        let v = (raw >> (1024 / denominator).trailing_zeros()) as f64 / denominator as f64;
        return Some((Value::Number(v), !(v >= min as f64 && v <= max as f64)));
    }
    Some((
        value,
        !(n.fract() == 0. && n >= min as f64 && n <= max as f64),
    ))
}

fn note(
    sink: &Violations,
    store: String,
    value: piccolo::Value<'_>,
    min: i64,
    max: i64,
    file: String,
    line: u32,
) {
    let value = value.to_number().unwrap_or(f64::NAN);
    sink.borrow_mut().push(Violation {
        store,
        value,
        min: min as i32,
        max: max as i32,
        file,
        line,
        scene: String::new(),
        frame: None,
        count: 1,
    });
}

/// The Lua line that called the running callback (a metamethod's: the store that triggered it).
// shortcut: a saveData field store goes through luau.lua's own proxy, so it reports that line, not the
// author's; make saveData's __newindex a host callback if authors break save contracts in practice.
fn caller(exec: &piccolo::Execution<'_, '_>) -> (String, u32) {
    exec.upper_lua_frame().map_or((String::new(), 0), |f| {
        (
            String::from_utf8_lossy(f.chunk_name.as_bytes()).into_owned(),
            f.current_line.0 as u32 + 1,
        )
    })
}

/// Luau record fields (`__ppu_record`, written by the frontend): the contract checks are host
/// callbacks so a broken store is reported at the line that made it. luau.lua builds the proxy.
pub(crate) fn install_records(ctx: piccolo::Context<'_>, sink: Violations) {
    use piccolo::{Callback, CallbackReturn, Error, IntoValue, Value};
    fn field<'gc>(
        ctx: piccolo::Context<'gc>,
        sink: &Violations,
        at: &(String, u32),
        name: Value<'gc>,
        contract: Value<'gc>,
        value: Value<'gc>,
    ) -> Result<Value<'gc>, Error<'gc>> {
        let Value::Table(c) = contract else {
            return Ok(value);
        };
        let fixed = c.get(ctx, 1).to_bool();
        let (min, max) = (
            c.get(ctx, 2).to_integer().unwrap_or(0),
            c.get(ctx, 3).to_integer().unwrap_or(0),
        );
        let denominator = if fixed {
            c.get(ctx, 4).to_integer().unwrap_or(1024)
        } else {
            0
        };
        let kind = if fixed { "makeFixed" } else { "makeBounded" };
        let Some((stored, broken)) = check(value, min, max, denominator) else {
            return Err(crate::precision::Failure {
                file: at.0.clone(),
                line: at.1,
                message: format!("{kind} field {name} requires a number; got {value}"),
            }
            .into());
        };
        if broken {
            note(sink, name.to_string(), stored, min, max, at.0.clone(), at.1);
        }
        Ok(stored)
    }
    let s = sink.clone();
    let record = Callback::from_fn(&ctx, move |ctx, exec, mut stack| {
        let (Value::Table(values), Value::Table(contracts)) = (stack.get(0), stack.get(1)) else {
            return Err("__ppu_record needs a values table and a contracts table"
                .into_value(ctx)
                .into());
        };
        let at = caller(&exec);
        for (name, contract) in contracts.iter() {
            let v = field(ctx, &s, &at, name, contract, values.get(ctx, name))?;
            values.set(ctx, name, v)?;
        }
        let Value::Function(proxy) = ctx.get_global("__ppu_record_proxy") else {
            return Err("makeFixed/makeBounded records need __ppu_record_proxy, which the engine owns: the program replaced it"
                .into_value(ctx)
                .into());
        };
        stack.replace(ctx, (values, contracts));
        Ok(CallbackReturn::Call {
            function: proxy,
            then: None,
        })
    });
    let store = Callback::from_fn(&ctx, move |ctx, exec, mut stack| {
        let (proxy, name, value) = (stack.get(0), stack.get(1), stack.get(2));
        let mt = match proxy {
            Value::Table(t) => t.metatable(),
            _ => None,
        };
        let Some((Value::Table(values), Value::Table(contracts))) =
            mt.map(|mt| (mt.get(ctx, "__ppu_values"), mt.get(ctx, "__ppu_contracts")))
        else {
            return Err("__ppu_record_store only stores into a record proxy"
                .into_value(ctx)
                .into());
        };
        let v = field(
            ctx,
            &sink,
            &caller(&exec),
            name,
            contracts.get(ctx, name),
            value,
        )?;
        values.set(ctx, name, v)?;
        stack.clear();
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("__ppu_record", record).unwrap();
    ctx.set_global("__ppu_record_store", store).unwrap();
}

pub(crate) fn install(ctx: piccolo::Context<'_>, sink: Violations) {
    use piccolo::{Callback, CallbackReturn};
    let assign = Callback::from_fn(&ctx, move |_, _, mut stack| {
        let file = stack.get(0).to_string();
        let line = stack.get(1).to_integer().unwrap_or(0) as u32;
        let count = stack.get(2).to_integer().unwrap_or(0) as usize;
        let mut values = (0..count)
            .map(|i| stack.get(3 + count * 4 + i))
            .collect::<Vec<_>>();
        for (i, value) in values.iter_mut().enumerate() {
            let binding = stack.get(3 + i * 4);
            if binding.is_nil() {
                continue;
            }
            let min = stack.get(4 + i * 4).to_integer().unwrap();
            let max = stack.get(5 + i * 4).to_integer().unwrap();
            let denominator = stack.get(6 + i * 4).to_integer().unwrap();
            let Some((stored, broken)) = check(*value, min, max, denominator) else {
                let (kind, what) = if denominator > 0 {
                    ("makeFixed", "a value")
                } else {
                    ("makeBounded", "an integer")
                };
                return Err(crate::precision::Failure {
                    file,
                    line,
                    message: format!(
                        "{kind} binding {binding} requires {what} in {min}..{max}; got {value}"
                    ),
                }
                .into());
            };
            if broken {
                note(
                    &sink,
                    binding.to_string(),
                    stored,
                    min,
                    max,
                    file.clone(),
                    line,
                );
            }
            *value = stored;
        }
        stack.clear();
        for v in values {
            stack.push_back(v);
        }
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("__ppu_bounded_assign", assign).unwrap();
}
