//! Hot swap: a recompile carries file-level state from the previous run's VM into the new one.
//!
//! The unit is a file-level local that a top-level function (a direct child of the file's main
//! chunk) captures: its upvalue descriptor names the parent's local, so `(file, name)` is its
//! identity across runs. The new run's init() has already run; each such cell the previous run also
//! had takes the previous value, so functions are the new code and data is the old.
//!
//! Values cross VMs as owned data ([`Owned`]): nil, boolean, integer, number, string, table (deep,
//! shared references and cycles kept, metatable included) and function (named by its place in the
//! file's prototype tree, resolved to the new run's closure there). A table that is the new initial
//! value of a variable is filled in place, so every other holder of it sees the old data.
//!
//! Engine values nested in carried data (a sprites group, a `dma()` result, a `@ppu/hud` field or
//! line, a built-in function)
//! keep the new run's value in their slot; a `makeFixed`/`makeBounded` record carries its values
//! into the new run's record, whose bounds stand. Tables are matched to the new run's by key.
//!
//! shortcut: a name two different locals of one file share is skipped (never carried, never listed),
//! and a closure only a Rust-side hook holds is found only when it is a timer's; upgrade if either
//! loses state in practice.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use piccolo::closure::{UpValue, UpValueState};
use piccolo::compiler::FunctionRef;
use piccolo::types::UpValueDescriptor;
use piccolo::{
    Closure, Context, Executor, Function, Lua, StashedClosure, StashedFunction, StashedTable,
    Table, Value,
};

/// What one finished load leaves for the next: its VM and the handles the carry reads.
pub(crate) struct Run {
    pub lua: Rc<RefCell<Lua>>,
    /// Each program file's main chunk, by file name.
    pub mains: Vec<(String, StashedClosure)>,
    /// Functions Rust holds that Luau globals do not reach (timers).
    pub roots: Vec<StashedFunction>,
    /// The metatable `dma()` and `world.load` put on their results: setup, never carried.
    pub setup_mark: Option<StashedTable>,
}

type Key = (String, String);

#[derive(Clone)]
enum Owned {
    Nil,
    Bool(bool),
    Int(i64),
    Num(f64),
    Str(Vec<u8>),
    Table(usize),
    Func(FnRef),
    /// Setup the engine rebuilds on every load (a `dma()` result, a built-in function, an engine
    /// object): the new run's value stands, and nothing is listed.
    Fresh,
    /// Cannot cross, and the author should know: userdata, a coroutine.
    Opaque(&'static str),
}

/// A function by place: its file, its index path in the file's prototype tree, its name.
#[derive(Clone)]
struct FnRef {
    file: String,
    path: Vec<usize>,
    name: Option<Vec<u8>>,
}

struct OwnedTable {
    pairs: Vec<(Owned, Owned)>,
    meta: Option<usize>,
    /// A `makeFixed`/`makeBounded` record proxy: the table of its values. The proxy itself is
    /// empty and its metatable is the engine's, so only the values travel.
    record: Option<usize>,
    /// That proxy's contracts (plain data): a record with no counterpart is rebuilt from them.
    contracts: Option<usize>,
}

/// A run's file-level state as owned data: what [`capture`] takes and [`apply`] puts back.
#[derive(Default)]
pub(crate) struct Snap {
    tables: Vec<OwnedTable>,
    values: Vec<(Key, Owned)>,
}

struct Proto {
    file: String,
    path: Vec<usize>,
    name: Option<Vec<u8>>,
}

struct Found<'gc> {
    cells: HashMap<Key, (usize, Option<UpValue<'gc>>)>,
    /// The first live closure of each prototype.
    closures: HashMap<usize, Closure<'gc>>,
    protos: HashMap<usize, Proto>,
}

fn addr<T>(r: &T) -> usize {
    r as *const T as usize
}

fn name_of(r: &FunctionRef<piccolo::String<'_>>) -> Option<Vec<u8>> {
    match r {
        FunctionRef::Named(n, _) => Some(n.as_bytes().to_vec()),
        _ => None,
    }
}

/// Every closure reachable from the globals (and `run.roots`), and the cells a top-level function
/// captures from its file's main chunk.
fn discover<'gc>(ctx: Context<'gc>, run: &Run) -> Found<'gc> {
    let mut found = Found {
        cells: HashMap::new(),
        closures: HashMap::new(),
        protos: HashMap::new(),
    };
    let mut children: HashMap<usize, String> = HashMap::new();
    for (file, main) in &run.mains {
        let main = ctx.fetch(main);
        let root = main.prototype();
        let mut stack = vec![(&*root, vec![])];
        while let Some((p, path)) = stack.pop() {
            found.protos.insert(
                addr(p),
                Proto {
                    file: file.clone(),
                    path: path.clone(),
                    name: name_of(&p.reference),
                },
            );
            for (i, k) in p.prototypes.iter().enumerate() {
                let mut at = path.clone();
                at.push(i);
                if path.is_empty() {
                    children.insert(addr(&**k), file.clone());
                }
                stack.push((&**k, at));
            }
        }
    }
    let mut tables: HashSet<Table<'gc>> = HashSet::new();
    let mut seen: HashSet<Closure<'gc>> = HashSet::new();
    let mut stack: Vec<Value<'gc>> = vec![Value::Table(ctx.globals())];
    stack.extend(run.roots.iter().map(|f| Value::Function(ctx.fetch(f))));
    while let Some(v) = stack.pop() {
        match v {
            Value::Table(t) => {
                if !tables.insert(t) {
                    continue;
                }
                for (k, v) in t.iter() {
                    stack.push(k);
                    stack.push(v);
                }
                stack.extend(t.metatable().map(Value::Table));
            }
            Value::Function(Function::Closure(c)) => {
                if !seen.insert(c) {
                    continue;
                }
                let p = c.prototype();
                found.closures.entry(addr(&*p)).or_insert(c);
                let file = children.get(&addr(&*p));
                for (i, up) in c.upvalues().iter().enumerate() {
                    if let (Some(file), Some(UpValueDescriptor::ParentLocal(_))) =
                        (file, p.upvalues.get(i))
                    {
                        let name = String::from_utf8_lossy(p.upvalue_names[i].as_bytes());
                        let at = addr(&*up.into_inner());
                        let key = (file.clone(), name.into_owned());
                        match found.cells.get_mut(&key) {
                            Some(cell) if cell.0 != at => cell.1 = None,
                            Some(_) => {}
                            None => {
                                found.cells.insert(key, (at, Some(*up)));
                            }
                        }
                    }
                    if let UpValueState::Closed(v) = up.get() {
                        stack.push(v);
                    }
                }
            }
            _ => {}
        }
    }
    found
}

struct Snapper<'a, 'gc> {
    ctx: Context<'gc>,
    found: &'a Found<'gc>,
    /// The metatable of the tables `dma()` and `world.load` returned.
    mark: Option<Table<'gc>>,
    ids: HashMap<Table<'gc>, usize>,
    queue: Vec<Table<'gc>>,
    snap: Snap,
}

impl<'a, 'gc> Snapper<'a, 'gc> {
    fn id(&mut self, t: Table<'gc>) -> usize {
        if let Some(&i) = self.ids.get(&t) {
            return i;
        }
        let i = self.snap.tables.len();
        self.snap.tables.push(OwnedTable {
            pairs: vec![],
            meta: None,
            record: None,
            contracts: None,
        });
        self.ids.insert(t, i);
        self.queue.push(t);
        i
    }

    /// A `@ppu/hud` field or line (its `set` is the library's): it remembers what its cells show,
    /// and a recompile clears VRAM, so the new run's (drawn by its init) stands, as a `dma()` does.
    fn hud_handle(&self, t: Table<'gc>) -> bool {
        match t.get(self.ctx, "set") {
            Value::Function(Function::Closure(c)) => self
                .found
                .protos
                .get(&addr(&*c.prototype()))
                .is_some_and(|p| p.file == "@ppu/hud.luau"),
            _ => false,
        }
    }

    fn value(&mut self, v: Value<'gc>) -> Owned {
        match v {
            Value::Nil => Owned::Nil,
            Value::Boolean(b) => Owned::Bool(b),
            Value::Integer(i) => Owned::Int(i),
            Value::Number(n) => Owned::Num(n),
            Value::String(s) => Owned::Str(s.as_bytes().to_vec()),
            Value::Table(t) if self.mark.is_some() && t.metatable() == self.mark => Owned::Fresh,
            Value::Table(t) if self.hud_handle(t) => Owned::Fresh,
            Value::Table(t) => Owned::Table(self.id(t)),
            Value::Function(Function::Closure(c)) => {
                match self.found.protos.get(&addr(&*c.prototype())) {
                    Some(p) => Owned::Func(FnRef {
                        file: p.file.clone(),
                        path: p.path.clone(),
                        name: p.name.clone(),
                    }),
                    None => Owned::Fresh,
                }
            }
            Value::Function(_) => Owned::Fresh,
            Value::Thread(_) => Owned::Opaque("a coroutine"),
            Value::UserData(_) => Owned::Opaque("userdata"),
        }
    }

    fn drain(&mut self) {
        while let Some(t) = self.queue.pop() {
            let i = self.ids[&t];
            if let Some(Value::Table(values)) =
                t.metatable().map(|m| m.get(self.ctx, "__ppu_values"))
            {
                let record = Some(self.id(values));
                let contracts = match t.metatable().map(|m| m.get(self.ctx, "__ppu_contracts")) {
                    Some(Value::Table(c)) => Some(self.id(c)),
                    _ => None,
                };
                self.snap.tables[i] = OwnedTable {
                    pairs: vec![],
                    meta: None,
                    record,
                    contracts,
                };
                continue;
            }
            let pairs: Vec<_> = t.iter().collect();
            let pairs = pairs
                .into_iter()
                .map(|(k, v)| (self.value(k), self.value(v)))
                .collect();
            let meta = t.metatable().map(|m| self.id(m));
            self.snap.tables[i] = OwnedTable {
                pairs,
                meta,
                record: None,
                contracts: None,
            };
        }
    }
}

fn kind(o: &Owned) -> &'static str {
    match o {
        Owned::Fresh => "an engine value",
        Owned::Opaque(what) => what,
        Owned::Nil => "nil",
        Owned::Bool(_) => "boolean",
        Owned::Int(_) | Owned::Num(_) => "number",
        Owned::Str(_) => "string",
        Owned::Table(_) => "table",
        Owned::Func(_) => "function",
    }
}

fn kind_of(v: Value<'_>) -> &'static str {
    match v {
        Value::Nil => "nil",
        Value::Boolean(_) => "boolean",
        Value::Integer(_) | Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Table(_) => "table",
        Value::Function(_) => "function",
        Value::Thread(_) => "coroutine",
        Value::UserData(_) => "userdata",
    }
}

struct Apply<'a, 'gc> {
    ctx: Context<'gc>,
    found: &'a Found<'gc>,
    snap: &'a Snap,
    /// The new run's table standing for each old table.
    images: HashMap<usize, Table<'gc>>,
    /// The new run's tables already standing for an old one.
    claimed: HashSet<Table<'gc>>,
    /// A hosted table's own function fields, which the old data never overwrites.
    host_fns: HashMap<usize, Vec<(Value<'gc>, Value<'gc>)>>,
    /// A hosted table's fields as the new run made them, for the slots the old data cannot fill.
    host_all: HashMap<usize, Vec<(Value<'gc>, Value<'gc>)>>,
    filled: HashSet<usize>,
    /// Record proxies built for records with no counterpart, with the old ids of their values and
    /// contracts: each gets the engine's proxy metatable once filled.
    adopt: Vec<(Table<'gc>, usize, usize)>,
    /// The variable being filled, and the lines about it (each at most once).
    at: Key,
    lines: Vec<String>,
}

fn kept(key: &Key, why: &str) -> String {
    format!("{}: {} kept its new value: {why}", key.0, key.1)
}

impl<'a, 'gc> Apply<'a, 'gc> {
    /// The new run's closure for an old function: the same place in the file's prototype tree
    /// when it is the same function by name, else the one function of that name in the file.
    fn resolve(&self, f: &FnRef) -> Option<Closure<'gc>> {
        let same: Vec<_> = self
            .found
            .protos
            .iter()
            .filter(|(_, p)| p.file == f.file && p.name == f.name)
            .filter_map(|(ptr, p)| Some((&p.path, *self.found.closures.get(ptr)?)))
            .collect();
        match same.iter().find(|(path, _)| **path == f.path) {
            Some((_, c)) => Some(*c),
            None if f.name.is_some() && same.len() == 1 => Some(same[0].1),
            None => None,
        }
    }

    /// Why this old value cannot be carried, if it cannot: the first function it holds with no
    /// counterpart (a hosted table's function fields the new table overwrites aside) or the
    /// first value that cannot cross. Engine values do not block: their slot keeps the new one.
    fn blocker(&self, root: &Owned) -> Option<String> {
        let mut seen = HashSet::new();
        let mut todo = vec![root.clone()];
        let bad = |o: &Owned| match o {
            Owned::Func(f) => self
                .resolve(f)
                .is_none()
                .then(|| "it holds a function the new code no longer has".to_string()),
            Owned::Opaque(what) => Some(format!("it holds {what}, which cannot be carried")),
            _ => None,
        };
        while let Some(o) = todo.pop() {
            let Owned::Table(id) = o else {
                if let Some(m) = bad(&o) {
                    return Some(m);
                }
                continue;
            };
            if !seen.insert(id) || self.filled.contains(&id) {
                continue;
            }
            let t = &self.snap.tables[id];
            for (k, v) in &t.pairs {
                let overwritten = self.host_fns.get(&id).is_some_and(|fns| {
                    matches!(v, Owned::Func(_) | Owned::Fresh | Owned::Opaque(_))
                        && fns.iter().any(|(hk, _)| self.same_key(hk, k))
                });
                if !overwritten {
                    todo.push(v.clone());
                }
                todo.push(k.clone());
            }
            if let Some(m) = t.meta {
                todo.push(Owned::Table(m));
            }
            todo.extend(t.record.map(Owned::Table));
            todo.extend(t.contracts.map(Owned::Table));
        }
        None
    }

    /// Whether a host key equals an old key (scalars and strings: the keys methods live under).
    fn same_key(&self, host: &Value<'gc>, old: &Owned) -> bool {
        match (host, old) {
            (Value::String(a), Owned::Str(b)) => a.as_bytes() == b.as_slice(),
            (Value::Integer(a), Owned::Int(b)) => a == b,
            (Value::Boolean(a), Owned::Bool(b)) => a == b,
            _ => false,
        }
    }

    fn make(&mut self, o: &Owned) -> Value<'gc> {
        match o {
            Owned::Nil | Owned::Fresh | Owned::Opaque(_) => Value::Nil,
            Owned::Bool(b) => Value::Boolean(*b),
            Owned::Int(i) => Value::Integer(*i),
            Owned::Num(n) => Value::Number(*n),
            Owned::Str(s) => Value::String(self.ctx.intern(s)),
            Owned::Func(f) => self
                .resolve(f)
                .map_or(Value::Nil, |c| Value::Function(Function::Closure(c))),
            Owned::Table(id) => Value::Table(self.image(*id)),
        }
    }

    /// The table standing for old table `id`. A record the new run has no counterpart for is an
    /// empty table that `adopt` makes a proxy over the images of its values and contracts.
    fn image(&mut self, id: usize) -> Table<'gc> {
        if let Some(t) = self.images.get(&id) {
            return *t;
        }
        let t = Table::new(&self.ctx);
        self.images.insert(id, t);
        if let (Some(v), Some(c)) = (self.snap.tables[id].record, self.snap.tables[id].contracts) {
            self.adopt.push((t, v, c));
        }
        t
    }

    /// The table of values behind a record proxy.
    fn record_values(&self, t: Table<'gc>) -> Option<Table<'gc>> {
        match t.metatable()?.get(self.ctx, "__ppu_values") {
            Value::Table(v) => Some(v),
            _ => None,
        }
    }

    /// Make the new run's table `host` stand for old table `id`. False when one is a record and
    /// the other is not.
    fn bind(&mut self, id: usize, host: Table<'gc>) -> bool {
        if self.images.contains_key(&id) || self.claimed.contains(&host) {
            return true;
        }
        match (self.snap.tables[id].record, self.record_values(host)) {
            (Some(old), Some(new)) => {
                self.images.insert(id, host);
                // The old values go over the new record's, whose other fields stay.
                self.images.insert(old, new);
            }
            (None, None) => {
                self.images.insert(id, host);
                let all: Vec<_> = host.iter().collect();
                let fns = all
                    .iter()
                    .filter(|(_, v)| matches!(v, Value::Function(_)))
                    .cloned()
                    .collect();
                self.host_fns.insert(id, fns);
                self.host_all.insert(id, all);
            }
            _ => return false,
        }
        self.claimed.insert(host);
        true
    }

    /// The new run's value in old table `id`'s slot `k`: `None` when `id` has no counterpart,
    /// `Some(None)` when it has and the new code has no such slot.
    fn host_slot(&self, id: usize, k: &Owned) -> Option<Option<Value<'gc>>> {
        let all = self.host_all.get(&id)?;
        Some(
            all.iter()
                .find(|(hk, _)| self.same_key(hk, k))
                .map(|(_, v)| *v),
        )
    }

    fn note(&mut self, why: &str) {
        let line = format!("{}: {} carried, but {why}", self.at.0, self.at.1);
        if !self.lines.contains(&line) {
            self.lines.push(line);
        }
    }

    /// Put old table `id`'s contents, and every table it reaches, into their images. A hosted
    /// table is cleared first and gets its own function fields back after; the tables nested in
    /// it stand in the new run's tables at the same keys.
    fn fill(&mut self, id: usize) {
        let ctx = self.ctx;
        let snap = self.snap;
        let mut todo = vec![id];
        while let Some(id) = todo.pop() {
            if !self.filled.insert(id) {
                continue;
            }
            let ot = &snap.tables[id];
            if let Some(values) = ot.record {
                todo.push(values);
                todo.extend(ot.contracts);
                continue;
            }
            // A slot where a record and a plain table swapped places keeps the new run's, as a
            // variable does.
            let mut swapped = HashSet::new();
            for (i, (k, v)) in ot.pairs.iter().enumerate() {
                if let (Owned::Table(vid), Some(Some(Value::Table(h)))) = (v, self.host_slot(id, k))
                {
                    if !self.bind(*vid, h) {
                        self.note("a plain table and a record swapped places inside it, and that part kept its new value");
                        swapped.insert(i);
                    }
                }
            }
            let t = self.image(id);
            let own = self.host_fns.get(&id).cloned();
            if own.is_some() {
                let keys: Vec<_> = t.iter().map(|(k, _)| k).collect();
                for k in keys {
                    let _ = t.set_value(&ctx, k, Value::Nil);
                }
            }
            for (i, (k, v)) in ot.pairs.iter().enumerate() {
                let v = match v {
                    _ if swapped.contains(&i) => self.host_slot(id, k).flatten().unwrap_or(Value::Nil),
                    Owned::Fresh => match self.host_slot(id, k) {
                        Some(new) => new.unwrap_or(Value::Nil),
                        None => {
                            self.note("an engine value inside it starts new");
                            continue;
                        }
                    },
                    v => self.make(v),
                };
                let k = self.make(k);
                let _ = t.set_value(&ctx, k, v);
            }
            for (k, v) in own.into_iter().flatten() {
                let _ = t.set_value(&ctx, k, v);
            }
            let meta = ot.meta.map(|m| self.image(m));
            t.set_metatable(&ctx, meta);
            todo.extend(
                ot.pairs
                    .iter()
                    .enumerate()
                    .flat_map(|(i, (k, v))| [Some(k), (!swapped.contains(&i)).then_some(v)])
                    .flatten()
                    .filter_map(|o| match o {
                        Owned::Table(i) => Some(*i),
                        _ => None,
                    })
                    .chain(ot.meta),
            );
        }
    }
}

/// Carry the old run's file-level state into the new run. Returns one line per variable that kept
/// its new value though the old run had it, or carried all but a part.
pub(crate) fn carry(old: &Run, new: &Run) -> Vec<String> {
    apply(&capture(old), new)
}

/// A run's file-level state, as data that outlives the moment: [`apply`] puts it into a run, the
/// same one too (a paused frame drawn again starts from the state it first started from).
pub(crate) fn capture(old: &Run) -> Snap {
    old.lua.borrow_mut().enter(|ctx| {
        let found = discover(ctx, old);
        let mut s = Snapper {
            ctx,
            found: &found,
            mark: old.setup_mark.as_ref().map(|m| ctx.fetch(m)),
            ids: HashMap::new(),
            queue: vec![],
            snap: Snap::default(),
        };
        let mut keys: Vec<_> = found.cells.iter().collect();
        keys.sort_by(|a, b| a.0.cmp(b.0));
        for (key, (_, cell)) in keys {
            if let Some(UpValueState::Closed(v)) = cell.map(|c| c.get()) {
                let o = s.value(v);
                s.snap.values.push((key.clone(), o));
            }
        }
        s.drain();
        s.snap
    })
}

/// Put captured state into a run's file-level variables. Returns one line per variable that kept
/// its current value though the capture had it, or took all but a part.
pub(crate) fn apply(snap: &Snap, new: &Run) -> Vec<String> {
    let (lines, adopt) = new.lua.borrow_mut().enter(|ctx| {
        let found = discover(ctx, new);
        let mut a = Apply {
            ctx,
            found: &found,
            snap,
            images: HashMap::new(),
            claimed: HashSet::new(),
            host_fns: HashMap::new(),
            host_all: HashMap::new(),
            filled: HashSet::new(),
            adopt: vec![],
            at: Key::default(),
            lines: vec![],
        };
        let mut lines = vec![];
        // The variables whose kinds agree: (key, cell, old value).
        let mut agreed = vec![];
        // The variables that point to a function: the new run's closure for it.
        let mut funcs = vec![];
        for (key, old_v) in &snap.values {
            let Some((_, Some(cell))) = found.cells.get(key) else {
                continue;
            };
            let UpValueState::Closed(new_v) = cell.get() else {
                continue;
            };
            match (old_v, new_v) {
                // Nil is a legal value of any variable, a function's too: a one-shot callback the run
                // consumed stays consumed. An initializer edited from nil applies on Run, like any other.
                (Owned::Nil, _) => agreed.push((key, *cell, old_v)),
                (Owned::Fresh, _) => {} // engine setup: the new run's stands
                (Owned::Opaque(what), _) => lines.push(kept(
                    key,
                    &format!("it holds {what}, which cannot be carried"),
                )),
                // A variable that points to a function keeps which function, as the new code.
                (Owned::Func(f), Value::Function(_)) => match a.resolve(f) {
                    Some(c) => funcs.push((*cell, c)),
                    None => lines.push(kept(
                        key,
                        "it points to a function the new code no longer has",
                    )),
                },
                (_, Value::Nil) => agreed.push((key, *cell, old_v)),
                (o, v) if kind(o) == kind_of(v) => agreed.push((key, *cell, o)),
                (o, v) => lines.push(kept(
                    key,
                    &format!("it changed from {} to {}", kind(o), kind_of(v)),
                )),
            }
        }
        // Each table that is a variable's new value stands for the old table in its place.
        let mut ok = vec![];
        for (key, cell, old_v) in agreed {
            if let (Owned::Table(id), UpValueState::Closed(Value::Table(host))) =
                (old_v, cell.get())
            {
                if !a.bind(*id, host) {
                    lines.push(kept(key, "it changed between a record and a plain table"));
                    continue;
                }
            }
            ok.push((key, cell, old_v));
        }
        let mut carried = vec![];
        for (key, cell, old_v) in ok {
            match a.blocker(old_v) {
                Some(why) => lines.push(kept(key, &why)),
                None => carried.push((key, cell, old_v)),
            }
        }
        for (key, cell, old_v) in carried {
            a.at = key.clone();
            let v = match old_v {
                Owned::Table(id) => {
                    a.fill(*id);
                    Value::Table(a.image(*id))
                }
                o => a.make(o),
            };
            cell.set(&ctx, UpValueState::Closed(v));
        }
        for (cell, c) in funcs {
            cell.set(
                &ctx,
                UpValueState::Closed(Value::Function(Function::Closure(c))),
            );
        }
        lines.extend(a.lines);
        let adopt: Vec<_> = a
            .adopt
            .iter()
            .map(|(t, v, c)| {
                (
                    ctx.stash(*t),
                    ctx.stash(a.images[v]),
                    ctx.stash(a.images[c]),
                )
            })
            .collect();
        (lines, adopt)
    });
    // Each record with no counterpart becomes a proxy again (bounds and store checks kept).
    let mut l = new.lua.borrow_mut();
    let mut lines = lines;
    for (proxy, values, contracts) in adopt {
        let ex = l.enter(|ctx| match ctx.get_global("__ppu_record_proxy") {
            Value::Function(f) => Some(ctx.stash(Executor::start(
                ctx,
                f,
                (ctx.fetch(&values), ctx.fetch(&contracts)),
            ))),
            _ => None,
        });
        let Some(ex) = ex else {
            lines.push("a record could not be rebuilt: the program replaced __ppu_record_proxy, which the engine owns".into());
            break;
        };
        l.finish(&ex);
        let _ = l.try_enter(|ctx| {
            if let Value::Table(p) = ctx.fetch(&ex).take_result::<Value>(ctx)?? {
                ctx.fetch(&proxy).set_metatable(&ctx, p.metatable());
            }
            Ok(())
        });
    }
    lines
}
