---
id: snes.code_size
kind: diagnostic
title: One function, or one top-level setup statement, must fit a 32 KiB ROM bank
codes: [snes.code_size]
---
`error[snes.code_size]` means one piece of code is bigger than a 32 KiB ROM bank. A function, `frame()` included, is
one piece and is never split; setup code splits only between its top-level statements. No cartridge size changes
this: the limit is per bank.

Move part of the named function's body into helpers it calls, one per job (`update_players()`, `update_enemies()`), or
split a long setup statement into several. When the message names the sprites library instead, declare fewer
[[sprites.art]] levels or fewer distinct multi-cell layouts: the library's code grows with them.

```luau
local function update_players(f: number)
  bg[1].scroll.x = f % 256
end

local function update_effects(f: number)
  brightness = 15
end

function frame(t, f)
  update_players(f)
  update_effects(f)
end
```
