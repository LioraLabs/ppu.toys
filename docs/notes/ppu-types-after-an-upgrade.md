---
id: ppu-types-after-an-upgrade
kind: recipe
title: After upgrading ppu, run ppu types so the declarations and library stubs match the new engine
codes: []
---
`ppu.d.luau` declares every global and `.ppu/lib/` holds typed stubs for each `@ppu/<name>` library;
both come from inside the `ppu` binary, so an older copy in a toy folder describes an older engine. `ppu
types <dir>` rewrites them, and writes `.luaurc` (the `@ppu/` mappings) and `.vscode/settings.json` (the
definition file, the new type solver) only where they are missing, so settings you edited stay. It also
refreshes the agent skills ([[ppu-skills-refresh]]). `ppu new` and `ppu connect` write the same files
when they make a Luau folder. Any editor that runs the stock luau-lsp reads them: point it at
`ppu.d.luau` with the new solver on.

```sh
ppu --version
ppu types .
```
