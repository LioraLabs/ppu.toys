---
id: ppu-new
kind: recipe
title: ppu new makes a runnable toy folder, and refuses a folder that exists
codes: []
---
`ppu new <dir>` creates the folder with a Luau toy: `main.luau` (a Mode 7 floor with HDMA perspective),
an editable `assets/floor.png`, `ppu.json`, `ppu.d.luau` (every global's declaration, for editors and
agents), the luau-lsp settings, `AGENTS.md` and the agent skills. `--lua` makes a Lua toy instead (a
cartridge builds Luau only, see [[snes.lua_toy]]); `--world` makes a world painted in Tiled, with
`level1.tmj`, a tileset and a Tiled project file. It refuses a directory that already exists, so it never
overwrites a project. To start a game from a template instead, use the Studio's New toy.

Run [[ppu-render|ppu render]] on the new folder to see it, then change `main.luau`.

```sh
ppu new my-toy
ppu render my-toy --at 0,2,4 -o my-toy/build/shots
```
