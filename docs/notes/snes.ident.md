---
id: snes.ident
kind: diagnostic
title: The hosted ident plays before the game and hands over to its frame(), so names starting __ppu_ are taken
codes: [snes.ident]
---
`error[snes.ident]` means the ppu.toys ident (the short logo screen a cartridge plays before the game, for 8 seconds
or until a button is pressed and released) cannot be put in front of this toy. Either a file or source is named with
the `__ppu_` prefix, which the ident reserves (rename it), or the game defines no `frame()` the ident can hand over to:
define `function frame(t, f)` at file level, or add scenes with [[scenes.add]] (see [[snes.entry]]).

```luau
function frame(t, f)
  brightness = 15
  bg[1].scroll.x = f % 256
end
```
