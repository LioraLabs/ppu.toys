---
id: typewriter-text-with-line-set
kind: recipe
title: Typewriter text is line:set(text, count) with count rising each frame, writing one new cell a frame
codes: []
fixture: adventure
---
[[hud.line|`line:set(text, count)`]] shows the first `count` characters of `text`. Raise `count` by one a
frame over the same text and the line writes only the new cell, so typing a dialog box costs one cell a
frame. For a box of several lines, keep one typed count for the whole page and give each line the
characters of it that fall on that line. A press of A shows the rest at once, then turns the page. The
lines are made once, blank until set; setting `""` clears them. While the box is open the game stops and
[[sprites-hold-for-a-pause|holds its sprites]]. Validate page sizes at load with `error()`, naming the
speaker, so a line too long for the box is refused before play.

```luau
local hud = require("@ppu/hud")
local font = dma("font", {palette = "font_pal", char = 0x5000, pal = 28})
local HUD = {map = 0x5400, width = 32, height = 28, palette = 7}
local PAGE = {"Welcome to Brackenford,", "traveller. Mind the moths", "by the lake."}
local rows = {hud.line(HUD, 3, 21, 25), hud.line(HUD, 3, 23, 25), hud.line(HUD, 3, 25, 25)}
local shown = makeBounded(0, 0, 75)

function init()
  mode = 1
  bg[3].char_base, bg[3].map_base = font.char, HUD.map
  screen.main.bg3 = true
end

function frame(t, f)
  local total = 0
  for i = 1, 3 do total += #PAGE[i] end
  shown = if pad.pressed.a then total else math.min(shown + 1, total)
  local left = shown
  for i = 1, 3 do
    local n = math.min(left, #PAGE[i])
    rows[i]:set(PAGE[i], n)
    left -= n
  end
end
```
