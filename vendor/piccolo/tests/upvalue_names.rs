use piccolo::{
    compiler::{compile_chunk, interning::BasicInterner, parse_chunk},
    types::UpValueDescriptor, Closure, Executor, Function, FunctionPrototype, Lua,
};

const SRC: &str = "local a, b = 1, 2\nfunction f() return b, a end\nreturn f";

fn names_of(p: &FunctionPrototype<'_>) -> Vec<String> {
    assert_eq!(p.upvalue_names.len(), p.upvalues.len());
    p.upvalue_names.iter().map(|n| n.to_str_lossy().into_owned()).collect()
}

#[test]
fn closure_load_keeps_upvalue_names() {
    let mut lua = Lua::core();
    let ex = lua
        .try_enter(|ctx| {
            let main = Closure::load(ctx, Some("t"), SRC.as_bytes())?;
            Ok(ctx.stash(Executor::start(ctx, main.into(), ())))
        })
        .unwrap();
    lua.finish(&ex);
    lua.try_enter(|ctx| {
        let f: Function = ctx.fetch(&ex).take_result(ctx)??;
        let Function::Closure(c) = f else { panic!("not a closure") };
        let p = c.prototype();
        assert_eq!(names_of(&p), ["b", "a"]);
        assert!(p.upvalues.iter().all(|d| matches!(d, UpValueDescriptor::ParentLocal(_))));
        Ok(())
    })
    .unwrap();
}

#[test]
fn compiled_prototype_path_keeps_upvalue_names() {
    let chunk = parse_chunk(SRC.as_bytes(), BasicInterner::default()).unwrap();
    let compiled = compile_chunk(&chunk, BasicInterner::default()).unwrap();
    assert_eq!(compiled.prototypes[0].upvalue_names.len(), 2);
    let mut lua = Lua::core();
    lua.enter(|ctx| {
        let proto = FunctionPrototype::from_compiled_map_strings(
            &ctx,
            ctx.intern(b"t"),
            &compiled,
            |s| ctx.intern(&s[..]),
        );
        assert_eq!(names_of(&proto.prototypes[0]), ["b", "a"]);
    });
}
