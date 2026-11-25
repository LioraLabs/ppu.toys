---
id: find-the-map-point-under-a-screen-pixel-on-mode-7
kind: recipe
title: On a turned or zoomed Mode 7 plane the map point under a screen pixel is found from the matrix and scroll you set
codes: []
---
For a map that loads whole, `w:follow` does nothing: the matrix (`m7.a` to `m7.d`), its centre (`m7.cx`, `m7.cy`) and `bg[1].scroll` are the camera, set every frame. A cartridge cannot read them back, so keep the values you set in variables and compute from those. `w:tile` and `w.objects` speak in plane pixels, the map's own (8 px a cell; a 16x16 Tiled tile is 2x2 cells), so
on a turned plane a screen pixel is somewhere else on the map. This helper finds where. When the camera turns about the player (`m7.cx, m7.cy` at the player's map position, the scroll putting that point under the sprite) the player is at `(m7.cx, m7.cy)` and needs no helper: use it for anything else on screen.

```luau
-- The matrix and scroll live in variables too: the Mode 7 registers are write-only on a cartridge.
local A, B, C, D = 1, 0, 0, 1
local CX, CY = 512, 512
local SX, SY = 384, 399

-- The plane pixel under screen pixel (sx, sy), for the matrix and scroll you set.
local function plane_at(sx: number, sy: number): (number, number)
  local dx, dy = sx + SX - CX, sy + 1 + SY - CY
  return A * dx + B * dy + CX, C * dx + D * dy + CY
end

local ahead_x = makeBounded(0, -4096, 4096)

function init()
  mode = 7
  m7.a, m7.b, m7.c, m7.d = A, B, C, D
  m7.cx, m7.cy = CX, CY
  bg[1].scroll.x, bg[1].scroll.y = SX, SY
  screen.main.bg1 = true
end

function frame(t, f)
  local ax, _ay = plane_at(128, 40)
  ahead_x = math.clamp(ax, -4096, 4096)
end
```
