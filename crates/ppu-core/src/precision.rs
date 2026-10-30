//! Canonical `snes fixed` storage law. Expressions use Q10; only stores choose
//! a binary denominator, 1..=1024. Bounds check the rounded stored value.

/// Parse the exact `1/N` step spelling (binary N, no finer than Q10).
pub fn denominator(step: &str) -> Option<u16> {
    let digits = step.strip_prefix("1/")?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: u16 = digits.parse().ok()?;
    (n.is_power_of_two() && n <= 1024).then_some(n)
}

/// Encode a Q10 expression in storage units, rounding nearest, ties away from
/// zero. The caller supplies a validated binary denominator in 1..=1024.
pub fn encode(raw: i32, denominator: u16) -> i32 {
    assert!(denominator.is_power_of_two() && denominator <= 1024);
    let divisor = 1024 / denominator as i64;
    let magnitude = (raw as i64).abs();
    let rounded = (magnitude + divisor / 2) / divisor;
    (if raw < 0 { -rounded } else { rounded }) as i32
}

/// The logical Q10 value after a store; None means rounding exceeds Q10.
pub fn snap(raw: i32, denominator: u16) -> Option<i32> {
    encode(raw, denominator).checked_mul(1024 / denominator as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_contract() {
        assert_eq!(denominator("1/16"), Some(16));
        for bad in ["1/0", "1/3", "1/2048", "2/16", "1/+16", "1/1.0"] {
            assert_eq!(denominator(bad), None, "{bad}");
        }
        assert_eq!(encode(204, 16), 3);
        assert_eq!(encode(32, 16), 1);
        assert_eq!(encode(-32, 16), -1);
        assert_eq!(encode(-31, 16), 0);
        assert_eq!(snap(i32::MIN, 1), Some(i32::MIN));
        assert_eq!(snap(i32::MAX, 1), None);
    }
}

// Inspect actual short comments, skipping quoted strings and Lua long blocks.
pub fn comments(source: &str) -> Vec<(usize, &str)> {
    let b = source.as_bytes();
    let (mut i, mut line) = (0, 1);
    let mut result = vec![];
    while i < b.len() {
        let start = i;
        if b[i] == b'\'' || b[i] == b'"' {
            let quote = b[i];
            i += 1;
            while i < b.len() {
                if b[i] == b'\\' {
                    i = (i + 2).min(b.len());
                } else if b[i] == quote {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
        } else {
            let comment = b[i..].starts_with(b"--");
            let open = i + if comment { 2 } else { 0 };
            let mut end = open + 1;
            if b.get(open) == Some(&b'[') {
                while b.get(end) == Some(&b'=') {
                    end += 1;
                }
            }
            if b.get(open) == Some(&b'[') && b.get(end) == Some(&b'[') {
                let close = format!("]{}]", "=".repeat(end - open - 1));
                i = source[end + 1..]
                    .find(&close)
                    .map(|n| end + 1 + n + close.len())
                    .unwrap_or(b.len());
            } else if comment {
                i = source[i..].find('\n').map(|n| i + n).unwrap_or(b.len());
                result.push((line, &source[start..i]));
            } else {
                i += 1;
            }
        }
        line += b[start..i].iter().filter(|&&c| c == b'\n').count();
    }
    result
}

use piccolo::compiler::{self, interning::BasicInterner, parser as ast, LineNumber};
use std::{collections::HashMap, rc::Rc};

type Text = Rc<[u8]>;
type Expr = ast::Expression<Text>;
type Block = ast::Block<Text>;

const ANNOTATION_HELP: &str = "Use `---@snes fixed name lo hi [step 1/N]` with N a power of two from 1 through 1024 (default 1024), or a legacy numeric annotation. Write ordered signed integer bounds on the line before the declaration: `---@snes range score 0 9999` (`fixed10 speed -4 4` and `array scores 8 0 99` take the same form).";

#[derive(Clone, Debug)]
pub struct PreviewError {
    pub line: u64,
    pub message: String,
    pub help: String,
}
impl PreviewError {
    fn new(line: u64, message: &str, help: &str) -> Self {
        Self {
            line,
            message: message.into(),
            help: help.into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Annotation {
    pub line: u64,
    pub comment: (u64, String),
    pub name: String,
    pub min: i32,
    pub max: i32,
    pub capacity: Option<usize>,
    pub fixed: bool,
    pub precision: Option<u16>,
}

pub fn annotations(source: &str) -> Result<Vec<Annotation>, PreviewError> {
    let mut diagnostics = vec![];
    let lines: Vec<_> = source.lines().collect();
    let mut result: Vec<Annotation> = vec![];
    for (line, comment) in comments(source) {
        let Some(annotation) = comment.strip_prefix("---@snes") else {
            continue;
        };
        let parts: Vec<_> = annotation.split_whitespace().collect();
        if parts == ["fixed", "10"]
            || matches!(
                parts.first(),
                Some(
                    &"vblank"
                        | &"lookup"
                        | &"frames"
                        | &"hdma-bake"
                        | &"over-budget"
                        | &"cap"
                        | &"spread"
                )
            )
        {
            continue;
        }
        let parsed = (|| {
            let mut precision = None;
            let (capacity, ty, min, max) = match parts.as_slice() {
                ["fixed", _, min, max] | ["fixed", _, min, max, "step", _] => {
                    precision = Some(if parts.len() == 4 {
                        1024
                    } else {
                        denominator(parts[5])?
                    });
                    (
                        None,
                        true,
                        min.parse::<i32>().ok()?,
                        max.parse::<i32>().ok()?,
                    )
                }
                ["range", _, min, max] => (
                    None,
                    false,
                    min.parse::<i32>().ok()?,
                    max.parse::<i32>().ok()?,
                ),
                ["fixed10", _, min, max] => (
                    None,
                    true,
                    min.parse::<i32>().ok()?,
                    max.parse::<i32>().ok()?,
                ),
                ["array", _, size, min, max] => (
                    Some(size.parse::<usize>().ok()?),
                    false,
                    min.parse::<i32>().ok()?,
                    max.parse::<i32>().ok()?,
                ),
                ["array-fixed10", _, size, min, max] => (
                    Some(size.parse::<usize>().ok()?),
                    true,
                    min.parse::<i32>().ok()?,
                    max.parse::<i32>().ok()?,
                ),
                _ => return None,
            };
            if min > max {
                return None;
            }
            let target = (line..lines.len()).find(|&i| {
                !lines[i].trim().is_empty() && !lines[i].trim_start().starts_with("--")
            })?;
            Some(Annotation {
                line: target as u64 + 1,
                comment: (line as u64, annotation.trim().to_string()),
                name: parts[1].into(),
                min,
                max,
                capacity,
                fixed: ty,
                precision,
            })
        })();
        if let Some(mut a) = parsed {
            if let Some(previous) = result
                .iter_mut()
                .find(|p: &&mut Annotation| p.line == a.line && p.name == a.name)
            {
                let capacity = if previous.precision.is_some()
                    && previous.capacity.is_none()
                    && a.precision.is_none()
                {
                    a.capacity
                } else if a.precision.is_some() && previous.precision.is_none() {
                    previous.capacity
                } else {
                    None
                };
                if let Some(capacity) =
                    capacity.filter(|_| previous.min == a.min && previous.max == a.max)
                {
                    if a.precision.is_some() {
                        a.capacity = Some(capacity);
                        *previous = a;
                    } else {
                        previous.capacity = Some(capacity);
                    }
                    continue;
                }
                diagnostics.push(PreviewError::new(line as u64, "Conflicting numeric annotations.", "Keep one annotation per binding, or stack fixed with one matching array capacity annotation."));
            } else {
                result.push(a);
            }
        } else {
            diagnostics.push(PreviewError::new(line as u64, "Use ---@snes range <name> <min> <max>, with ordered signed 32-bit integer bounds, immediately before a declaration.", ANNOTATION_HELP));
        }
    }
    if let Some(prepared) = crate::bounded::prepare(&[("", source)], false).map_err(|(_, e)| e)? {
        for d in prepared.declarations {
            if result.iter().any(|a| a.line == d.line && a.name == d.name) {
                return Err(PreviewError::new(
                    d.line,
                    "The numeric constructor already declares this binding's contract.",
                    "Remove the numeric annotation above the constructor.",
                ));
            }
            result.push(d.annotation());
        }
    }
    match diagnostics.into_iter().next() {
        Some(e) => Err(e),
        None => Ok(result),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Int,
    Fixed,
    Bool,
    Other,
    Table(usize),
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Key {
    Named(String),
    Position(i64),
    Element,
}
struct Record {
    fields: HashMap<Key, Kind>,
}
impl Kind {
    fn numeric(self) -> bool {
        matches!(self, Self::Int | Self::Fixed)
    }
    fn pair(self, other: Self) -> Self {
        if self == Self::Fixed || other == Self::Fixed {
            Self::Fixed
        } else {
            self
        }
    }
}
#[derive(Clone)]
struct Binding {
    kind: Kind,
    annotation: Option<Annotation>,
    capacity: Option<usize>,
    returns: Option<Vec<Kind>>,
    parameters: Option<Vec<Kind>>,
}
impl Binding {
    fn scalar(kind: Kind) -> Self {
        Self {
            kind,
            annotation: None,
            capacity: None,
            returns: None,
            parameters: None,
        }
    }
}
fn text(s: &Text) -> String {
    String::from_utf8_lossy(s).into_owned()
}
fn name(s: &str) -> Text {
    Rc::from(s.as_bytes())
}
fn simple(s: ast::SimpleExpression<Text>) -> Expr {
    Expr {
        head: Box::new(ast::HeadExpression::Simple(s)),
        tail: vec![],
    }
}
fn integer(i: i64) -> Expr {
    simple(ast::SimpleExpression::Integer(i))
}
fn variable(n: Text) -> Expr {
    simple(ast::SimpleExpression::Suffixed(ast::SuffixedExpression {
        primary: ast::PrimaryExpression::Name(n),
        suffixes: vec![],
    }))
}
fn call(n: &str, args: Vec<Expr>) -> Expr {
    simple(ast::SimpleExpression::Suffixed(ast::SuffixedExpression {
        primary: ast::PrimaryExpression::Name(name(n)),
        suffixes: vec![ast::SuffixPart::Call(ast::CallSuffix::Function(args))],
    }))
}

/// True for a canonical annotation or built-in makeFixed declaration, not string data.
pub fn enabled(source: &str) -> bool {
    comments(source).iter().any(|(_, c)| {
        let p: Vec<_> = c
            .strip_prefix("---@snes")
            .unwrap_or("")
            .split_whitespace()
            .collect();
        p.first() == Some(&"fixed") && p.as_slice() != ["fixed", "10"]
    }) || source.contains("makeFixed")
        && crate::bounded::prepare(&[("", source)], false)
            .ok()
            .flatten()
            .is_some_and(|p| p.declarations.iter().any(|d| d.fixed))
}

/// The same bounded source guard used by raw preview and cartridge validation.
/// Programs without canonical precision or bounded declarations keep ordinary Lua execution.
pub fn prepare_sources(
    files: &[(&str, &str)],
) -> Result<Option<Vec<compiler::CompiledPrototype<Text>>>, (String, PreviewError)> {
    let annotated = files.iter().any(|(_, s)| {
        comments(s).iter().any(|(_, c)| {
            let p: Vec<_> = c
                .strip_prefix("---@snes")
                .unwrap_or("")
                .split_whitespace()
                .collect();
            p.first() == Some(&"fixed") && p.as_slice() != ["fixed", "10"]
        })
    });
    let bounded = crate::bounded::prepare(files, !annotated)?;
    let canonical = annotated
        || bounded
            .as_ref()
            .is_some_and(|p| p.declarations.iter().any(|d| d.fixed));
    if !canonical {
        return bounded
            .map(|p| {
                p.chunks
                    .into_iter()
                    .map(|(file, chunk)| {
                        compiler::compile_chunk(&chunk, BasicInterner::default()).map_err(|e| {
                            (
                                file,
                                PreviewError::new(
                                    e.line_number.0 + 1,
                                    &e.to_string(),
                                    "Split this large function into smaller helpers.",
                                ),
                            )
                        })
                    })
                    .collect()
            })
            .transpose();
    }
    let mut scopes = vec![HashMap::new()];
    let mut records = vec![];
    let mut chunks = vec![];
    for &(file, source) in files {
        let result = (|| {
            let annotations = annotations(source)?;
            let mut lexer =
                compiler::lexer::Lexer::new(source.as_bytes(), BasicInterner::default());
            while let Ok(Some(token)) = lexer.read_token() {
                if matches!(token,compiler::lexer::Token::Name(n) if n.starts_with(b"__ppu_precision_"))
                {
                    return Err(PreviewError::new(lexer.line_number().0+1,"This name is reserved for precision execution.","Rename identifiers beginning __ppu_precision_; ordinary strings and comments may contain that text."));
                }
            }
            let mut chunk = if let Some((_, chunk)) = bounded
                .as_ref()
                .and_then(|p| p.chunks.iter().find(|(f, _)| f == file))
            {
                chunk.clone()
            } else {
                compiler::parse_chunk(source.as_bytes(), BasicInterner::default()).map_err(|e| {
                    PreviewError::new(
                        e.line_number.0 + 1,
                        &e.to_string(),
                        "Fix the Lua syntax at this location.",
                    )
                })?
            };
            scopes.push(HashMap::new());
            let mut transform = Transform {
                scopes: &mut scopes,
                annotations,
                used: vec![],
                source,
                file,
                line: 1,
                returns: vec![],
                records: &mut records,
            };
            transform.block(&mut chunk.block, false)?;
            if let Some(a) = transform
                .annotations
                .iter()
                .find(|a| !transform.used.contains(&(a.line, a.name.clone())))
            {
                return Err(PreviewError::new(a.comment.0, "This numeric annotation did not bind to a supported declaration.", "Put the annotation immediately before its local/global numeric declaration or named helper parameter."));
            }
            transform.scopes.pop();
            compiler::compile_chunk(&chunk, BasicInterner::default()).map_err(|e| PreviewError::new(e.line_number.0+1,&e.to_string(),"Split this large precision function or assignment into smaller named helpers and explicit stores."))
        })();
        chunks.push(result.map_err(|e| (file.to_string(), e))?);
    }
    Ok(Some(chunks))
}

struct Transform<'a> {
    scopes: &'a mut Vec<HashMap<String, Binding>>,
    annotations: Vec<Annotation>,
    used: Vec<(u64, String)>,
    source: &'a str,
    file: &'a str,
    line: u64,
    returns: Vec<Vec<Kind>>,
    records: &'a mut Vec<Record>,
}
impl Transform<'_> {
    fn helper(&self, n: &str, mut args: Vec<Expr>) -> Expr {
        args.push(simple(ast::SimpleExpression::String(name(self.file))));
        call(n, args)
    }
    fn parallel(&self, values: &mut Vec<Expr>) {
        if values.len() < 2 {
            return;
        }
        let mut annotated = false;
        let mut args = vec![];
        for value in values.iter() {
            if let ast::HeadExpression::Simple(ast::SimpleExpression::Suffixed(
                ast::SuffixedExpression {
                    primary: ast::PrimaryExpression::Name(n),
                    suffixes,
                },
            )) = value.head.as_ref()
            {
                if value.tail.is_empty() && text(n) == "__ppu_precision_store" {
                    if let [ast::SuffixPart::Call(ast::CallSuffix::Function(store))] =
                        suffixes.as_slice()
                    {
                        args.extend([
                            store[0].clone(),
                            store[1].clone(),
                            store[2].clone(),
                            store[3].clone(),
                            store[5].clone(),
                        ]);
                        annotated = true;
                        continue;
                    }
                }
            }
            args.extend([
                value.clone(),
                integer(-1),
                integer(0),
                integer(0),
                simple(ast::SimpleExpression::String(name(""))),
            ]);
        }
        if annotated {
            args.push(integer(self.line as i64));
            *values = vec![self.helper("__ppu_precision_parallel", args)];
        }
    }
    fn error(&self, message: &str, help: &str) -> PreviewError {
        PreviewError::new(self.line, message, help)
    }
    fn binding(&self, n: &str) -> Option<Binding> {
        self.scopes.iter().rev().find_map(|s| s.get(n).cloned())
    }
    fn annotation(&mut self, n: &str) -> Result<Option<Annotation>, PreviewError> {
        let a = self
            .annotations
            .iter()
            .find(|a| a.line == self.line && a.name == n)
            .cloned();
        if let Some(a) = &a {
            if a.fixed && (a.min.checked_mul(1024).is_none() || a.max.checked_mul(1024).is_none()) {
                return Err(self.error(
                    "Fixed bounds exceed Q21.10.",
                    "Keep fixed bounds between -2097152 and 2097151.",
                ));
            }
            self.used.push((a.line, a.name.clone()));
        }
        Ok(a)
    }
    fn store(&self, e: Expr, a: &Annotation) -> Expr {
        self.helper(
            "__ppu_precision_store",
            vec![
                e,
                integer(if a.fixed {
                    a.precision.unwrap_or(1024) as i64
                } else {
                    0
                }),
                integer(a.min as i64),
                integer(a.max as i64),
                integer(self.line as i64),
                simple(ast::SimpleExpression::String(name(&a.name))),
            ],
        )
    }
    fn fixed_value(&self, e: Expr) -> Expr {
        self.helper(
            "__ppu_precision_op",
            vec![
                simple(ast::SimpleExpression::String(name("add"))),
                e,
                integer(0),
                integer(1),
                integer(self.line as i64),
            ],
        )
    }
    fn key(&mut self, field: &mut ast::FieldSuffix<Text>) -> Result<Key, PreviewError> {
        match field {
            ast::FieldSuffix::Named(n) => Ok(Key::Named(text(n))),
            ast::FieldSuffix::Indexed(e) => {
                if e.tail.is_empty() {
                    if let ast::HeadExpression::Simple(ast::SimpleExpression::String(n)) =
                        e.head.as_ref()
                    {
                        return Ok(Key::Named(text(n)));
                    }
                }
                let position = if e.tail.is_empty() {
                    match e.head.as_ref() {
                        ast::HeadExpression::Simple(ast::SimpleExpression::Integer(n)) => Some(*n),
                        _ => None,
                    }
                } else {
                    None
                };
                if self.expr(e)? != Kind::Int {
                    return Err(self.error(
                        "A precision list index must be an integer.",
                        "Use an integer index or a literal field name.",
                    ));
                }
                Ok(position.map(Key::Position).unwrap_or(Key::Element))
            }
        }
    }
    fn table_key(fields: &HashMap<Key, Kind>, key: Key) -> Key {
        if matches!(key, Key::Position(_))
            && !fields.contains_key(&key)
            && fields.contains_key(&Key::Element)
        {
            Key::Element
        } else {
            key
        }
    }
    fn compatible(&self, a: Kind, b: Kind, seen: &mut Vec<(usize, usize)>) -> bool {
        if a == b {
            return true;
        }
        let (Kind::Table(a), Kind::Table(b)) = (a, b) else {
            return false;
        };
        if seen.contains(&(a, b)) {
            return true;
        }
        seen.push((a, b));
        let (a, b) = (&self.records[a].fields, &self.records[b].fields);
        a.len() == b.len()
            && a.iter().all(|(key, &kind)| {
                b.get(key)
                    .is_some_and(|&other| self.compatible(kind, other, seen))
            })
    }
    fn constructor(&mut self, t: &mut ast::TableConstructor<Text>) -> Result<Kind, PreviewError> {
        if matches!(t.fields.last(),Some(ast::ConstructorField::Array(e)) if self.returned_kinds(e).len()>1)
        {
            return Err(self.error("A precision constructor tail cannot expand multiple helper results.","Capture the results into named locals, then list each constructor entry explicitly."));
        }
        let mut fields: HashMap<Key, Kind> = HashMap::new();
        let mut keys = vec![];
        let mut position = 0;
        for field in &mut t.fields {
            let (key, value) = match field {
                ast::ConstructorField::Array(e) => {
                    position += 1;
                    (Key::Position(position), e)
                }
                ast::ConstructorField::Record(k, e) => {
                    let key = match k {
                        ast::RecordKey::Named(n) => Key::Named(text(n)),
                        ast::RecordKey::Indexed(index) => {
                            let mut field = ast::FieldSuffix::Indexed(index.clone());
                            let key = self.key(&mut field)?;
                            let ast::FieldSuffix::Indexed(value) = field else {
                                unreachable!()
                            };
                            *index = value;
                            key
                        }
                    };
                    (key, e)
                }
            };
            keys.push(key.clone());
            let kind = self.expr(value)?;
            if let Some(&previous) = fields.get(&key) {
                let combined = if previous.numeric() && kind.numeric() {
                    previous.pair(kind)
                } else if self.compatible(previous, kind, &mut vec![]) {
                    previous
                } else {
                    return Err(self.error(
                        "A precision field has incompatible initializer types.",
                        "Keep one type per field; use literal positions for mixed tuples.",
                    ));
                };
                fields.insert(key, combined);
            } else {
                fields.insert(key, kind);
            }
        }
        let mut common = None;
        let mut uniform = true;
        for key in &keys {
            if matches!(key, Key::Named(_)) {
                continue;
            }
            let kind = fields[key];
            common = match common {
                None => Some(kind),
                Some(previous) if previous.numeric() && kind.numeric() => Some(previous.pair(kind)),
                Some(previous) if self.compatible(previous, kind, &mut vec![]) => Some(previous),
                _ => {
                    uniform = false;
                    break;
                }
            };
        }
        if uniform {
            if let Some(kind) = common {
                fields.retain(|key, _| matches!(key, Key::Named(_)));
                fields.insert(Key::Element, kind);
            }
        } else if fields.contains_key(&Key::Element) {
            return Err(self.error(
                "Dynamic precision list keys need one element type.",
                "Use literal positions for mixed tuples and homogeneous types for dynamic indexes.",
            ));
        }
        for (field, key) in t.fields.iter_mut().zip(keys) {
            if fields.get(&Self::table_key(&fields, key)) == Some(&Kind::Fixed) {
                let (ast::ConstructorField::Array(value) | ast::ConstructorField::Record(_, value)) =
                    field;
                *value = self.fixed_value(value.clone());
            }
        }
        let id = self.records.len();
        self.records.push(Record { fields });
        Ok(Kind::Table(id))
    }
    fn record_head(
        &mut self,
        s: &mut ast::SuffixedExpression<Text>,
    ) -> Result<Option<Kind>, PreviewError> {
        let ast::PrimaryExpression::Name(n) = &s.primary else {
            return Ok(None);
        };
        let Some(binding) = self.binding(&text(n)) else {
            return Ok(None);
        };
        let Kind::Table(_) = binding.kind else {
            return Ok(None);
        };
        let mut kind = binding.kind;
        for suffix in &mut s.suffixes {
            let Kind::Table(id) = kind else {
                return Err(self.error(
                    "A scalar precision value has no fields.",
                    "Access fields on a record, and use numbers directly.",
                ));
            };
            let ast::SuffixPart::Field(field) = suffix else {
                return Err(self.error(
                    "Calling a record field is outside precision execution.",
                    "Use a named helper with numeric arguments.",
                ));
            };
            let key = self.key(field)?;
            let key = Self::table_key(&self.records[id].fields, key);
            kind = *self.records[id].fields.get(&key).ok_or_else(|| {
                self.error(
                    "This precision field has no known type.",
                    "Initialize the record field or list before reading it.",
                )
            })?;
        }
        Ok(Some(kind))
    }
    fn block(&mut self, b: &mut Block, scoped: bool) -> Result<(), PreviewError> {
        if scoped {
            self.scopes.push(HashMap::new());
        }
        for s in &mut b.statements {
            self.line = s.line_number.0 + 1;
            self.statement(&mut s.inner)?;
        }
        if let Some(r) = &mut b.return_statement {
            self.line = r.line_number.0 + 1;
            let kinds = r
                .inner
                .returns
                .iter_mut()
                .map(|e| self.expr(e))
                .collect::<Result<Vec<_>, _>>()?;
            if kinds.iter().any(|k| !k.numeric() && *k != Kind::Bool) {
                return Err(self.error("Precision helpers return only scalar values.", "Return named numbers or booleans; keep tables in their original numeric-array binding."));
            }
            self.returns.push(kinds);
        }
        if scoped {
            self.scopes.pop();
        }
        Ok(())
    }
    fn returned_kinds(&self, e: &Expr) -> Vec<Kind> {
        if let ast::HeadExpression::Simple(ast::SimpleExpression::Suffixed(
            ast::SuffixedExpression {
                primary: ast::PrimaryExpression::Name(n),
                suffixes,
            },
        )) = e.head.as_ref()
        {
            if e.tail.is_empty() && matches!(suffixes.as_slice(), [ast::SuffixPart::Call(_)]) {
                return self
                    .binding(&text(n))
                    .and_then(|b| b.returns)
                    .unwrap_or_default();
            }
        }
        vec![]
    }
    fn statement(&mut self, s: &mut ast::Statement<Text>) -> Result<(), PreviewError> {
        use ast::Statement as S;
        match s {
            S::LocalStatement(s) => {
                let annotations = s.names.iter().map(|n| self.annotation(&text(n))).collect::<Result<Vec<_>,_>>()?;
                if s.values.len() != s.names.len() && annotations.iter().any(Option::is_some) { return Err(self.error("An annotated declaration needs one explicit value per name.", "Split a multiple-return call into unannotated temporaries, then declare each annotated value explicitly.")); }
                let returned=s.values.last().map(|e|self.returned_kinds(e)).unwrap_or_default();
                let mut bindings = vec![];
                for (i, n) in s.names.iter().enumerate() {
                    let a = annotations[i].clone();
                    let mut binding = Binding::scalar(Kind::Other);
                    if let Some(e) = s.values.get_mut(i) {
                        if let ast::HeadExpression::Simple(ast::SimpleExpression::TableConstructor(t)) = e.head.as_mut() {
                            if a.is_some() {
                            if e.tail.is_empty() {
                                if matches!(t.fields.last(),Some(ast::ConstructorField::Array(e)) if self.returned_kinds(e).len()>1) {return Err(self.error("A precision constructor tail cannot expand multiple helper results.","Capture the results into named locals, then list each constructor entry explicitly."));}
                                let cap = a.as_ref().and_then(|a| a.capacity).unwrap_or(t.fields.len());
                                if cap == 0 || cap > 32767 { return Err(self.error("A numeric array needs a bounded capacity.", "Use a nonempty numeric constructor, or stack a matching array capacity annotation above fixed.")); }
                                let mut kind = Kind::Int;
                                for field in &mut t.fields {
                                    let ast::ConstructorField::Array(v) = field else { return Err(self.error("Precision arrays use numeric list constructors.", "Write {0, 0, 0}; keep records and nested tables outside precision programs.")); };
                                    let k = self.expr(v)?;
                                    if !k.numeric() { return Err(self.error("A precision array element must be numeric.", "Initialize each array element with a number.")); }
                                    kind = kind.pair(k);
                                    if let Some(a) = &a { *v = self.store(v.clone(), a); }
                                }
                                if t.fields.len() > cap { return Err(self.error("The array constructor exceeds its capacity.", "Increase the matching capacity annotation or shorten the constructor.")); }
                                binding.kind = if a.as_ref().is_some_and(|a| a.fixed) { Kind::Fixed } else { kind };
                                binding.capacity = Some(cap);
                            }
                            }
                        }
                        if binding.capacity.is_none() {
                            let kind = self.expr(e)?;
                            if let Some(a) = &a {
                                if a.fixed {
                                    if !kind.numeric() { return Err(self.error("A fixed binding must be numeric.", "Initialize it with a number, or a numeric list constructor for an array.")); }
                                    *e = self.store(e.clone(), a);
                                    binding.kind = Kind::Fixed;
                                } else {
                                    if kind!=Kind::Int { return Err(self.error("An integer annotation needs an integer value.","Use a fixed annotation for fractional state.")); }
                                    *e=self.store(e.clone(),a); binding.kind=Kind::Int;
                                }
                            } else { binding.kind = kind; }
                        }
                    }
                    if i>=s.values.len() {binding.kind=returned.get(i+1-s.values.len()).copied().unwrap_or(Kind::Other);}
                    binding.annotation = a;
                    bindings.push((text(n), binding));
                }
                self.parallel(&mut s.values);
                self.scopes.last_mut().unwrap().extend(bindings);
            }
            S::Assignment(s) => self.assign(s)?,
            S::Function(s) => {
                if !s.fields.is_empty() || s.method.is_some() { return Err(self.error("Precision helpers need plain names.", "Use a named helper such as local function update(x), then call update(x).")); }
                let binding = self.function(&mut s.definition, &text(&s.name))?;
                self.scopes[0].insert(text(&s.name), binding);
            }
            S::LocalFunction(s) => {
                let binding = self.function(&mut s.definition, &text(&s.name))?;
                self.scopes.last_mut().unwrap().insert(text(&s.name), binding);
            }
            S::Do(b) => self.block(b, true)?,
            S::If(s) => {
                self.expr(&mut s.if_part.0)?; self.block(&mut s.if_part.1, true)?;
                for (c,b) in &mut s.else_if_parts { self.expr(c)?; self.block(b,true)?; }
                if let Some(b) = &mut s.else_part { self.block(b,true)?; }
            }
            S::While(s) => { self.expr(&mut s.condition)?; self.block(&mut s.block,true)?; }
            S::Repeat(s) => {
                self.scopes.push(HashMap::new()); self.block(&mut s.body,false)?; self.expr(&mut s.until)?; self.scopes.pop();
            }
            S::For(ast::ForStatement::Numeric {name, initial, limit, step, body}) => {
                for e in [Some(initial),Some(limit),step.as_mut()].into_iter().flatten() {
                    if self.expr(e)? != Kind::Int { return Err(self.error("Precision loops use integer bounds.", "Use integer loop bounds and floor() a fractional limit first.")); }
                }
                self.scopes.push(HashMap::from([(text(name),Binding::scalar(Kind::Int))]));
                self.block(body,false)?; self.scopes.pop();
            }
            S::FunctionCall(s) => {
                let mut e = simple(ast::SimpleExpression::Suffixed(ast::SuffixedExpression { primary: s.head.primary.clone(), suffixes: { let mut v=s.head.suffixes.clone(); v.push(ast::SuffixPart::Call(s.call.clone())); v } }));
                self.expr(&mut e)?;
                let ast::HeadExpression::Simple(ast::SimpleExpression::Suffixed(mut head)) = *e.head else { unreachable!() };
                let ast::SuffixPart::Call(last) = head.suffixes.pop().unwrap() else { unreachable!() };
                s.head=head; s.call=last;
            }
            S::Break => {},
            _ => return Err(self.error("This Lua statement is outside the precision preview surface.", "Use numeric for/while loops, if blocks and named helpers; replace generic iteration, goto and labels with those forms.")),
        }
        Ok(())
    }
    fn function(
        &mut self,
        f: &mut ast::FunctionDefinition<Text>,
        n: &str,
    ) -> Result<Binding, PreviewError> {
        let line = self.line;
        let lookup = comments(self.source).iter().any(|(l, c)| {
            c.strip_prefix("---@snes")
                .is_some_and(|a| a.split_whitespace().next() == Some("lookup"))
                && self
                    .source
                    .lines()
                    .enumerate()
                    .skip(*l)
                    .find(|(_, l)| !l.trim().is_empty() && !l.trim_start().starts_with("--"))
                    .is_some_and(|(i, _)| i as u64 + 1 == line)
        });
        if lookup {
            return Ok(Binding {
                returns: Some(vec![Kind::Int]),
                parameters: Some(vec![Kind::Int; f.parameters.len()]),
                ..Binding::scalar(Kind::Other)
            });
        }
        if f.has_varargs {
            return Err(self.error(
                "Varargs cannot preserve precision argument stores.",
                "Give the helper named numeric parameters.",
            ));
        }
        self.scopes.push(HashMap::new());
        let mut parameters = vec![];
        let mut stores = ast::AssignmentStatement {
            targets: vec![],
            values: vec![],
        };
        for (i, p) in f.parameters.iter().enumerate() {
            let a = self.annotation(&text(p))?;
            let kind = if a.as_ref().is_some_and(|a| a.fixed) || n == "frame" && i == 0 {
                Kind::Fixed
            } else {
                Kind::Int
            };
            parameters.push(kind);
            let mut b = Binding::scalar(kind);
            b.annotation = a.clone();
            self.scopes.last_mut().unwrap().insert(text(p), b);
            if let Some(a) = a.filter(|_| n != "frame" || i != 0) {
                stores.targets.push(ast::AssignmentTarget::Name(p.clone()));
                stores.values.push(self.store(variable(p.clone()), &a));
            }
        }
        if n == "frame" && !f.parameters.is_empty() {
            if f.parameters.len() < 2 {
                return Err(self.error("Precision frame time needs the frame counter.", "Declare function frame(t, f); time is derived from f using the cartridge's floor(f*1024/60) rule."));
            }
            let mut time = self.helper(
                "__ppu_precision_time",
                vec![variable(f.parameters[1].clone()), integer(line as i64)],
            );
            if let Some(a) = self
                .binding(&text(&f.parameters[0]))
                .and_then(|b| b.annotation)
            {
                time = self.store(time, &a);
            }
            stores
                .targets
                .push(ast::AssignmentTarget::Name(f.parameters[0].clone()));
            stores.values.push(time);
        }
        let prev = std::mem::take(&mut self.returns);
        self.block(&mut f.body, false)?;
        let rows = std::mem::replace(&mut self.returns, prev);
        self.scopes.pop();
        self.line = line;
        self.parallel(&mut stores.values);
        if !stores.targets.is_empty() {
            f.body.statements.insert(
                0,
                ast::LineAnnotated::new(LineNumber(line - 1), ast::Statement::Assignment(stores)),
            );
        }
        let mut kinds = rows.first().cloned().unwrap_or_default();
        for row in &rows {
            if row.len() != kinds.len() {
                return Err(self.error(
                    "Precision helpers need a consistent return count.",
                    "Return the same number of scalar values on every branch.",
                ));
            }
            for (k, r) in kinds.iter_mut().zip(row) {
                if k.numeric() && r.numeric() {
                    *k = k.pair(*r);
                } else if k != r {
                    return Err(self.error(
                        "Precision helpers need consistent return types.",
                        "Return a number on every branch, or a boolean on every branch.",
                    ));
                }
            }
        }
        Ok(Binding {
            returns: Some(kinds),
            parameters: Some(parameters),
            ..Binding::scalar(Kind::Other)
        })
    }
    fn assign(&mut self, s: &mut ast::AssignmentStatement<Text>) -> Result<(), PreviewError> {
        if self.annotations.iter().any(|a| a.line == self.line)
            && s.targets
                .iter()
                .all(|t| matches!(t, ast::AssignmentTarget::Name(_)))
        {
            if s.targets.iter().any(
                |t| matches!(t,ast::AssignmentTarget::Name(n) if self.binding(&text(n)).is_some()),
            ) {
                return Err(self.error(
                    "This annotation follows an existing declaration.",
                    "Move the numeric annotation immediately above the original declaration.",
                ));
            }
            let names = s
                .targets
                .iter()
                .map(|t| {
                    let ast::AssignmentTarget::Name(n) = t else {
                        unreachable!()
                    };
                    n.clone()
                })
                .collect();
            let mut local = ast::Statement::LocalStatement(ast::LocalStatement {
                names,
                values: std::mem::take(&mut s.values),
            });
            self.scopes.push(HashMap::new());
            self.statement(&mut local)?;
            let globals = self.scopes.pop().unwrap();
            self.scopes[0].extend(globals);
            let ast::Statement::LocalStatement(local) = local else {
                unreachable!()
            };
            s.values = local.values;
            return Ok(());
        }
        let mut stores = vec![];
        let mut record_targets = vec![];
        for t in &mut s.targets {
            record_targets.push(None);
            let b = match t {
                ast::AssignmentTarget::Name(n) => self.binding(&text(n)),
                ast::AssignmentTarget::Field(head, field) => {
                    if let Some(kind) = self.record_head(head)? {
                        let Kind::Table(id) = kind else {
                            return Err(self.error(
                                "A scalar precision value has no fields.",
                                "Assign a record field or a numeric binding.",
                            ));
                        };
                        let key = self.key(field)?;
                        let key = Self::table_key(&self.records[id].fields, key);
                        stores.push(
                            self.records[id]
                                .fields
                                .get(&key)
                                .copied()
                                .map(Binding::scalar),
                        );
                        *record_targets.last_mut().unwrap() = Some((id, key));
                        continue;
                    }
                    if let ast::PrimaryExpression::Name(n) = &head.primary {
                        if head.suffixes.is_empty() {
                            if let Some(b) = self.binding(&text(n)).filter(|b| b.capacity.is_some())
                            {
                                let ast::FieldSuffix::Indexed(index) = field else {
                                    return Err(self.error(
                                        "A numeric array needs an integer index.",
                                        "Write A[i] with i from 1 through the declared capacity.",
                                    ));
                                };
                                if self.expr(index)? != Kind::Int {
                                    return Err(self.error("A numeric array index must be an integer.","Use floor(index) and keep the result within the array's capacity."));
                                }
                                *index = self.helper(
                                    "__ppu_precision_index",
                                    vec![
                                        index.clone(),
                                        integer(b.capacity.unwrap() as i64),
                                        integer(self.line as i64),
                                    ],
                                );
                                stores.push(Some(b));
                                continue;
                            }
                        }
                    }
                    self.hardware(head)?;
                    if let ast::FieldSuffix::Indexed(e) = field {
                        self.expr(e)?;
                    }
                    None
                }
            };
            if b.as_ref().is_some_and(|b| b.capacity.is_some()) {
                return Err(self.error(
                    "A numeric array binding cannot be replaced or aliased.",
                    "Assign its elements with A[i] = value; keep the original array binding.",
                ));
            }
            stores.push(b);
        }
        if s.values.len() != s.targets.len()
            && stores
                .iter()
                .flatten()
                .any(|b| b.annotation.as_ref().is_some_and(|a| a.fixed) || b.capacity.is_some())
        {
            return Err(self.error("A precision parallel assignment needs one explicit expression per destination.","Split a multiple-return helper call into unannotated temporaries, then assign the precision destinations explicitly."));
        }
        let explicit = s.values.len() == s.targets.len();
        for (i, e) in s.values.iter_mut().enumerate() {
            let matrix = matches!(s.targets.get(i),Some(ast::AssignmentTarget::Field(ast::SuffixedExpression {primary:ast::PrimaryExpression::Name(n),suffixes},ast::FieldSuffix::Named(f))) if text(n)=="m7" && suffixes.is_empty() && matches!(text(f).as_str(),"a"|"b"|"c"|"d"));
            if matrix && constant_expression(e) {
                continue;
            } // Mode7 constants have one authored-f64 -> Q8 rounding.
            let k = self.expr(e)?;
            if let Some(b) = stores.get(i).and_then(Option::as_ref) {
                if !k.numeric() && b.kind.numeric() {
                    return Err(self.error(
                        "A numeric binding needs a numeric store.",
                        "Assign a number; keep tables and booleans in separate bindings.",
                    ));
                }
                if b.kind == Kind::Int && k == Kind::Fixed {
                    return Err(self.error("A later fractional store changes an inferred integer binding.","Initialize a fractional record field with 0.0, or declare a scalar using makeFixed(initial, minimum, maximum)."));
                }
                if matches!(b.kind, Kind::Table(_)) && !self.compatible(b.kind, k, &mut vec![]) {
                    return Err(self.error("A precision record replacement changes its field types.","Initialize replacement records with the same fields and numeric representations."));
                }
                if let Some(a) = b.annotation.as_ref().filter(|a| a.fixed || explicit) {
                    *e = self.store(e.clone(), a);
                } else if b.kind == Kind::Fixed {
                    *e = self.fixed_value(e.clone());
                }
            } else if let Some(ast::AssignmentTarget::Name(n)) = s.targets.get(i) {
                if self.binding(&text(n)).is_none() {
                    self.scopes[0].insert(text(n), Binding::scalar(k));
                }
            }
            if let Some(Some((id, key))) = record_targets.get(i) {
                self.records[*id].fields.entry(key.clone()).or_insert(k);
            }
        }
        self.parallel(&mut s.values);
        Ok(())
    }
    fn hardware(&mut self, head: &mut ast::SuffixedExpression<Text>) -> Result<(), PreviewError> {
        let ast::PrimaryExpression::Name(n) = &head.primary else {
            return Err(self.error(
                "Precision field access needs a known hardware binding.",
                "Use direct obj/bg/m7/vram/cgram accesses, or a named numeric array.",
            ));
        };
        if !matches!(
            text(n).as_str(),
            "obj" | "bg" | "m7" | "vram" | "cgram" | "raster" | "pad" | "voice" | "dsp"
        ) {
            return Err(self.error(&format!("Precision analysis cannot resolve fields on {:?}.",text(n)),"Use direct hardware access, or initialize a record before helpers that read it. Forward record/helper inference is not yet supported."));
        }
        for suffix in &mut head.suffixes {
            match suffix {
                ast::SuffixPart::Field(ast::FieldSuffix::Indexed(e)) => {
                    self.expr(e)?;
                }
                ast::SuffixPart::Field(_) => {}
                _ => {
                    return Err(self.error(
                        "Chained calls are outside the precision preview surface.",
                        "Call a named helper directly, then use its scalar result.",
                    ))
                }
            }
        }
        Ok(())
    }
    fn expr(&mut self, e: &mut Expr) -> Result<Kind, PreviewError> {
        let mut kind=match e.head.as_mut() {
            ast::HeadExpression::Simple(s)=>match s {
                ast::SimpleExpression::Integer(n)=>{if i32::try_from(*n).is_err(){return Err(self.error("Integer literal exceeds the cartridge range.","Keep literal integers within signed 32-bit range."));} Kind::Int},
                ast::SimpleExpression::Float(v)=>{let raw=(*v*1024.).round(); if !raw.is_finite() || raw < i32::MIN as f64 || raw>i32::MAX as f64 {return Err(self.error("Fractional literal exceeds Q21.10.","Keep fractional literals in the Q21.10 range."));} *v=raw/1024.; Kind::Fixed},
                ast::SimpleExpression::True|ast::SimpleExpression::False=>Kind::Bool,
                ast::SimpleExpression::Nil|ast::SimpleExpression::String(_)=>Kind::Other,
                ast::SimpleExpression::Suffixed(s)=>self.suffixed(s)?,
                ast::SimpleExpression::TableConstructor(t)=>self.constructor(t)?,
                _=>return Err(self.error("This value is outside the precision preview surface.","Use named numeric helpers, scalar bindings and numeric list constructors; replace varargs, anonymous functions and table aliases with those forms.")),
            },
            ast::HeadExpression::UnaryOperator(op,v)=>{
                let k=if matches!(op,ast::UnaryOperator::Len) {
                    let valid=matches!(v.head.as_ref(),ast::HeadExpression::Simple(ast::SimpleExpression::Suffixed(ast::SuffixedExpression {primary:ast::PrimaryExpression::Name(n),suffixes})) if v.tail.is_empty() && suffixes.is_empty() && self.binding(&text(n)).is_some_and(|b|b.capacity.is_some()));
                    if !valid && !matches!(self.expr(v)?,Kind::Table(_)) { return Err(self.error("Precision length needs an array or record list.", "Use #A on a numeric array or list.")); }
                    Kind::Int
                } else { self.expr(v)? };
                if matches!(op,ast::UnaryOperator::Not) {Kind::Bool}
                else if matches!(op,ast::UnaryOperator::Len) {Kind::Int}
                else {if !k.numeric(){return Err(self.error("Unary arithmetic needs a number.","Use a numeric scalar operand."));} k}
            }
        };
        if let ast::HeadExpression::UnaryOperator(ast::UnaryOperator::Minus, value) =
            e.head.as_ref()
        {
            e.head = self
                .helper(
                    "__ppu_precision_op",
                    vec![
                        simple(ast::SimpleExpression::String(name("neg"))),
                        value.clone(),
                        integer(0),
                        integer((kind == Kind::Fixed) as i64),
                        integer(self.line as i64),
                    ],
                )
                .head;
        }
        let tail = std::mem::take(&mut e.tail);
        let mut left = e.clone();
        for (op, mut right) in tail {
            let r = self.expr(&mut right)?;
            use ast::BinaryOperator as B;
            if matches!(op, B::And | B::Or) {
                kind = if kind == Kind::Bool && r != Kind::Bool {
                    r
                } else if kind.numeric() && r.numeric() {
                    kind.pair(r)
                } else if kind == r {
                    kind
                } else {
                    return Err(self.error("This conditional mixes incompatible precision values.","Use an if statement and assign numeric alternatives to an annotated binding."));
                };
                left.tail.push((op, right));
                continue;
            }
            if matches!(
                op,
                B::Equal
                    | B::NotEqual
                    | B::LessThan
                    | B::LessEqual
                    | B::GreaterThan
                    | B::GreaterEqual
            ) {
                if kind.numeric() && r.numeric() {
                    let code = match op {
                        B::Equal => "eq",
                        B::NotEqual => "ne",
                        B::LessThan => "lt",
                        B::LessEqual => "le",
                        B::GreaterThan => "gt",
                        B::GreaterEqual => "ge",
                        _ => unreachable!(),
                    };
                    left = self.helper(
                        "__ppu_precision_op",
                        vec![
                            simple(ast::SimpleExpression::String(name(code))),
                            left,
                            right,
                            integer((kind.pair(r) == Kind::Fixed) as i64),
                            integer(self.line as i64),
                        ],
                    );
                    kind = Kind::Bool;
                    continue;
                }
                kind = Kind::Bool;
                left.tail.push((op, right));
                continue;
            }
            if !kind.numeric() || !r.numeric() {
                return Err(self.error(
                    "Precision arithmetic needs numeric operands.",
                    "Use numeric scalars and floor() when an integer is needed.",
                ));
            }
            let pair = kind.pair(r);
            let result = if matches!(op, B::Div) {
                Kind::Fixed
            } else if matches!(
                op,
                B::IDiv | B::BitAnd | B::BitOr | B::BitXor | B::ShiftLeft | B::ShiftRight
            ) {
                Kind::Int
            } else {
                pair
            };
            let code=match op {B::Add=>"add",B::Sub=>"sub",B::Mul=>"mul",B::Div=>"div",B::IDiv=>"idiv",B::Mod=>"mod",B::BitAnd|B::BitOr|B::BitXor|B::ShiftLeft|B::ShiftRight=>{if pair!=Kind::Int{return Err(self.error("Bit operations need integer operands.","Use floor(value) before bit operations."));} left.tail.push((op,right));kind=result;continue;},_=>return Err(self.error("This arithmetic operator is outside the precision preview surface.","Replace powers with explicit multiplication; use +, -, *, /, // or % on numbers."))};
            left = self.helper(
                "__ppu_precision_op",
                vec![
                    simple(ast::SimpleExpression::String(name(code))),
                    left,
                    right,
                    integer((pair == Kind::Fixed || op == B::Div) as i64),
                    integer(self.line as i64),
                ],
            );
            kind = result;
        }
        *e = left;
        Ok(kind)
    }
    fn suffixed(&mut self, s: &mut ast::SuffixedExpression<Text>) -> Result<Kind, PreviewError> {
        if let ast::PrimaryExpression::GroupedExpression(e) = &mut s.primary {
            if !s.suffixes.is_empty() {
                return Err(self.error(
                    "A precision grouped expression cannot be called or indexed.",
                    "Store a scalar result in a local, or access a named numeric array directly.",
                ));
            }
            return self.expr(e);
        }
        let ast::PrimaryExpression::Name(n) = &s.primary else {
            unreachable!()
        };
        let n = text(n);
        let binding = self.binding(&n);
        if s.suffixes.is_empty() {
            if let Some(b) = binding {
                if b.capacity.is_some() {
                    return Err(self.error(
                        "Numeric arrays cannot escape through aliases or returns.",
                        "Read A[i] or #A directly; pass individual numbers to helpers.",
                    ));
                }
                return Ok(b.kind);
            }
            return match n.as_str(){"BGMODE"|"MOSAIC"|"WH0"|"WH1"|"WH2"|"WH3"=>Ok(Kind::Int),_=>Err(self.error(&format!("Unknown precision input {n:?}."),"Declare numeric state and helpers before their uses; give helper parameters explicit numeric names."))};
        }
        if let Some(ast::SuffixPart::Call(_)) = s.suffixes.last() {
            let callee = if s.suffixes.len() == 1 {
                n.clone()
            } else if s.suffixes.len() == 2 && n == "math" {
                let ast::SuffixPart::Field(ast::FieldSuffix::Named(f)) = &s.suffixes[0] else {
                    return Err(
                        self.error("Unsupported precision call.", "Call a plain named helper.")
                    );
                };
                format!("math.{}", text(f))
            } else {
                return Err(self.error(
                    "Unsupported precision call chain.",
                    "Call a plain named helper returning a scalar.",
                ));
            };
            let ast::SuffixPart::Call(ast::CallSuffix::Function(args)) =
                s.suffixes.last_mut().unwrap()
            else {
                return Err(self.error(
                    "Methods cannot preserve precision stores.",
                    "Use a named helper with explicit parameters.",
                ));
            };
            let mut kinds = vec![];
            for a in args.iter_mut() {
                kinds.push(self.expr(a)?);
            }
            if let Some(params) = binding.as_ref().and_then(|b| b.parameters.as_ref()) {
                if params.len() != kinds.len() {
                    return Err(self.error("Precision helper arguments need an explicit value per parameter.", "Pass one numeric expression per named parameter; split multiple-return calls into temporaries first."));
                }
                if params
                    .iter()
                    .zip(&kinds)
                    .any(|(p, k)| *p == Kind::Int && *k == Kind::Fixed)
                {
                    return Err(self.error("A fractional argument needs an annotated helper parameter.", "Put ---@snes fixed parameter lo hi above the helper definition to declare its Q10 parameter."));
                }
                if kinds.iter().any(|k| matches!(k, Kind::Table(_))) {
                    return Err(self.error("Record helper arguments need precision type inference.","Pass the numeric fields to this helper until record parameter inference is available."));
                }
                return Ok(binding
                    .as_ref()
                    .and_then(|b| b.returns.as_ref())
                    .and_then(|r| r.first())
                    .copied()
                    .unwrap_or(Kind::Other));
            }
            let numeric = || kinds.iter().copied().fold(Kind::Int, Kind::pair);
            if matches!(
                callee.as_str(),
                "abs" | "math.abs" | "min" | "max" | "math.min" | "math.max"
            ) {
                let code = callee.strip_prefix("math.").unwrap_or(&callee);
                let count = if code == "abs" { 1 } else { 2 };
                if args.len() != count || kinds.iter().any(|k| !k.numeric()) {
                    return Err(self.error(
                        "Precision min/max/abs need explicit numeric arguments.",
                        "Pass two numbers to min/max, or one number to abs.",
                    ));
                }
                let kind = numeric();
                let operation = self.helper(
                    "__ppu_precision_op",
                    vec![
                        simple(ast::SimpleExpression::String(name(code))),
                        args[0].clone(),
                        args.get(1).cloned().unwrap_or(integer(0)),
                        integer((kind == Kind::Fixed) as i64),
                        integer(self.line as i64),
                    ],
                );
                let ast::HeadExpression::Simple(ast::SimpleExpression::Suffixed(replacement)) =
                    *operation.head
                else {
                    unreachable!()
                };
                *s = replacement;
                return Ok(kind);
            }
            return match callee.as_str() {
                "floor"|"math.floor"=>Ok(Kind::Int),
                "abs"|"math.abs"|"min"|"max"|"math.min"|"math.max"=>Ok(numeric()),
                "rgb"|"rgb15"|"lobyte"|"hibyte"=>Ok(Kind::Int),
                "error"|"assert"|"print"|"kon"|"koff"|"sfx"=>Ok(Kind::Other),
                _=>binding.and_then(|b|b.returns).map(|r|r.first().copied().unwrap_or(Kind::Other)).ok_or_else(||self.error(&format!("Call {callee:?} is outside the precision preview surface."),"Use a previously defined named numeric helper or min/max/abs/floor. Keep nonlinear f64 math inside a pure ---@snes lookup helper.")),
            };
        }
        if let Some(b) = binding.filter(|b| b.capacity.is_some()) {
            if s.suffixes.len() != 1 {
                return Err(self.error(
                    "Numeric array accesses need one index.",
                    "Use A[i] with no nested field access.",
                ));
            }
            let ast::SuffixPart::Field(ast::FieldSuffix::Indexed(index)) = &mut s.suffixes[0]
            else {
                return Err(self.error(
                    "Numeric arrays need integer indexes.",
                    "Use A[i] rather than named fields.",
                ));
            };
            if self.expr(index)? != Kind::Int {
                return Err(self.error(
                    "A numeric array index must be an integer.",
                    "Use floor(index) within the array capacity.",
                ));
            }
            let e = self.helper(
                "__ppu_precision_get",
                vec![
                    variable(name(&n)),
                    index.clone(),
                    integer(b.capacity.unwrap() as i64),
                    integer(self.line as i64),
                ],
            );
            let ast::HeadExpression::Simple(ast::SimpleExpression::Suffixed(replacement)) = *e.head
            else {
                unreachable!()
            };
            *s = replacement;
            return Ok(b.kind);
        }
        if let Some(kind) = self.record_head(s)? {
            return Ok(kind);
        }
        self.hardware(s)?;
        Ok(if n == "m7" {
            Kind::Fixed
        } else if matches!(s.suffixes.last(),Some(ast::SuffixPart::Field(ast::FieldSuffix::Named(f))) if matches!(text(f).as_str(),"on"|"visible"|"a"|"b"|"left"|"right"|"up"|"down"))
        {
            Kind::Bool
        } else {
            Kind::Int
        })
    }
}
fn constant_expression(e: &Expr) -> bool {
    (match e.head.as_ref() {
        ast::HeadExpression::Simple(
            ast::SimpleExpression::Integer(_) | ast::SimpleExpression::Float(_),
        ) => true,
        ast::HeadExpression::Simple(ast::SimpleExpression::Suffixed(ast::SuffixedExpression {
            primary: ast::PrimaryExpression::GroupedExpression(e),
            suffixes,
        })) => suffixes.is_empty() && constant_expression(e),
        ast::HeadExpression::UnaryOperator(ast::UnaryOperator::Minus, e) => constant_expression(e),
        _ => false,
    }) && e.tail.iter().all(|(op, e)| {
        matches!(
            op,
            ast::BinaryOperator::Add
                | ast::BinaryOperator::Sub
                | ast::BinaryOperator::Mul
                | ast::BinaryOperator::Div
                | ast::BinaryOperator::Pow
                | ast::BinaryOperator::Mod
                | ast::BinaryOperator::IDiv
        ) && constant_expression(e)
    })
}

#[derive(Debug)]
pub(crate) struct Failure {
    pub file: String,
    pub line: u32,
    pub message: String,
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for Failure {}

/// Install only in a fresh precision VM; helpers are never added to a legacy VM.
pub(crate) fn install(ctx: piccolo::Context<'_>) {
    use piccolo::{Callback, CallbackReturn, Value};
    fn fail<'gc>(
        _ctx: piccolo::Context<'gc>,
        line: i64,
        file: &str,
        message: &str,
    ) -> piccolo::Error<'gc> {
        Failure { file: file.into(), line: line as u32, message: format!("SNES precision {message} at source line {line}; use bounded numeric operands and explicit stores") }.into()
    }
    fn raw(v: Value<'_>, fixed: bool) -> Option<i32> {
        let n = v.to_number()? * if fixed { 1024. } else { 1. };
        (n.is_finite() && n.fract() == 0. && n >= i32::MIN as f64 && n <= i32::MAX as f64)
            .then_some(n as i32)
    }
    let op = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let file = stack.get(stack.len() - 1).to_string();
        let code = stack.get(0);
        let code = if let Value::String(s) = code {
            s.as_bytes()
        } else {
            b""
        };
        let fixed = stack.get(3).to_integer() == Some(1);
        let line = stack.get(4).to_integer().unwrap_or(0);
        let a = raw(stack.get(1), fixed).ok_or_else(|| {
            fail(
                ctx,
                line,
                &file,
                "operand exceeds its numeric representation",
            )
        })? as i64;
        let b = raw(stack.get(2), fixed).ok_or_else(|| {
            fail(
                ctx,
                line,
                &file,
                "operand exceeds its numeric representation",
            )
        })? as i64;
        let comparison = match code {
            b"eq" => Some(a == b),
            b"ne" => Some(a != b),
            b"lt" => Some(a < b),
            b"le" => Some(a <= b),
            b"gt" => Some(a > b),
            b"ge" => Some(a >= b),
            _ => None,
        };
        if let Some(result) = comparison {
            stack.replace(ctx, Value::Boolean(result));
            return Ok(CallbackReturn::Return);
        }
        if b == 0 && matches!(code, b"div" | b"idiv" | b"mod") {
            return Err(fail(ctx, line, &file, "division by zero"));
        }
        let scale = if fixed { 1024 } else { 1 };
        let n = match code {
            b"add" => a + b,
            b"sub" => a - b,
            b"neg" => -a,
            b"abs" => a.abs(),
            b"min" => a.min(b),
            b"max" => a.max(b),
            b"mul" => a * b / scale,
            b"div" => a * 1024 / b,
            b"idiv" => {
                let q = a / b;
                q - if a % b != 0 && (a < 0) != (b < 0) {
                    1
                } else {
                    0
                }
            }
            b"mod" => {
                let r = a % b;
                if r != 0 && (a < 0) != (b < 0) {
                    r + b
                } else {
                    r
                }
            }
            _ => return Err(fail(ctx, line, &file, "unknown arithmetic operation")),
        };
        let n = i32::try_from(n)
            .map_err(|_| fail(ctx, line, &file, "arithmetic exceeds signed 32-bit storage"))?;
        let result = if fixed && code != b"idiv" {
            Value::Number(n as f64 / 1024.)
        } else {
            Value::Integer(n as i64)
        };
        stack.replace(ctx, result);
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("__ppu_precision_op", op).unwrap();
    fn stored<'gc>(
        ctx: piccolo::Context<'gc>,
        file: &str,
        line: i64,
        input: Value<'gc>,
        n: u16,
        lo: i64,
        hi: i64,
        binding: Value<'gc>,
    ) -> Result<Value<'gc>, piccolo::Error<'gc>> {
        if n == 0 {
            let value = raw(input, false).ok_or_else(|| {
                fail(
                    ctx,
                    line,
                    file,
                    "integer store needs a signed 32-bit integer",
                )
            })?;
            if (value as i64) < lo || (value as i64) > hi {
                return Err(fail(
                    ctx,
                    line,
                    file,
                    &format!("range contract for {binding}: {value} is outside {lo}..{hi}"),
                ));
            }
            return Ok(Value::Integer(value as i64));
        }
        let input =
            raw(input, true).ok_or_else(|| fail(ctx, line, file, "store needs a Q21.10 number"))?;
        let value =
            snap(input, n).ok_or_else(|| fail(ctx, line, file, "rounded store exceeds Q21.10"))?;
        if (value as i64) < lo * 1024 || (value as i64) > hi * 1024 {
            return Err(fail(
                ctx,
                line,
                file,
                &format!(
                    "range contract for {binding}: rounded value {} is outside {lo}..{hi}",
                    value as f64 / 1024.
                ),
            ));
        }
        Ok(Value::Number(value as f64 / 1024.))
    }
    let store = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let file = stack.get(stack.len() - 1).to_string();
        let line = stack.get(4).to_integer().unwrap_or(0);
        let value = stored(
            ctx,
            &file,
            line,
            stack.get(0),
            stack.get(1).to_integer().unwrap_or(1024) as u16,
            stack.get(2).to_integer().unwrap_or(0),
            stack.get(3).to_integer().unwrap_or(0),
            stack.get(5),
        )?;
        stack.replace(ctx, value);
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("__ppu_precision_store", store).unwrap();
    let parallel = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let file = stack.get(stack.len() - 1).to_string();
        let line = stack.get(stack.len() - 2).to_integer().unwrap_or(0);
        let mut values = vec![];
        for i in (0..stack.len() - 2).step_by(5) {
            let n = stack.get(i + 1).to_integer().unwrap_or(-1);
            values.push(if n < 0 {
                stack.get(i)
            } else {
                stored(
                    ctx,
                    &file,
                    line,
                    stack.get(i),
                    n as u16,
                    stack.get(i + 2).to_integer().unwrap_or(0),
                    stack.get(i + 3).to_integer().unwrap_or(0),
                    stack.get(i + 4),
                )?
            });
        }
        stack.clear();
        stack.extend(values);
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("__ppu_precision_parallel", parallel)
        .unwrap();
    for get in [false, true] {
        let cb = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let file = stack.get(stack.len() - 1).to_string();
            let offset = usize::from(get);
            let line = stack.get(offset + 2).to_integer().unwrap_or(0);
            let index = stack
                .get(offset)
                .to_integer()
                .ok_or_else(|| fail(ctx, line, &file, "array index is not an integer"))?;
            let cap = stack.get(offset + 1).to_integer().unwrap_or(0);
            if !(1..=cap).contains(&index) {
                return Err(fail(
                    ctx,
                    line,
                    &file,
                    "array index is outside its capacity",
                ));
            }
            let result = if get {
                let Value::Table(table) = stack.get(0) else {
                    return Err(fail(ctx, line, &file, "array binding is not a table"));
                };
                let v = table.get(ctx, index);
                if v.is_nil() {
                    return Err(fail(ctx, line, &file, "uninitialized array element"));
                }
                v
            } else {
                Value::Integer(index)
            };
            stack.replace(ctx, result);
            Ok(CallbackReturn::Return)
        });
        ctx.set_global(
            if get {
                "__ppu_precision_get"
            } else {
                "__ppu_precision_index"
            },
            cb,
        )
        .unwrap();
    }
    let time = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let file = stack.get(stack.len() - 1).to_string();
        let line = stack.get(1).to_integer().unwrap_or(0);
        let f = stack
            .get(0)
            .to_integer()
            .filter(|f| *f >= 0 && *f <= i32::MAX as i64)
            .ok_or_else(|| {
                fail(
                    ctx,
                    line,
                    &file,
                    "frame counter needs a nonnegative integer",
                )
            })?;
        let t = (f / 60) * 1024 + (f % 60) * 1024 / 60;
        if t > i32::MAX as i64 {
            return Err(fail(ctx, line, &file, "frame time exceeds Q21.10"));
        }
        stack.replace(ctx, Value::Number(t as f64 / 1024.));
        Ok(CallbackReturn::Return)
    });
    ctx.set_global("__ppu_precision_time", time).unwrap();
}
