---
id: sprite-levels-draw-by-size
kind: recipe
title: An art with several levels draws the first one at least as large as a draw's size, so a far sprite is cheap and a near one detailed
codes: []
fixture: twinstar
---
`sprites.art` takes a list of levels, each a size in pixels and the cells that make it. A draw's `size` option picks the first level at
least that large (the largest when none is, and the largest when `size` is left out). So one art can be a single 8x8 sprite far away and
a 32x32 one near, drawing from different sheets when a level names its own `sheet`. Order the levels smallest first. Each key's slot block is
sized from the art's largest level, so a keyed draw with a smaller level hides the extra slots.

```luau
local big = dma("ships", {palette = "ships_pal", char = 0x4000, pal = 0})
local small = dma("bits", {palette = "bits_pal", char = 0x4800, pal = 6})
local PLANE = sprites.art({
  {size = 8, columns = 1, cells = {3}, sheet = small},
  {size = 16, columns = 1, cells = {0}},
}, big)
local air = sprites.group()

function init()
  mode = 1
  screen.main.obj = true
  obj.char_base = big.char
end

function frame(t, f)
  local near = f // 60 % 2 == 1
  air:draw(PLANE, 128, 104, {size = if near then 16 else 8})
end
```
