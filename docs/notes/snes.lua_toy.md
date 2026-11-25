---
id: snes.lua_toy
kind: diagnostic
title: A cartridge builds Luau toys only, so a Lua toy is converted first
codes: [snes.lua_toy]
---
`error[snes.lua_toy]: This is a Lua toy, and the cartridge builds Luau only` means the project has
no `main.luau`, only Lua files. The preview still runs Lua, but a cartridge build starts from
Luau. Convert the toy: in the Studio press Convert to Luau (an agent calls `convert_to_luau`; on
the command line `ppu convert <dir> -o <dir>`). Conversion keeps the Lua original beside the new
files. After that, `main.luau` holds `frame(t, f)` and the build goes on.

```luau
function frame(t: number, f: number)
  brightness = 15
  cgram[0] = rgb(24, 16, 72)
end
```
