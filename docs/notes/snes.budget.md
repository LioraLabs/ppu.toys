---
id: snes.budget
kind: diagnostic
title: A frame() that needs more 65816 time than one update allows slows the cartridge down, it does not drop work
codes: [snes.budget]
---
`frame() needs at least N video frames of 65816 time per update` means the cartridge's CPU cannot
finish one update in the time the project's rate allows (one video frame at 60 Hz, two at 30 Hz).
Nothing is skipped: as on SNES games, a late update shows one frame later and `f` still advances, so
the game runs slower in real time while it is busy. The message names the lines that take most of the
time, each with the kind of work it is (trig, division, multiply, tilemap writes, scanline rows).

The usual fixes, cheapest first: move maths with a known input range into a [[lookup]] (sines,
divisions, square roots: see [[snes.cost]]), [[bake-scanline-effects|bake scanline effects]] with
[[bakeHdma]], spread work that need not finish this update over several with [[routine]], and redraw
only what changed. A game that is busy on every update can choose a steady 30 Hz instead of slowdown
with `"targetHz": 30` in ppu.json. The estimate is static; the real rate shows when the cartridge runs (in the Studio, `snes_check` counts it). The estimate takes the costlier side of every `if` outside loops and the cheaper side inside them, leaves out the update that enters a scene (its init runs once, like power-on), and counts a loop that can `break` early at half its trips (at most 16).

```luau
-- Was: bg[1].scroll.x = math.floor(math.sin(f * math.pi / 32) * 8), a sine every update.
local SWAY = lookup({t = {0, 63}}, function(t)
  return math.floor(math.sin(t * math.pi / 32) * 8 + 0.5)
end)

function frame(t, f)
  bg[1].scroll.x = SWAY(f % 64)
end
```
