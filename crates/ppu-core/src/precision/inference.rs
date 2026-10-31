//! Whole-program representation facts for the precision rewriter, without executing Lua.
use super::*;

type Slot = usize;
#[derive(Default)]
struct Shape {
    parent: usize,
    fields: HashMap<Key, Slot>,
}
#[derive(Default)]
struct Function {
    parameters: Vec<Slot>,
    returns: Vec<Slot>,
    defined: bool,
}
#[derive(Clone)]
struct Name {
    slot: Slot,
    annotation: Option<Annotation>,
    capacity: Option<usize>,
    function: Option<usize>,
    defined: bool,
}
#[derive(Clone)]
enum Constraint {
    Copy(Slot, Slot),
    Field(Slot, Key, Slot),
    Arithmetic(Slot, Slot, Slot, bool),
    Select(Slot, Slot, Slot),
    Call(usize, Vec<Slot>, Vec<Slot>, Option<&'static str>),
}

/// Addresses identify nodes only between analysis and rewriting of the same owned AST.
/// Rewriting must not clone or reorder unvisited declarations.
#[derive(Default)]
pub(super) struct Facts {
    pub globals: HashMap<String, Binding>,
    pub locals: HashMap<usize, Vec<Binding>>,
    pub functions: HashMap<usize, Binding>,
    pub constructors: HashMap<usize, usize>,
    pub records: Vec<Record>,
}
#[derive(Default)]
struct Infer {
    slots: Vec<Option<Kind>>,
    declared: HashMap<Slot, Kind>,
    shapes: Vec<Shape>,
    functions: Vec<Function>,
    scopes: Vec<HashMap<String, Name>>,
    constraints: Vec<Constraint>,
    locals: HashMap<usize, Vec<Name>>,
    definitions: HashMap<usize, usize>,
    constructors: HashMap<usize, usize>,
    current_function: Option<usize>,
    annotations: Vec<Annotation>,
    lookup_lines: Vec<u64>,
    line: u64,
    changed: bool,
}
impl Infer {
    fn slot(&mut self, kind: Option<Kind>) -> Slot {
        let id = self.slots.len();
        self.slots.push(kind);
        id
    }
    fn lookup(&mut self, n: &str) -> Name {
        if let Some(v) = self.scopes.iter().rev().find_map(|s| s.get(n)).cloned() {
            return v;
        }
        let value = Name {
            slot: self.slot(None),
            annotation: None,
            capacity: None,
            function: None,
            defined: false,
        };
        self.scopes[0].insert(n.into(), value.clone());
        value
    }
    fn declaration(&mut self, n: &str) -> Name {
        let a = self
            .annotations
            .iter()
            .find(|a| a.line == self.line && a.name == n)
            .cloned();
        let kind = a
            .as_ref()
            .map(|a| if a.fixed { Kind::Fixed } else { Kind::Int });
        let slot = self.slot(kind);
        if let Some(kind) = kind {
            self.declared.insert(slot, kind);
        }
        Name {
            slot,
            capacity: a.as_ref().and_then(|a| a.capacity),
            annotation: a,
            function: None,
            defined: true,
        }
    }
    fn copy(&mut self, target: Slot, value: Slot) {
        self.constraints.push(Constraint::Copy(target, value));
    }
    fn root(&self, mut id: usize) -> usize {
        while self.shapes[id].parent != id {
            id = self.shapes[id].parent;
        }
        id
    }
    fn shape(&mut self) -> usize {
        let id = self.shapes.len();
        self.shapes.push(Shape {
            parent: id,
            ..Shape::default()
        });
        id
    }
    fn field(&mut self, id: usize, key: Key) -> Slot {
        let id = self.root(id);
        let key = Transform::table_key(&self.shapes[id].fields, key);
        if let Some(slot) = self.shapes[id].fields.get(&key) {
            return *slot;
        }
        let slot = self.slot(None);
        self.shapes[id].fields.insert(key, slot);
        self.changed = true;
        slot
    }
    fn merge(&mut self, a: usize, b: usize) -> usize {
        let (a, b) = (self.root(a), self.root(b));
        if a == b {
            return a;
        }
        let (a, b) = (a.min(b), a.max(b));
        self.shapes[b].parent = a;
        let fields = std::mem::take(&mut self.shapes[b].fields);
        for (key, value) in fields {
            if let Some(&other) = self.shapes[a].fields.get(&key) {
                self.copy(other, value);
                self.copy(value, other);
            } else {
                self.shapes[a].fields.insert(key, value);
            }
        }
        self.changed = true;
        a
    }
    fn set(&mut self, slot: Slot, kind: Kind) {
        let kind = if let Some(&declared) = self.declared.get(&slot) {
            declared
        } else {
            match (self.slots[slot], kind) {
                (Some(Kind::Int), Kind::Fixed) | (Some(Kind::Fixed), Kind::Int) => Kind::Fixed,
                (Some(Kind::Table(a)), Kind::Table(b)) => Kind::Table(self.merge(a, b)),
                (Some(previous), _) => previous,
                _ => kind,
            }
        };
        if self.slots[slot] != Some(kind) {
            self.slots[slot] = Some(kind);
            self.changed = true;
        }
    }
    fn key(field: &ast::FieldSuffix<Text>) -> Key {
        match field {
            ast::FieldSuffix::Named(n) => Key::Named(text(n)),
            ast::FieldSuffix::Indexed(e) if e.tail.is_empty() => match e.head.as_ref() {
                ast::HeadExpression::Simple(ast::SimpleExpression::String(n)) => {
                    Key::Named(text(n))
                }
                ast::HeadExpression::Simple(ast::SimpleExpression::Integer(n)) => Key::Position(*n),
                _ => Key::Element,
            },
            _ => Key::Element,
        }
    }
    fn project(&mut self, owner: Slot, key: Key) -> Slot {
        let result = self.slot(None);
        self.constraints.push(Constraint::Field(owner, key, result));
        result
    }
    fn head(&mut self, s: &ast::SuffixedExpression<Text>) -> Slot {
        if let ast::PrimaryExpression::Name(n) = &s.primary {
            let n = text(n);
            if !self.lookup(&n).defined
                && matches!(
                    n.as_str(),
                    "obj"
                        | "bg"
                        | "m7"
                        | "vram"
                        | "cgram"
                        | "raster"
                        | "pad"
                        | "voice"
                        | "dsp"
                        | "win"
                        | "color"
                        | "screen"
                )
            {
                for suffix in &s.suffixes {
                    if let ast::SuffixPart::Field(ast::FieldSuffix::Indexed(e)) = suffix {
                        self.expr(e);
                    }
                }
                let kind = if n == "m7" {
                    Kind::Fixed
                } else if matches!(s.suffixes.last(),Some(ast::SuffixPart::Field(ast::FieldSuffix::Named(f))) if matches!(text(f).as_str(),"on"|"visible"|"a"|"b"|"left"|"right"|"up"|"down"))
                {
                    Kind::Bool
                } else {
                    Kind::Int
                };
                return self.slot(Some(kind));
            }
        }
        let mut value = match &s.primary {
            ast::PrimaryExpression::Name(n) => self.lookup(&text(n)).slot,
            ast::PrimaryExpression::GroupedExpression(e) => self.expr(e),
        };
        let array = matches!(&s.primary,ast::PrimaryExpression::Name(n) if self.lookup(&text(n)).capacity.is_some());
        for suffix in &s.suffixes {
            if let ast::SuffixPart::Field(f) = suffix {
                if let ast::FieldSuffix::Indexed(e) = f {
                    self.expr(e);
                }
                if !array {
                    value = self.project(value, Self::key(f));
                }
            }
        }
        value
    }
    fn callable(&mut self, n: &str) -> usize {
        let v = self.lookup(n);
        if let Some(id) = v.function {
            return id;
        }
        let id = self.functions.len();
        self.functions.push(Function::default());
        for scope in self.scopes.iter_mut().rev() {
            if let Some(v) = scope.get_mut(n) {
                v.function = Some(id);
                break;
            }
        }
        id
    }
    fn call(&mut self, n: &str, args: &[Expr], count: usize) -> Vec<Slot> {
        let args: Vec<_> = args.iter().map(|a| self.expr(a)).collect();
        let out: Vec<_> = (0..count.max(1)).map(|_| self.slot(None)).collect();
        let builtin = match n {
            "floor" | "math.floor" | "rgb" | "rgb15" | "lobyte" | "hibyte" => Some("integer"),
            "min" | "max" | "abs" | "math.min" | "math.max" | "math.abs" => Some("numeric"),
            _ => None,
        };
        let id = self.callable(n);
        self.constraints
            .push(Constraint::Call(id, args, out.clone(), builtin));
        out
    }
    fn values(&mut self, values: &[Expr], count: usize) -> Vec<Slot> {
        let mut result = vec![];
        for (i, value) in values.iter().enumerate() {
            if i + 1 == values.len() && value.tail.is_empty() {
                if let ast::HeadExpression::Simple(ast::SimpleExpression::Suffixed(s)) =
                    value.head.as_ref()
                {
                    if let (
                        ast::PrimaryExpression::Name(n),
                        [ast::SuffixPart::Call(ast::CallSuffix::Function(args))],
                    ) = (&s.primary, s.suffixes.as_slice())
                    {
                        result.extend(self.call(&text(n), args, count.saturating_sub(i)));
                        continue;
                    }
                }
            }
            // A stored nil leaves the target untyped: its later stores decide its representation.
            result.push(if nil(value) { self.slot(None) } else { self.expr(value) });
        }
        result
    }
    fn expr(&mut self, e: &Expr) -> Slot {
        use ast::{BinaryOperator as B, HeadExpression as H, SimpleExpression as S};
        let mut left = match e.head.as_ref() {
            H::Simple(S::Integer(_)) => self.slot(Some(Kind::Int)),
            H::Simple(S::Float(_)) => self.slot(Some(Kind::Fixed)),
            H::Simple(S::True | S::False) => self.slot(Some(Kind::Bool)),
            H::Simple(S::String(_)) => self.slot(Some(Kind::String)),
            H::Simple(S::TableConstructor(t)) => {
                let id = self.shape();
                self.constructors.insert(t as *const _ as usize, id);
                let mut position = 0;
                for field in &t.fields {
                    let (key, value) = match field {
                        ast::ConstructorField::Array(e) => {
                            position += 1;
                            (Key::Position(position), e)
                        }
                        ast::ConstructorField::Record(ast::RecordKey::Named(n), e) => {
                            (Key::Named(text(n)), e)
                        }
                        ast::ConstructorField::Record(ast::RecordKey::Indexed(k), e) => {
                            self.expr(k);
                            (Self::key(&ast::FieldSuffix::Indexed(k.clone())), e)
                        }
                    };
                    let value = self.expr(value);
                    let target = self.field(id, key);
                    self.copy(target, value);
                }
                self.slot(Some(Kind::Table(id)))
            }
            H::Simple(S::Suffixed(s)) => {
                let call = match (&s.primary, s.suffixes.as_slice()) {
                    (
                        ast::PrimaryExpression::Name(n),
                        [ast::SuffixPart::Call(ast::CallSuffix::Function(args))],
                    ) => Some((text(n), args)),
                    (
                        ast::PrimaryExpression::Name(n),
                        [ast::SuffixPart::Field(ast::FieldSuffix::Named(f)), ast::SuffixPart::Call(ast::CallSuffix::Function(args))],
                    ) if text(n) == "math" => Some((format!("math.{}", text(f)), args)),
                    _ => None,
                };
                if let Some((n, args)) = call {
                    self.call(&n, args, 1)[0]
                } else {
                    self.head(s)
                }
            }
            H::UnaryOperator(op, value) => {
                let input = self.expr(value);
                match op {
                    ast::UnaryOperator::Not => self.slot(Some(Kind::Bool)),
                    ast::UnaryOperator::Len => self.slot(Some(Kind::Int)),
                    _ => {
                        let output = self.slot(None);
                        self.copy(output, input);
                        output
                    }
                }
            }
            _ => self.slot(Some(Kind::Other)),
        };
        for (op, right) in &e.tail {
            let right = self.expr(right);
            let output = self.slot(None);
            match op {
                B::Equal
                | B::NotEqual
                | B::LessThan
                | B::LessEqual
                | B::GreaterThan
                | B::GreaterEqual => self.set(output, Kind::Bool),
                B::And | B::Or => self
                    .constraints
                    .push(Constraint::Select(output, left, right)),
                B::IDiv | B::BitAnd | B::BitOr | B::BitXor | B::ShiftLeft | B::ShiftRight => {
                    self.set(output, Kind::Int)
                }
                _ => self.constraints.push(Constraint::Arithmetic(
                    output,
                    left,
                    right,
                    *op == B::Div,
                )),
            }
            left = output;
        }
        left
    }
    fn function(&mut self, n: &str, f: &ast::FunctionDefinition<Text>, local: bool) {
        let id = if local {
            let id = self.functions.len();
            self.functions.push(Function::default());
            let mut v = self.declaration(n);
            v.function = Some(id);
            self.scopes.last_mut().unwrap().insert(n.into(), v);
            id
        } else {
            self.callable(n)
        };
        self.functions[id].defined = true;
        self.definitions.insert(f as *const _ as usize, id);
        for scope in self.scopes.iter_mut().rev() {
            if let Some(v) = scope.get_mut(n) {
                v.defined = true;
                break;
            }
        }
        let previous = self.current_function.replace(id);
        self.scopes.push(HashMap::new());
        for (i, p) in f.parameters.iter().enumerate() {
            let mut v = self.declaration(&text(p));
            if n == "frame" {
                self.set(v.slot, if i == 0 { Kind::Fixed } else { Kind::Int });
            }
            v.defined = true;
            self.functions[id].parameters.push(v.slot);
            self.scopes.last_mut().unwrap().insert(text(p), v);
        }
        if self.lookup_lines.contains(&self.line) {
            let result = self.slot(Some(Kind::Int));
            self.functions[id].returns.push(result);
        } else {
            self.block(&f.body, false);
        }
        self.scopes.pop();
        self.current_function = previous;
    }
    fn block(&mut self, b: &Block, scoped: bool) {
        if scoped {
            self.scopes.push(HashMap::new());
        }
        for s in &b.statements {
            self.line = s.line_number.0 + 1;
            use ast::Statement as S;
            match &s.inner {
                S::LocalStatement(s) => {
                    let values = self.values(&s.values, s.names.len());
                    let mut names = vec![];
                    for (i, n) in s.names.iter().enumerate() {
                        let v = self.declaration(&text(n));
                        if v.capacity.is_none() {
                            if let Some(&value) = values.get(i) {
                                self.copy(v.slot, value);
                            }
                        }
                        self.scopes.last_mut().unwrap().insert(text(n), v.clone());
                        names.push(v);
                    }
                    self.locals.insert(s as *const _ as usize, names);
                }
                S::Assignment(s) => {
                    let targets:Vec<_>=s.targets.iter().map(|t|match t {
                        ast::AssignmentTarget::Name(n)=>{
                            let mut v=self.lookup(&text(n));
                            if let Some(a)=self.annotations.iter().find(|a|a.line==self.line && a.name==text(n)).cloned() {
                                self.declared.insert(v.slot,if a.fixed {Kind::Fixed} else {Kind::Int});
                                v.capacity=a.capacity;v.annotation=Some(a);
                            }
                            v.defined=true;
                            let scope=self.scopes.iter_mut().rev().find(|s|s.contains_key(&text(n))).unwrap();
                            scope.insert(text(n),v.clone());
                            v.slot
                        },
                        ast::AssignmentTarget::Field(head,field)=>{
                            let array=matches!(&head.primary,ast::PrimaryExpression::Name(n) if head.suffixes.is_empty() && self.lookup(&text(n)).capacity.is_some());
                            if array {self.head(head)} else {let owner=self.head(head);self.project(owner,Self::key(field))}
                        },
                    }).collect();
                    let values = self.values(&s.values, s.targets.len());
                    for (target, value) in targets.into_iter().zip(values) {
                        self.copy(target, value);
                    }
                }
                S::Function(s) if s.fields.is_empty() && s.method.is_none() => {
                    self.function(&text(&s.name), &s.definition, false)
                }
                S::LocalFunction(s) => self.function(&text(&s.name), &s.definition, true),
                S::Do(b) => self.block(b, true),
                S::If(s) => {
                    self.expr(&s.if_part.0);
                    self.block(&s.if_part.1, true);
                    for (e, b) in &s.else_if_parts {
                        self.expr(e);
                        self.block(b, true);
                    }
                    if let Some(b) = &s.else_part {
                        self.block(b, true);
                    }
                }
                S::While(s) => {
                    self.expr(&s.condition);
                    self.block(&s.block, true);
                }
                S::Repeat(s) => {
                    self.scopes.push(HashMap::new());
                    self.block(&s.body, false);
                    self.expr(&s.until);
                    self.scopes.pop();
                }
                S::For(ast::ForStatement::Numeric {
                    name,
                    initial,
                    limit,
                    step,
                    body,
                }) => {
                    self.expr(initial);
                    self.expr(limit);
                    if let Some(e) = step {
                        self.expr(e);
                    }
                    let v = Name {
                        slot: self.slot(Some(Kind::Int)),
                        annotation: None,
                        capacity: None,
                        function: None,
                        defined: true,
                    };
                    self.scopes.push(HashMap::from([(text(name), v)]));
                    self.block(body, false);
                    self.scopes.pop();
                }
                S::FunctionCall(s) => {
                    if let (ast::PrimaryExpression::Name(n), ast::CallSuffix::Function(args)) =
                        (&s.head.primary, &s.call)
                    {
                        if s.head.suffixes.is_empty() {
                            self.call(&text(n), args, 0);
                        }
                    }
                }
                _ => {}
            }
        }
        if let (Some(id), Some(r)) = (self.current_function, &b.return_statement) {
            let values = self.values(&r.inner.returns, r.inner.returns.len());
            while self.functions[id].returns.len() < values.len() {
                let slot = self.slot(None);
                self.functions[id].returns.push(slot);
            }
            let targets = self.functions[id].returns.clone();
            for (target, value) in targets.into_iter().zip(values) {
                self.copy(target, value);
            }
        }
        if scoped {
            self.scopes.pop();
        }
    }
    fn compatible(&self, a: Kind, b: Kind, seen: &mut Vec<(usize, usize)>) -> bool {
        if a == b || a.numeric() && b.numeric() {
            return true;
        }
        let (Kind::Table(a), Kind::Table(b)) = (a, b) else {
            return false;
        };
        let (a, b) = (self.root(a), self.root(b));
        if a == b || seen.contains(&(a, b)) {
            return true;
        }
        seen.push((a, b));
        let (a, b) = (&self.shapes[a].fields, &self.shapes[b].fields);
        a.len() == b.len()
            && a.iter().all(|(key, &slot)| {
                b.get(key)
                    .is_some_and(|&other| match (self.slots[slot], self.slots[other]) {
                        (Some(a), Some(b)) => self.compatible(a, b, seen),
                        _ => false,
                    })
            })
    }
    fn normalize(&mut self) {
        for id in (0..self.shapes.len()).rev() {
            if self.root(id) != id {
                continue;
            }
            let positions: Vec<_> = self.shapes[id]
                .fields
                .iter()
                .filter_map(|(k, &v)| matches!(k, Key::Position(_)).then_some((k.clone(), v)))
                .collect();
            if let Some(&(_, first)) = positions.first() {
                if positions.iter().all(|(_,slot)|matches!((self.slots[first],self.slots[*slot]),(Some(a),Some(b)) if self.compatible(a,b,&mut vec![]))) {
                    let element=self.field(id,Key::Element);
                    for (key,slot) in positions {self.copy(element,slot);self.copy(slot,element);self.shapes[id].fields.remove(&key);}
                    self.changed=true;
                }
            }
        }
    }
    fn solve(&mut self) {
        loop {
            self.changed = false;
            let constraints = self.constraints.clone();
            for c in constraints {
                match c {
                    Constraint::Copy(to, from) => {
                        if let Some(kind) = self.slots[from] {
                            self.set(to, kind);
                        }
                    }
                    Constraint::Field(owner, key, out) => {
                        if let Some(Kind::Table(id)) = self.slots[owner] {
                            let slot = self.field(id, key);
                            if let Some(k) = self.slots[slot] {
                                self.set(out, k);
                            }
                            if let Some(k) = self.slots[out] {
                                self.set(slot, k);
                            }
                        }
                    }
                    Constraint::Arithmetic(out, a, b, fixed) => {
                        let kind = if fixed
                            || self.slots[a] == Some(Kind::Fixed)
                            || self.slots[b] == Some(Kind::Fixed)
                        {
                            Some(Kind::Fixed)
                        } else if self.slots[a] == Some(Kind::Int)
                            && self.slots[b] == Some(Kind::Int)
                        {
                            Some(Kind::Int)
                        } else {
                            None
                        };
                        if let Some(k) = kind {
                            self.set(out, k);
                        }
                    }
                    Constraint::Select(out, a, b) => {
                        if let (Some(a), Some(b)) = (self.slots[a], self.slots[b]) {
                            self.set(
                                out,
                                if a == Kind::Bool && b != Kind::Bool {
                                    b
                                } else if a.numeric() && b.numeric() {
                                    a.pair(b)
                                } else {
                                    a
                                },
                            );
                        }
                    }
                    Constraint::Call(id, args, out, builtin) => {
                        if !self.functions[id].defined {
                            if builtin == Some("integer") {
                                self.set(out[0], Kind::Int);
                            } else if builtin == Some("numeric") && !args.is_empty() {
                                if args.iter().any(|&a| self.slots[a] == Some(Kind::Fixed)) {
                                    self.set(out[0], Kind::Fixed);
                                } else if args.iter().all(|&a| self.slots[a] == Some(Kind::Int)) {
                                    self.set(out[0], Kind::Int);
                                }
                            }
                            continue;
                        }
                        let f = &self.functions[id];
                        let parameters = f.parameters.clone();
                        let returns = f.returns.clone();
                        for (p, a) in parameters.into_iter().zip(args) {
                            if let Some(k) = self.slots[a] {
                                self.set(p, k);
                            }
                        }
                        for (o, r) in out.into_iter().zip(returns) {
                            if let Some(k) = self.slots[r] {
                                self.set(o, k);
                            }
                        }
                    }
                }
            }
            self.normalize();
            if !self.changed {
                break;
            }
        }
    }
    fn kind(&self, slot: Slot) -> Kind {
        match self.slots[slot].unwrap_or(Kind::Other) {
            Kind::Table(id) => Kind::Table(self.root(id)),
            k => k,
        }
    }
    fn binding(&self, v: &Name) -> Binding {
        let (parameters, returns) = v
            .function
            .filter(|id| self.functions[*id].defined)
            .map(|id| {
                let f = &self.functions[id];
                (
                    Some(f.parameters.iter().map(|&s| self.kind(s)).collect()),
                    Some(f.returns.iter().map(|&s| self.kind(s)).collect()),
                )
            })
            .unwrap_or((None, None));
        Binding {
            kind: self.kind(v.slot),
            annotation: v.annotation.clone(),
            capacity: v.capacity,
            parameters,
            returns,
        }
    }
    fn facts(self) -> Facts {
        let globals = self.scopes[0]
            .iter()
            .filter(|(_, v)| v.defined)
            .map(|(n, v)| (n.clone(), self.binding(v)))
            .collect();
        let locals = self
            .locals
            .iter()
            .map(|(k, v)| (*k, v.iter().map(|v| self.binding(v)).collect()))
            .collect();
        let functions = self
            .definitions
            .iter()
            .map(|(k, &id)| {
                let f = &self.functions[id];
                (
                    *k,
                    Binding {
                        parameters: Some(f.parameters.iter().map(|&s| self.kind(s)).collect()),
                        returns: Some(f.returns.iter().map(|&s| self.kind(s)).collect()),
                        ..Binding::scalar(Kind::Other)
                    },
                )
            })
            .collect();
        let constructors = self
            .constructors
            .iter()
            .map(|(p, &id)| (*p, self.root(id)))
            .collect();
        let records = (0..self.shapes.len())
            .map(|id| Record {
                fields: self.shapes[self.root(id)]
                    .fields
                    .iter()
                    .map(|(k, &v)| (k.clone(), self.kind(v)))
                    .collect(),
            })
            .collect();
        Facts {
            globals,
            locals,
            functions,
            constructors,
            records,
        }
    }
}

pub(super) fn infer(
    chunks: &[ast::Chunk<Text>],
    annotations: &[Vec<Annotation>],
    files: &[(&str, &str)],
) -> Facts {
    let mut i = Infer {
        scopes: vec![HashMap::new()],
        ..Infer::default()
    };
    for ((chunk, annotations), (_, source)) in chunks.iter().zip(annotations).zip(files) {
        i.annotations = annotations.clone();
        i.lookup_lines = lookup_definition_lines(source);
        i.scopes.push(HashMap::new());
        i.block(&chunk.block, false);
        i.scopes.pop();
    }
    i.solve();
    let parameters: Vec<_> = i
        .functions
        .iter()
        .flat_map(|f| f.parameters.iter().copied())
        .collect();
    for slot in parameters {
        if i.slots[slot].is_none() {
            i.set(slot, Kind::Int);
        }
    }
    i.solve();
    i.facts()
}
