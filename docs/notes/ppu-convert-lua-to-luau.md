---
id: ppu-convert-lua-to-luau
kind: recipe
title: ppu convert turns a Lua toy into a Luau toy and lists what it changed and what it could not
codes: []
---
A cartridge builds Luau only ([[snes.lua_toy]]), so a Lua toy is converted first: `ppu convert <dir> -o
<new dir>` (either side may be a `.ppu.json` file). Every `.lua` file becomes `.luau`; Lua 5.3 bitwise
operators become arithmetic where that is exact (`x & 31` is `x % 32`, `x << 3` is `x * 8`) and `bit32`
calls otherwise; and `main.luau` requires the other files in their old tab order. Comments and
`---@snes` annotations stay. Each change prints with its file and line, and anything Luau cannot say
(`goto`, for one) prints as `not converted`: fix those by hand. A Luau toy converts to itself, unchanged.
The Studio's Convert to Luau and the agent tool `convert_to_luau` keep the Lua original beside the result.

```sh
ppu new old-lua-toy --lua
ppu convert old-lua-toy -o luau-toy
ppu check luau-toy
```
