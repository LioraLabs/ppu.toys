# Mode 7

Mode 7 treats one 128×128-tile plane as a texture and transforms it with a 2D
matrix. **M7A–M7D** rotate, scale, and shear; **M7X/M7Y** choose the center;
**M7SEL** controls flipping and what appears beyond the map.

## Start with an imported plane

Mode 7 has a fixed memory layout, so its `dma()` call needs no addresses.

```lua
local floor = dma("floor")

function init()
  mode = 7
  screen.main.bg1 = true
  m7.cx = 128
  m7.cy = 112
end
```

## Transform · M7A–M7D `$211B–$211E`

The identity matrix is `a = 1`, `d = 1`, with `b` and `c` at zero. Rotation is
the familiar cosine-and-sine matrix. The PPU stores M7A–M7D as signed 8.8
fixed-point values; Lua lets you write the equivalent ordinary numbers.

```lua
function frame(t, f)
  local angle = t * 0.25
  m7.a = cos(angle)
  m7.b = -sin(angle)
  m7.c = sin(angle)
  m7.d = cos(angle)
end
```

Scaling the matrix changes how quickly the source plane is sampled.

The matrix maps **screen coordinates to source texels**, not the reverse.
Ignoring fixed-point rounding (and with flips/mosaic disabled), the mapping is:

```text
px = screen_x + bg[1].scroll.x - m7.cx
py = screen_y + bg[1].scroll.y - m7.cy
source_x = m7.a * px + m7.b * py + m7.cx
source_y = m7.c * px + m7.d * py + m7.cy
```

Smaller matrix coefficients enlarge the image. To center a texture point
`(cx, cy)` at screen `(128, 112)`, set `m7.cx/cy` to that texture point and
`bg[1].scroll.x/y` to `cx - 128, cy - 112`. For perspective, change the matrix
and source center for each scanline with `hdma()`; the `ppu new` starter
contains a working floor projection.

```lua
function frame(t, f)
  m7.a = 0.5
  m7.d = 0.5
end
```

## Center and edges · M7X, M7Y, M7SEL

`m7.cx` and `m7.cy` set the transformation center. `m7.flip_x` and
`m7.flip_y` reflect the plane. `m7.wrap` selects what appears outside it:
wrapped pixels, transparency, or tile zero. These are M7SEL bits 0, 1, and 6–7.

```lua
m7.cx = 128
m7.cy = 112
m7.wrap = 0
```

`m7.wrap = 0` or `1` wraps at the full 1024×1024 plane boundary; `2` makes
out-of-plane samples transparent; `3` samples tile zero outside the plane.
An imported image smaller than the plane is placed at its top-left, with
uncovered map cells left as tile zero. Wrapping does not repeat the smaller
image at its own dimensions. Repeat a texture across the source PNG or
explicitly fill the Mode 7 tilemap when a repeating plane is wanted.

## EXTBG · SETINI `$2133`

EXTBG lets Mode 7 pixel bit 7 split the plane between BG1 and BG2 priorities.
Enable both layers to show both parts of an imported EXTBG source.

```lua
m7.extbg = true
screen.main.bg1 = true
screen.main.bg2 = true
```

See also: [Mode 7 source files](dma.md#mode-7), [scanline transforms](scanlines.md),
and [color math](color-math.md).
