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

fn range(node: &impl Node) -> Range<usize> {
    let (first, last) = node.range().expect("parsed syntax has source positions");
    first.bytes()..last.bytes()
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
        visit_if_expression: IfExpression => "if expressions",
        visit_interpolated_string: InterpolatedString => "interpolated strings",
        visit_if_condition_binding: IfConditionBinding => "condition bindings",
        visit_luau_attribute: LuauAttribute => "attributes",
        visit_const_assignment: ConstAssignment => "const declarations",
        visit_const_function: ConstFunction => "const functions",
    }
    fn visit_last_stmt(&mut self, node: &ast::LastStmt) {
        if matches!(node, ast::LastStmt::Continue(_)) {
            self.reject(node, "continue");
        }
    }
    fn visit_compound_assignment(&mut self, node: &CompoundAssignment) {
        let simple = match node.lhs() {
            ast::Var::Name(_) => true,
            ast::Var::Expression(v) => {
                matches!(v.prefix(), ast::Prefix::Name(_))
                    && matches!(
                        v.suffixes().collect::<Vec<_>>().as_slice(),
                        [ast::Suffix::Index(ast::Index::Dot { .. })]
                    )
            }
            _ => false,
        };
        if !simple {
            self.reject(node, "compound assignment to indexed or computed targets; bind the record to a local first");
            return;
        }
        let lhs: String = node.lhs().tokens().map(|t| t.token().to_string()).collect();
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
    lower.edits.sort_by(|a, b| b.1.start.cmp(&a.1.start));
    for (_, span, replacement) in lower.edits {
        result.replace_range(span, &replacement);
    }
    Ok(Cow::Owned(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use piccolo::compiler::{interning::BasicInterner, parse_chunk};

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
    fn unsupported_luau_does_not_fall_back_or_drop_code() {
        for source in [
            "local n=if true then 1 else 2",
            "local s=`hello`",
            "while true do continue end",
            "local a={{x=1}} a[1].x += 1",
        ] {
            assert!(prepare(source).is_err(), "{source}");
        }
        let mut engine = crate::LuaEngine::new();
        assert_eq!(engine.set_source("-- ok\n\0").unwrap_err().line, Some(2));
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
