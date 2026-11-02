//! Luau syntax first, with the existing Lua parser as the compatibility fallback.
//! Types are erased before the existing VM/IL; this does not embed a Luau VM.
use full_moon::{
    ast::{self, luau, CompoundAssignment},
    node::Node,
    visitors::Visitor,
};
use std::{borrow::Cow, ops::Range};

#[derive(Debug)]
pub struct Error {
    pub line: u32,
    pub message: String,
}

#[derive(Default)]
struct Lower {
    erase: Vec<Range<usize>>,
    edits: Vec<(usize, Range<usize>, String)>,
    errors: Vec<(usize, Error)>,
}

// From the first token's start to the last token's end: `Node::range` can stop before a closing
// bracket (`a[1]`), which put inserted parentheses inside it.
fn range(node: &impl Node) -> Range<usize> {
    let mut tokens = node.tokens().map(|t| t.token().start_position().bytes()..t.token().end_position().bytes());
    let first = tokens.next().expect("parsed syntax has tokens");
    tokens.fold(first, |a, b| a.start.min(b.start)..a.end.max(b.end))
}

impl Lower {
    fn erase(&mut self, node: &impl Node) {
        // Token trivia is deliberately untouched: contracts and comments survive.
        self.erase.extend(
            node.tokens()
                .map(|t| t.token().start_position().bytes()..t.token().end_position().bytes()),
        );
    }
    fn reject(&mut self, node: &impl Node, feature: &str) {
        self.errors.push((
            range(node).start,
            Error {
                line: node.start_position().unwrap().line() as u32,
                message: format!(
                    "Luau {feature} is not supported by the current execution subset."
                ),
            },
        ));
    }
}

macro_rules! erase {
    ($($method:ident : $node:ident),* $(,)?) => {$ (
        fn $method(&mut self, node: &luau::$node) { self.erase(node); }
    )*};
}
macro_rules! reject {
    ($($method:ident : $node:ident => $feature:literal),* $(,)?) => {$ (
        fn $method(&mut self, node: &luau::$node) { self.reject(node, $feature); }
    )*};
}

impl Lower {
    // `saveData({hiscore = makeBounded(0, 0, 999999), seen = false})` gets its layout as a second argument
    // (crate::save): field order and offsets are decided here, once, for the preview and the cartridge.
    fn save_data(&mut self, node: &ast::FunctionCall) {
        const HELP: &str = "saveData takes one table written inline whose fields are makeBounded(default, min, max) with whole-number literal bounds, or true/false: `saveData({hiscore = makeBounded(0, 0, 999999), seen_intro = false})`";
        fn fail(this: &mut Lower, node: &impl Node, message: String) {
            this.errors.push((
                range(node).start,
                Error { line: node.start_position().unwrap().line() as u32, message },
            ));
        }
        let [ast::Suffix::Call(ast::Call::AnonymousCall(args))] = node.suffixes().collect::<Vec<_>>().as_slice() else {
            return fail(self, node, HELP.into());
        };
        let ast::FunctionArgs::Parentheses { parentheses, arguments } = args.as_ref() else {
            return fail(self, node, HELP.into());
        };
        let values: Vec<_> = arguments.iter().collect();
        // Already prepared (a pass that prepares the prepared text again): the layout is the second argument.
        // shortcut: a layout written by hand is trusted as this one; compare it to the record's if that bites.
        if let [_, ast::Expression::TableConstructor(_)] = values.as_slice() {
            return;
        }
        let [ast::Expression::TableConstructor(table)] = values.as_slice() else {
            return fail(self, node, HELP.into());
        };
        let mut fields = vec![];
        for field in table.fields() {
            let ast::Field::NameKey { key, value, .. } = field else {
                return fail(self, field, HELP.into());
            };
            let name = key.token().to_string();
            let kind = match &**value {
                ast::Expression::Symbol(t) if matches!(t.token().to_string().as_str(), "true" | "false") => {
                    crate::save::Kind::Boolean
                }
                ast::Expression::FunctionCall(call)
                    if matches!(call.prefix(), ast::Prefix::Name(n) if n.token().to_string() == "makeBounded") =>
                {
                    let bounds = match call.suffixes().collect::<Vec<_>>().as_slice() {
                        [ast::Suffix::Call(ast::Call::AnonymousCall(a))] => match a.as_ref() {
                            ast::FunctionArgs::Parentheses { arguments, .. } => {
                                let a: Vec<_> = arguments.iter().collect();
                                match a.as_slice() {
                                    [_, lo, hi] => integer(lo).zip(integer(hi)),
                                    _ => None,
                                }
                            }
                            _ => None,
                        },
                        _ => None,
                    };
                    let Some((lo, hi)) = bounds else {
                        return fail(self, &**value, format!("saveData field {name}: {HELP}"));
                    };
                    crate::save::Kind::Bounded { lo, hi }
                }
                _ => return fail(self, &**value, format!("saveData field {name}: {HELP}")),
            };
            fields.push((name, kind));
        }
        match crate::save::layout(&fields) {
            Ok(layout) => {
                let close = range(parentheses.tokens().1).start;
                self.edits.push((close, close..close, format!(", {}", layout.lua())));
            }
            Err(message) => fail(self, node, message),
        }
    }
}

/// A whole-number literal, negative or not.
fn integer(e: &ast::Expression) -> Option<i64> {
    match e {
        ast::Expression::Number(n) => n.token().to_string().parse().ok(),
        ast::Expression::UnaryOperator { unop: ast::UnOp::Minus(_), expression } => integer(expression).map(|n| -n),
        ast::Expression::Parentheses { expression, .. } => integer(expression),
        _ => None,
    }
}

impl Visitor for Lower {
    erase! {
        visit_type_specifier: TypeSpecifier,
        visit_type_assertion: TypeAssertion,
        visit_type_declaration: TypeDeclaration,
        visit_exported_type_declaration: ExportedTypeDeclaration,
        visit_generic_declaration: GenericDeclaration,
        visit_type_instantiation: TypeInstantiation,
        visit_type_function: TypeFunction,
        visit_exported_type_function: ExportedTypeFunction,
    }
    reject! {
        visit_interpolated_string: InterpolatedString => "interpolated strings",
        visit_if_condition_binding: IfConditionBinding => "condition bindings",
        visit_luau_attribute: LuauAttribute => "attributes",
        visit_const_assignment: ConstAssignment => "const declarations",
        visit_const_function: ConstFunction => "const functions",
    }
    // `if c then a elseif d then b else e` -> `((c) and (a) or ((d) and (b) or (e)))`.
    // Only keyword tokens are replaced, so nested expressions and line breaks stay in place.
    fn visit_if_expression(&mut self, node: &luau::IfExpression) {
        let branches = std::iter::once((node.binding(), node.if_expression())).chain(
            node.else_if_expressions()
                .into_iter()
                .flatten()
                .map(|e| (e.binding(), e.expression())),
        );
        let conditions: Vec<&ast::Expression> = std::iter::once(node.condition())
            .chain(node.else_if_expressions().into_iter().flatten().map(|e| e.condition()))
            .collect();
        let mut paired = false;
        for (binding, value) in branches {
            if binding.is_some() {
                return self.reject(node, "condition bindings");
            }
            paired |= may_be_false(value);
        }
        // `c and a or b` picks `b` when `a` is false or nil. A then-value that may be false takes
        // `c and a or not c and b`, which tests each condition twice: only conditions without calls (and on one line,
        // so the copy keeps the line numbers). A then-value that is nil comes out false.
        if paired {
            if let Some(c) = conditions.iter().find(|c| has_call(c) || c.to_string().contains('\n')) {
                return self.reject(*c, "if expressions whose then-value may be false or nil and whose condition calls a function; use an if statement");
            }
        }
        let text = |c: &ast::Expression| c.to_string().trim().to_string();
        let mut inserted = String::new();
        let mut swap = |token: &full_moon::tokenizer::TokenReference, text: String| {
            let span = range(token);
            inserted.push_str(&text);
            self.edits.push((span.start, span, text));
        };
        swap(node.if_token(), "((".into());
        swap(node.then_token(), ") and (".into());
        for (k, e) in node.else_if_expressions().into_iter().flatten().enumerate() {
            let otherwise = if paired { format!(") or (not ({})) and ((", text(conditions[k])) } else { ") or ((".into() };
            swap(e.else_if_token(), otherwise);
            swap(e.then_token(), ") and (".into());
        }
        let last = if paired { format!(") or (not ({})) and (", text(conditions[conditions.len() - 1])) } else { ") or (".into() };
        swap(node.else_token(), last);
        let closers = inserted.matches('(').count() - inserted.matches(')').count();
        let span = range(node);
        self.edits.push((span.start, span.end..span.end, ")".repeat(closers)));
    }
    // `{x = makeFixed(0, -8, 8)}` -> `__ppu_record({x = __ppu_initial(0, -8, 8)}, {x = {true, -8, 8, 1024}})`:
    // a record whose field keeps its contract for every later store (see luau.lua).
    fn visit_table_constructor(&mut self, node: &ast::TableConstructor) {
        let mut contracts = vec![];
        for field in node.fields() {
            let ast::Field::NameKey { key, value, .. } = field else { continue };
            let ast::Expression::FunctionCall(call) = value.as_ref() else { continue };
            let ast::Prefix::Name(callee) = call.prefix() else { continue };
            let fixed = match callee.token().to_string().as_str() {
                "makeFixed" => true,
                "makeBounded" => false,
                _ => continue,
            };
            let [ast::Suffix::Call(ast::Call::AnonymousCall(args))] =
                call.suffixes().collect::<Vec<_>>().as_slice()
            else {
                continue;
            };
            let ast::FunctionArgs::Parentheses { arguments, .. } = args.as_ref() else { continue };
            let text = |e: &ast::Expression| e.tokens().map(|t| t.token().to_string()).collect::<String>();
            let args: Vec<String> = arguments.iter().map(text).collect();
            if args.len() != 3 && !(fixed && args.len() == 4) {
                continue; // the runtime constructor reports the arity error
            }
            let denominator = args.get(3).map_or("1024", String::as_str);
            contracts.push(format!(
                "{} = {{{fixed}, {}, {}, {denominator}}}",
                key.token(),
                args[1],
                args[2]
            ));
            let span = range(callee);
            self.edits.push((span.start, span, "__ppu_initial".into()));
        }
        if contracts.is_empty() {
            return;
        }
        let span = range(node);
        self.edits.push((span.start, span.start..span.start, "__ppu_record(".into()));
        self.edits.push((span.start, span.end..span.end, format!(", {{{}}})", contracts.join(", "))));
    }
    // Luau iterates a table directly; `pairs` visits the array part in order, then the rest.
    fn visit_generic_for(&mut self, node: &ast::GenericFor) {
        let mut values = node.expressions().iter();
        if let (Some(value), None) = (values.next(), values.next()) {
            if !matches!(value, ast::Expression::FunctionCall(_)) {
                let span = range(value);
                self.edits.push((span.start, span.start..span.start, "pairs(".into()));
                self.edits.push((span.start, span.end..span.end, ")".into()));
            }
        }
    }
    // lookup/bakeHdma grids are keyed by parameter name, which Lua cannot read at run time.
    fn visit_function_call(&mut self, node: &ast::FunctionCall) {
        let ast::Prefix::Name(name) = node.prefix() else { return };
        if name.token().to_string() == "saveData" {
            return self.save_data(node);
        }
        if !matches!(name.token().to_string().as_str(), "lookup" | "bakeHdma") {
            return;
        }
        let [ast::Suffix::Call(ast::Call::AnonymousCall(args))] =
            node.suffixes().collect::<Vec<_>>().as_slice()
        else {
            return;
        };
        let ast::FunctionArgs::Parentheses { parentheses, arguments } = args.as_ref() else {
            return;
        };
        let Some(ast::Expression::Function(f)) = arguments.iter().last() else { return };
        let names: Vec<String> = f
            .body()
            .parameters()
            .iter()
            .filter_map(|p| match p {
                ast::Parameter::Name(n) => Some(format!("{:?}", n.token().to_string())),
                _ => None,
            })
            .collect();
        let close = range(parentheses.tokens().1).start;
        self.edits.push((close, close..close, format!(", {{{}}}", names.join(", "))));
    }
    fn visit_last_stmt(&mut self, node: &ast::LastStmt) {
        if matches!(node, ast::LastStmt::Continue(_)) {
            self.reject(node, "continue");
        }
    }
    fn visit_compound_assignment(&mut self, node: &CompoundAssignment) {
        // The target is read and written, so it is written out twice: every index must be call-free.
        let simple = match node.lhs() {
            ast::Var::Name(_) => true,
            ast::Var::Expression(v) => {
                matches!(v.prefix(), ast::Prefix::Name(_))
                    && v.suffixes().all(|s| match s {
                        ast::Suffix::Index(ast::Index::Dot { .. }) => true,
                        ast::Suffix::Index(ast::Index::Brackets { expression, .. }) => !has_call(expression),
                        _ => false,
                    })
            }
            _ => false,
        };
        if !simple {
            self.reject(node, "compound assignment to a target that calls a function; bind the called part to a local first");
            return;
        }
        // Node::tokens visits a bracketed span's delimiters before its contents: order by position.
        let mut tokens: Vec<_> = node.lhs().tokens().map(|t| (t.token().start_position().bytes(), t.token().to_string())).collect();
        tokens.sort_by_key(|(at, _)| *at);
        let lhs = tokens.into_iter().map(|(_, t)| t).collect::<Vec<_>>().join(" ");
        let operator = node.compound_operator().token().token().to_string();
        let operator = operator.strip_suffix('=').unwrap();
        let operator_span = range(node.compound_operator());
        self.edits.push((
            operator_span.start,
            operator_span.clone(),
            format!("= {lhs} {operator} ("),
        ));
        let end = range(node.rhs()).end;
        self.edits.push((operator_span.start, end..end, ")".into()));
    }
}

/// The `require("./name")` paths in `source`, in order of appearance.
pub fn requires(source: &str) -> Vec<&str> {
    let mut paths = vec![];
    for (at, _) in source.match_indices("require(") {
        // shortcut: only `--` line comments are skipped, not --[[ ]] blocks; parse if one bites.
        let line = &source[source[..at].rfind('\n').map_or(0, |n| n + 1)..at];
        if line.contains("--") {
            continue;
        }
        let rest = &source[at + "require(".len()..];
        let Some(quote) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') else { continue };
        let Some(end) = rest[1..].find(quote) else { continue };
        if let Some(path) = rest[1..1 + end].strip_prefix("./") {
            paths.push(path);
        }
    }
    paths
}

/// The listed files some file requires: `./state` names `state.luau`, else `state.lua`.
pub fn required_files(files: &[(&str, &str)]) -> std::collections::HashSet<String> {
    let names: std::collections::HashSet<&str> = files.iter().map(|(n, _)| *n).collect();
    files
        .iter()
        .flat_map(|(_, s)| requires(s))
        .filter_map(|p| {
            [format!("{p}.luau"), format!("{p}.lua")]
                .into_iter()
                .find(|f| names.contains(f.as_str()))
        })
        .collect()
}

// shortcut: names, fields, indexes and calls are assumed to hold non-false values; type-directed check if that bites.
/// Whether evaluating `e` may call a function (so it must run once).
fn has_call(e: &ast::Expression) -> bool {
    match e {
        ast::Expression::FunctionCall(_) | ast::Expression::IfExpression(_) | ast::Expression::Function(_) => true,
        ast::Expression::Parentheses { expression, .. } => has_call(expression),
        ast::Expression::UnaryOperator { expression, .. } => has_call(expression),
        ast::Expression::BinaryOperator { lhs, rhs, .. } => has_call(lhs) || has_call(rhs),
        ast::Expression::Var(ast::Var::Name(_)) => false,
        ast::Expression::Var(ast::Var::Expression(v)) => v.suffixes().any(|s| match s {
            ast::Suffix::Call(_) => true,
            ast::Suffix::Index(ast::Index::Brackets { expression, .. }) => has_call(expression),
            _ => false,
        }) || matches!(v.prefix(), ast::Prefix::Expression(inner) if has_call(inner)),
        ast::Expression::Symbol(_) | ast::Expression::Number(_) | ast::Expression::String(_) => false,
        _ => true,
    }
}

fn may_be_false(value: &ast::Expression) -> bool {
    match value {
        ast::Expression::Symbol(t) => matches!(t.token().to_string().as_str(), "false" | "nil"),
        ast::Expression::Parentheses { expression, .. } => may_be_false(expression),
        ast::Expression::UnaryOperator { unop, .. } => matches!(unop, ast::UnOp::Not(_)),
        ast::Expression::BinaryOperator { binop, .. } => matches!(
            binop.token().token().to_string().as_str(),
            "==" | "~=" | "<" | "<=" | ">" | ">=" | "and" | "or"
        ),
        _ => false,
    }
}

/// Normalize supported Luau syntax without moving comments or newlines.
/// A Luau syntax rejection returns the original source for the existing parser.
/// An accepted but unsupported Luau construct is an error, never a fallback.
pub fn prepare(source: &str) -> Result<Cow<'_, str>, Error> {
    let Ok(ast) = full_moon::parse_fallible(source, full_moon::LuaVersion::luau()).into_result()
    else {
        // The existing parser validates the original bytes and retains its diagnostics.
        return Ok(Cow::Borrowed(source));
    };
    let mut lower = Lower::default();
    lower.visit_ast(&ast);
    let mut masked = source.as_bytes().to_vec();
    let mut erased = vec![false; source.len()];
    for span in &lower.erase {
        for i in span.clone() {
            erased[i] = true;
            if !matches!(masked[i], b'\n' | b'\r') {
                masked[i] = b' ';
            }
        }
    }
    if let Some((_, error)) = lower.errors.into_iter().find(|(at, _)| !erased[*at]) {
        return Err(error);
    }
    lower.edits.retain(|(at, _, _)| !erased[*at]);
    if lower.erase.is_empty() && lower.edits.is_empty() {
        return Ok(Cow::Borrowed(source));
    }
    let mut result = String::from_utf8(masked).expect("only whole syntax tokens are erased");
    // At one position, replace a token before inserting in front of it.
    lower.edits.sort_by(|a, b| (b.1.start, b.1.end).cmp(&(a.1.start, a.1.end)));
    for (_, span, replacement) in lower.edits {
        result.replace_range(span, &replacement);
    }
    Ok(Cow::Owned(result))
}

/// True when a table constructor declares a field with makeFixed: a record contract, which
/// opts the program into makeFixed's arithmetic (`fixed_point`).
pub fn fixed_contracts(source: &str) -> bool {
    makes_fixed(source, false)
}

/// True when any call to makeFixed is written: a record field or a binding (the Luau surface).
pub fn fixed_calls(source: &str) -> bool {
    makes_fixed(source, true)
}

fn makes_fixed(source: &str, anywhere: bool) -> bool {
    if !source.contains("makeFixed") {
        return false;
    }
    let Ok(ast) = full_moon::parse_fallible(source, full_moon::LuaVersion::luau()).into_result()
    else {
        return false;
    };
    #[derive(Default)]
    struct Found(bool, bool);
    impl Visitor for Found {
        fn visit_function_call(&mut self, node: &ast::FunctionCall) {
            self.0 |= self.1
                && matches!(node.prefix(), ast::Prefix::Name(n) if n.token().to_string() == "makeFixed");
        }
        fn visit_table_constructor(&mut self, node: &ast::TableConstructor) {
            self.0 |= node.fields().iter().any(|field| {
                let ast::Field::NameKey { value, .. } = field else { return false };
                let ast::Expression::FunctionCall(call) = value.as_ref() else { return false };
                matches!(call.prefix(), ast::Prefix::Name(n) if n.token().to_string() == "makeFixed")
            });
        }
    }
    let mut found = Found(false, anywhere);
    found.visit_ast(&ast);
    found.0
}

/// Fixed-point arithmetic for a program with makeFixed contracts: the same law canonical
/// precision uses. Fractional literals round to Q10 (nearest, ties away from zero), and `*`
/// and `/` run on Q10 operands with the result truncated toward zero (`__ppu_mul`/`__ppu_div`
/// in luau.lua). Sums, differences, comparisons, floors and `%` of Q10 values are already
/// exact. Functions passed to `lookup` keep f64: they are sampled tables, not runtime code;
/// helpers they call follow `__ppu_sampling` at run time.
/// Runs on `prepare`d text, so compound assignments are already plain operators.
pub fn fixed_point(source: &str) -> Cow<'_, str> {
    let Ok(ast) = full_moon::parse_fallible(source, full_moon::LuaVersion::luau()).into_result()
    else {
        return Cow::Borrowed(source);
    };
    #[derive(Default)]
    struct Sampled(Vec<Range<usize>>);
    impl Visitor for Sampled {
        fn visit_function_call(&mut self, node: &ast::FunctionCall) {
            let ast::Prefix::Name(name) = node.prefix() else { return };
            if name.token().to_string() != "lookup" {
                return;
            }
            for suffix in node.suffixes() {
                let ast::Suffix::Call(ast::Call::AnonymousCall(args)) = suffix else { continue };
                if let ast::FunctionArgs::Parentheses { arguments, .. } = args.as_ref() {
                    for a in arguments.iter().filter(|a| matches!(a, ast::Expression::Function(_))) {
                        self.0.push(range(a));
                    }
                }
            }
        }
    }
    struct Fixed {
        sampled: Vec<Range<usize>>,
        // (span, visit order, text); one position's insertions keep the outer call first.
        edits: Vec<(Range<usize>, usize, String)>,
    }
    impl Fixed {
        fn runtime(&self, at: usize) -> bool {
            !self.sampled.iter().any(|r| r.contains(&at))
        }
    }
    impl Visitor for Fixed {
        fn visit_expression(&mut self, node: &ast::Expression) {
            let order = self.edits.len();
            match node {
                ast::Expression::BinaryOperator { lhs, binop, rhs } if self.runtime(range(node).start) => {
                    let helper = match binop {
                        ast::BinOp::Star(_) => "__ppu_mul",
                        ast::BinOp::Slash(_) => "__ppu_div",
                        _ => return,
                    };
                    let start = range(lhs.as_ref()).start;
                    self.edits.push((start..start, order, format!("{helper}(")));
                    self.edits.push((range(binop), order, ",".into()));
                    let end = range(rhs.as_ref()).end;
                    self.edits.push((end..end, order, ")".into()));
                }
                ast::Expression::Number(n) if self.runtime(range(node).start) => {
                    let text = n.token().to_string();
                    if text.starts_with("0x") || text.starts_with("0X") || !text.contains(['.', 'e', 'E']) {
                        return;
                    }
                    let Ok(value) = text.parse::<f64>() else { return };
                    let raw = (value * 1024.).round();
                    if raw / 1024. != value {
                        let q = raw / 1024.;
                        let q = if q.fract() == 0. { format!("{q:.1}") } else { format!("{q:?}") };
                        // A helper a lookup is sampling sees the authored value.
                        self.edits.push((range(n), order, format!("(__ppu_sampling and {text} or {q})")));
                    }
                }
                _ => {}
            }
        }
    }
    let mut sampled = Sampled::default();
    sampled.visit_ast(&ast);
    let mut fixed = Fixed { sampled: sampled.0, edits: vec![] };
    fixed.visit_ast(&ast);
    if fixed.edits.is_empty() {
        return Cow::Borrowed(source);
    }
    let mut result = source.to_string();
    // Back to front; at one insertion point the later-visited (inner) text goes in first.
    fixed
        .edits
        .sort_by(|a, b| (b.0.start, b.0.end, b.1).cmp(&(a.0.start, a.0.end, a.1)));
    for (span, _, text) in fixed.edits {
        result.replace_range(span, &text);
    }
    Cow::Owned(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use piccolo::compiler::{interning::BasicInterner, parse_chunk};

    #[test]
    fn fixed_point_rewrites_products_quotients_and_literals_outside_lookups() {
        let source = "local a = x * 0.164 / 60\nlocal b = 1.5 + 2 * -y\nlocal t = lookup(g, function(s) return s * 0.3 end, {\"s\"})\n";
        let out = fixed_point(source);
        assert_eq!(
            out,
            "local a = __ppu_div(__ppu_mul(x , (__ppu_sampling and 0.164 or 0.1640625)) , 60)\nlocal b = 1.5 + __ppu_mul(2 , -y)\nlocal t = lookup(g, function(s) return s * 0.3 end, {\"s\"})\n"
        );
        assert!(parse_chunk(out.as_bytes(), BasicInterner::default()).is_ok());
    }

    #[test]
    fn ordinary_lua_and_legacy_bitwise_code_stay_byte_identical() {
        for source in [
            "-- comment\nlocal n=1\n",
            "local n=(7 & 3) << 1\n",
            "local n=1 | 2\n",
        ] {
            assert!(matches!(prepare(source).unwrap(), Cow::Borrowed(_)));
            assert!(parse_chunk(
                prepare(source).unwrap().as_bytes(),
                BasicInterner::default()
            )
            .is_ok());
        }
    }
    #[test]
    fn erases_types_preserving_contracts_comments_and_lines() {
        let source="--!strict\r\nexport type Enemy = {\r\n x: number, -- keep 🌠\r\n}\r\n---@snes range hp 0 10\r\nlocal hp: number=1\r\nlocal function id<T>(n: T): T return n :: T end\r\nfunction frame() hp += (\r\n 1 -- retained\r\n)\r\nend\r\n";
        let normalized = prepare(source).unwrap();
        assert_eq!(
            source.matches('\n').count(),
            normalized.matches('\n').count()
        );
        for text in ["-- keep 🌠", "---@snes range hp 0 10", "-- retained"] {
            assert!(normalized.contains(text));
        }
        assert!(
            parse_chunk(normalized.as_bytes(), BasicInterner::default()).is_ok(),
            "{normalized}"
        );
    }
    #[test]
    fn declarations_inside_erased_types_do_not_become_runtime_errors() {
        let source="type F = typeof(function() local x=1 x += 1 return if x>0 then x else 0 end)\nlocal n: number=1\n";
        let normalized = prepare(source).unwrap();
        assert!(
            parse_chunk(normalized.as_bytes(), BasicInterner::default()).is_ok(),
            "{normalized}"
        );
    }
    #[test]
    fn requires_in_comments_are_not_requires() {
        let source = "-- main.luau loads it with require(\"./self\")\nlocal A = require(\"./a\") -- not require(\"./b\")\n";
        assert_eq!(requires(source), ["a"]);
    }

    #[test]
    fn unsupported_luau_does_not_fall_back_or_drop_code() {
        for source in [
            "local n=if f() then false else 2",
            "local s=`hello`",
            "while true do continue end",
            "local a={{x=1}} a[f()].x += 1",
            "local a={{x=1}} a[1].x += 1 g().x += 1",
        ] {
            assert!(prepare(source).is_err(), "{source}");
        }
        let mut engine = crate::LuaEngine::new();
        assert_eq!(engine.set_source("-- ok\n\0").unwrap_err().line, Some(2));
    }
    #[test]
    fn index_and_call_values_keep_their_closing_brackets() {
        // The then-value may be an index or a call (taken as never false, like a name or field).
        let source = "local a = {7, 9}\nlocal function f(n) return a[n] end\nlocal x = 2\nlocal n = 0\nfunction frame()\n  local t = if x == 1 then a[1] else a[2]\n  local u = if x == 2 then f(1) else f(2)\n  n += a[2]\n  obj[0].x = t\n  obj[1].x = u\n  obj[2].x = n\nend\n";
        let mut engine = crate::LuaEngine::new();
        engine.set_sources(&[("main.luau", source)]).unwrap();
        engine.frame(0., 0).unwrap();
        let oam = &engine.memory().oam;
        assert_eq!((oam[0].x, oam[1].x, oam[2].x), (9, 7, 9));
        // A then-value that may be false needs a condition without calls (it is tested twice).
        let error = prepare("local t = if f(1) == 1 then false else a[2]").unwrap_err();
        assert!(error.message.contains("then-value may be false or nil"), "{}", error.message);
    }
    #[test]
    fn if_expressions_whose_then_value_may_be_false_pick_it() {
        // `c and a or b` would pick b when a is false; the paired form tests the condition again.
        let source = "local n = 0\nlocal hits = 0\nlocal function near(d) return d < 3 end\nfunction frame(t, f)\n  local owner = f % 3\n  local hit = if owner == 1 then false elseif owner == 2 then near(f % 5) else not near(f % 5)\n  if hit then hits += 1 end\n  local tally = if f % 2 == 0 then hits > 2 else n == 0\n  obj[0].x = hits\n  obj[1].x = if tally then 1 else 0\nend\n";
        let mut engine = crate::LuaEngine::new();
        engine.set_sources(&[("main.luau", source)]).unwrap();
        let mut hits = 0;
        for f in 0..12 {
            engine.frame(f as f64 / 60., f).unwrap();
            let owner = f % 3;
            let near = (f % 5) < 3;
            let hit = match owner { 1 => false, 2 => near, _ => !near };
            if hit { hits += 1 }
            let tally = if f % 2 == 0 { hits > 2 } else { true };
            let oam = &engine.memory().oam;
            assert_eq!((oam[0].x as i32, oam[1].x), (hits, tally as i16), "frame {f}");
        }
    }
    #[test]
    fn nested_compounds_preserve_execution_and_rhs_precedence() {
        let source="local n: number=1\nlocal other=0\nfunction frame() n += (function() other += 2 return other end)() + 1\nobj[0].x=n\nend\n";
        let mut engine = crate::LuaEngine::new();
        engine.set_source(source).unwrap();
        engine.frame(0., 0).unwrap();
        assert_eq!(engine.memory().oam[0].x, 4);
    }
    #[test]
    fn indexed_compound_targets_read_and_write_one_element() {
        let source = "local t = {0, 0, 0}\nlocal r = {a = {1, 2}}\nlocal i = 1\nfunction frame(_, f)\n  i = f % 3 + 1\n  t[i] += 2\n  t[i] *= 3\n  r.a[i % 2 + 1] -= 1\n  t[i == 2 and 3 or 1] += 0\n  obj[0].x = t[1] + t[2] * 4 + t[3] * 16\n  obj[1].x = r.a[1] + 100\n  obj[2].x = r.a[2] + 100\nend\n";
        let mut engine = crate::LuaEngine::new();
        engine.set_sources(&[("main.luau", source)]).unwrap();
        let (mut t, mut a) = ([0i32; 3], [1i32, 2]);
        for f in 0..6u32 {
            engine.frame(f as f64 / 60., f).unwrap();
            let i = (f % 3) as usize;
            t[i] = (t[i] + 2) * 3;
            a[(i + 1) % 2] -= 1;
            let oam = &engine.memory().oam;
            assert_eq!((oam[0].x as i32, oam[1].x as i32, oam[2].x as i32), (t[0] + t[1] * 4 + t[2] * 16, a[0] + 100, a[1] + 100), "frame {f}");
        }
        let error = prepare("local a={0} local function f() return 1 end a[f()] += 1").unwrap_err();
        assert!(error.message.contains("target that calls a function"), "{}", error.message);
    }
    #[test]
    fn preview_uses_shared_frontend_and_keeps_runtime_locations() {
        let source="--!strict\nlocal hp: number=makeBounded(1,0,2)\nfunction frame()\n hp += (\n 1\n )\n hp=3\nend\n";
        let mut engine = crate::LuaEngine::new();
        engine.set_sources(&[("game.luau", source)]).unwrap();
        let error = engine.frame(0., 0).unwrap_err();
        assert_eq!(error.file.as_deref(), Some("game.luau"));
        assert_eq!(error.line, Some(7));
        assert!(error.message.contains("makeBounded"));
    }
    #[test]
    fn binary_string_content_and_contextual_identifiers_remain_lua() {
        for source in [
            "local value='a\0b'\n",
            "local continue=1\nlocal type=continue\n",
        ] {
            let mut engine = crate::LuaEngine::new();
            engine.set_source(source).unwrap();
        }
    }
}
