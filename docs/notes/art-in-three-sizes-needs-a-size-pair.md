---
id: art-in-three-sizes-needs-a-size-pair
kind: gotcha
title: Sprite art in three hardware sizes is refused, because the hardware shows only two sizes at a time
codes: [snes.sprites]
fixture: twinstar
---
The sprite hardware draws every sprite in one of two sizes, a pair chosen once (`obj.size_sel`, 0 to 7: 8 and 16, 8 and
32, 8 and 64, 16 and 32, 16 and 64, 32 and 64, 16x32 with 32x64, 16x32 with 32x32). When a program never sets it, the
library picks the pair that holds every 8, 16, 32 or 64 square (or 16x32, 32x64) cell the art uses and draws the other
sizes with the fewest sprites. Art in three hardware sizes has no such pair, so the build refuses it
([[snes.sprites]]). Keep a toy to two sizes, or set `obj.size_sel` to one constant pair and let the third size be built
from that pair's sprites ([[odd-sized-cells-are-several-sprites]]).

```luau
local big = dma("ships", {palette = "ships_pal", char = 0x4000, pal = 0})
local small = dma("bits", {palette = "bits_pal", char = 0x4800, pal = 6})
-- 8x8 and 16x16 cells: size pair 0, chosen by the library.
local SPARK = sprites.art({{size = 8, columns = 1, cells = {3}}}, small)
local SHIP = sprites.art({{size = 16, columns = 1, cells = {0}}}, big)
local actors = sprites.group()

function init()
  mode = 1
  screen.main.obj = true
  obj.char_base = big.char
end

function frame(t, f)
  actors:draw(SHIP, 120, 100)
  actors:draw(SPARK, 140, 100)
end
```
