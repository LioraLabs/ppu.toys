# ppu-rules

The engine's dependency-free law: the rules every reader of a toy must apply the same way,
so the preview and any other tool that reads toy sources agree.

- **Owns:** `precision::{denominator, encode, snap}` (the `snes fixed` storage rule: Q10
  expressions, stores rounded down to a binary step of 1/1 to 1/1024), `precision::comments`
  (the Lua comment scanner annotations are read from), and `frontend::LIBRARY` (the `@ppu/`
  require prefix).
- **Doesn't own:** parsing or running Lua, the annotation grammar, precision inference, or the
  engine libraries themselves. Those stay in ppu-core, which re-exports these items at their
  old paths (`ppu_core::precision::encode`, `ppu_core::frontend::LIBRARY`, ...).
- **Depends on:** nothing. Keep it that way: a crate that needs only this law must not pull in
  the Lua VM or the Luau parser.
