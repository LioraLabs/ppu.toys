---
id: snes.assets
kind: diagnostic
title: A cartridge fixes every dma() placement when it is built, so each placement is constant and named once
codes: [snes.assets, snes.assets.layout, snes.assets.dynamic, snes.assets.missing]
fixture: twinstar
---
The `snes.assets.*` refusals all come from one fact: a cartridge decides where each [[dma|dma()]] placement goes in
VRAM and CGRAM when it is built, not while it runs. So:

- `snes.assets.dynamic`: a placement's addresses depend on run-time state, or the `dma()` sits behind a condition. Give
  constant `char`, `map` and `pal`, and call it at file level, above `function init()`. A load mid-game (a stage change)
  goes in `frame()` with `force_blank = true` for that update.
- `snes.assets.layout`: a handle is read before it is assigned, assigned twice, or copied into a local that holds one
  placement or another. Give each placement its own name and branch on which one you read.
- `snes.assets.missing`: the source is not in the project's ppu.json. Add it there (see
  [[place-art-with-dma|placing art]]); a cartridge build only sees the project's listed sources.

```luau
local far = dma("far", {palette = "far_pal", char = 0x6000, map = 0x7800, pal = 28})
local near = dma("near", {palette = "near_pal", char = 0x0000, map = 0x7400, pal = 32})
local stage = makeBounded(0, 0, 1)

function frame(t, f)
  if pad.a then stage = 1 end
  -- Was: local pic = if stage == 0 then far else near
  if stage == 0 then
    bg[1].char_base, bg[1].map_base = far.char, far.map
  else
    bg[1].char_base, bg[1].map_base = near.char, near.map
  end
  screen.main.bg1 = true
end
```
