use std::cmp::Ordering;

use piccolo::{Lua, Table, Value};

#[test]
fn test_table_iter() {
    let mut lua = Lua::core();

    lua.enter(|ctx| {
        let table = Table::new(&ctx);

        table.set(ctx, 1, "1").unwrap();
        table.set(ctx, 2, "2").unwrap();
        table.set(ctx, 3, "3").unwrap();
        table.set(ctx, "1", 1).unwrap();
        table.set(ctx, "2", 2).unwrap();
        table.set(ctx, "3", 3).unwrap();

        let mut pairs = table.iter().collect::<Vec<_>>();
        pairs.sort_by(|&(ak, _), &(bk, _)| match (ak, bk) {
            (piccolo::Value::Integer(a), piccolo::Value::Integer(b)) => a.cmp(&b),
            (piccolo::Value::Integer(_), piccolo::Value::String(_)) => Ordering::Less,
            (piccolo::Value::String(_), piccolo::Value::Integer(_)) => Ordering::Greater,
            (piccolo::Value::String(a), piccolo::Value::String(b)) => a.cmp(&b),
            _ => unreachable!(),
        });

        assert_eq!(pairs.len(), 6);
        assert!(matches!(pairs[0], (Value::Integer(1), Value::String(s)) if s == "1" ));
        assert!(matches!(pairs[1], (Value::Integer(2), Value::String(s)) if s == "2" ));
        assert!(matches!(pairs[2], (Value::Integer(3), Value::String(s)) if s == "3" ));
        assert!(matches!(pairs[3], (Value::String(s), Value::Integer(1)) if s == "1" ));
        assert!(matches!(pairs[4], (Value::String(s), Value::Integer(2)) if s == "2" ));
        assert!(matches!(pairs[5], (Value::String(s), Value::Integer(3)) if s == "3" ));

        for (k, _) in table.iter() {
            table.set(ctx, k, Value::Nil).unwrap();
        }

        assert!(table.get(ctx, 1).is_nil());
        assert!(table.get(ctx, 2).is_nil());
        assert!(table.get(ctx, 3).is_nil());
        assert!(table.get(ctx, "1").is_nil());
        assert!(table.get(ctx, "2").is_nil());
        assert!(table.get(ctx, "3").is_nil());
    });
}

// LOCAL DELTA (PPU-1048): a key removed and set again through a different string object
// leaves a dead bucket beside the live one. `next` from that key must land on the live
// bucket, or iteration returns the same key forever (it hung ppu-core's hot-swap carry).
#[test]
fn test_next_after_resurrected_string_key() {
    let mut lua = Lua::core();

    lua.enter(|ctx| {
        for fill in 0..64 {
            let table = Table::new(&ctx);
            for i in 0..fill {
                table.set(ctx, piccolo::String::from_slice(&ctx, format!("k{i}")), i as i64).unwrap();
            }
            let old = piccolo::String::from_slice(&ctx, "x");
            let new = piccolo::String::from_slice(&ctx, "x");
            table.set(ctx, old, 1).unwrap();
            table.set(ctx, old, Value::Nil).unwrap();
            table.set(ctx, new, 1).unwrap();
            for key in [old, new] {
                if let piccolo::table::NextValue::Found { key: k, .. } = table.next(key.into()) {
                    assert!(!matches!(k, Value::String(s) if s == "x"), "fill {fill}");
                }
            }
            assert_eq!(table.iter().count(), fill + 1, "fill {fill}");
            // `old` stands for a new string at a freed key's address: setting through it must
            // reach the live key, not resurrect the dead bucket beside it.
            table.set(ctx, old, 2).unwrap();
            assert_eq!(table.iter().count(), fill + 1, "fill {fill}");
            assert!(matches!(table.get(ctx, new), Value::Integer(2)), "fill {fill}");
            table.set(ctx, new, Value::Nil).unwrap();
            assert!(table.get(ctx, old).is_nil(), "fill {fill}");
        }
    });
}
