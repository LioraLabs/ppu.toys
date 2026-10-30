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
                instrument: preview && pass_index == 1 && !fixed,
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
                ]);
            } else {
                args.extend([simple(ast::SimpleExpression::Nil), integer(0), integer(0)]);
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

pub(crate) fn install(ctx: piccolo::Context<'_>) {
    use piccolo::{Callback, CallbackReturn, Value};
    let assign = Callback::from_fn(&ctx, |_, _, mut stack| {
        let file = stack.get(0).to_string();
        let line = stack.get(1).to_integer().unwrap_or(0) as u32;
        let count = stack.get(2).to_integer().unwrap_or(0) as usize;
        let values = (0..count)
            .map(|i| stack.get(3 + count * 3 + i))
            .collect::<Vec<_>>();
        for (i, &value) in values.iter().enumerate() {
            let binding = stack.get(3 + i * 3);
            if binding.is_nil() {
                continue;
            }
            let min = stack.get(4 + i * 3).to_integer().unwrap();
            let max = stack.get(5 + i * 3).to_integer().unwrap();
            let n = matches!(value, Value::Integer(_) | Value::Number(_))
                .then(|| value.to_integer())
                .flatten();
            if !n.is_some_and(|n| n >= min && n <= max) {
                return Err(crate::precision::Failure {file,line,message:format!("makeBounded binding {binding} requires an integer in {min}..{max}; got {value}")}.into());
            }
        }
        stack.clear();
        for v in values {
            stack.push_back(v);
        }
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("__ppu_bounded_assign", assign).unwrap();
}
