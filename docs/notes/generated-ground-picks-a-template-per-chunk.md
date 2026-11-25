---
id: generated-ground-picks-a-template-per-chunk
kind: concept
title: Endless ground is a sheet of templates drawn in one Tiled map and a pick function that chooses a template for each chunk as the camera reaches it
codes: []
---
Draw the **templates** side by side in one Tiled map and write `pick(cx, cy)`, which returns a template's number (from 0, left to right, top to bottom) for each **chunk** of the ground. On backgrounds a template is
`chunk = {w, h}` cells, 16 or 32 each (16x16 by default); on the Mode 7 plane they are 16x16. The map's width and height are multiples of the template size, at most 512 cells. Load with
`world.load("shaft", {period = 512, pick = pick, chunk = {32, 16}})`: `period` is the ground's size in cells, and the library runs `pick` once for each chunk that comes into view, never per cell. Every bound layer shows the picked templates at the camera's rate;
a slower far layer is a world of its own. `w:follow(x, y)` takes the camera's top left and clamps nothing. `w.objects` are the template sheet's own, not placed on the ground; `w:enter` and `w:set` are refused.
`w:tile` answers from the same pick, wherever the cell is, loaded or not, so use it to place things on what was generated. On a cartridge `pick` is a file-level `local function` named in the load, and a program has one generated world.

```luau
local seed = makeBounded(0, 0, 9999)
local shown = makeBounded(0, 0, 3)

-- Chunk (cx, cy)'s template: the first two templates near the top, any of the four deeper down.
local function pick(cx: number, cy: number): number
  if cy < 8 then return math.hash(seed, cx, cy) % 2 end
  return math.hash(seed, cx, cy) % 4
end

function init()
  seed = 1225
end

function frame(t, f)
  shown = pick(f % 16, f // 16 % 16)
end
```
