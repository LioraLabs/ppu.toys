# Mode 7

Mode 7 treats one 128×128-tile plane as a texture and transforms it with a 2D
matrix. **M7A–M7D** rotate, scale, and shear; **M7X/M7Y** choose the center;
**M7SEL** controls flipping and what appears beyond the map.

## Start with an imported plane

Mode 7 has a fixed memory layout, so its `dma()` call needs no addresses.

```luau
local plane = dma("floor")

function init()
  mode = 7
  screen.main.bg1 = true
  m7.cx = 128
  m7.cy = 112
end
```

## Rotate, zoom, or make a floor from the Backgrounds panel

With a Mode 7 texture loaded, open **Backgrounds → Layer settings → Mode 7 view**.
**Rotate / zoom** sets angle (degrees), visual zoom, and texture center.
**Floor** sets horizon, camera height, heading (degrees), and camera position X/Z.
A zoom of 2 makes the texture twice as large. A floor exposes the existing
backdrop at and above its horizon; it does not replace your palette or texture.

These edits use the selected band (or Full frame), share normal poke undo, and
can be released with **clear m7 pokes**. **identity** restores an unrotated,
unscaled screen mapping. Open **Matrix registers** to adjust individual fields.

The generated `ppuglobals.lua` is also the example to copy. It calculates one
view with a built-in helper, then explicitly assigns its fields. The helpers
are always available in `main.lua`, without any generated file:

- `mode7_transform(angle, zoom, x, y)` centers texture `(x,y)` at screen `(128,112)`.
  Angle is in degrees, zoom must be at least `1/127`, and centers wrap at 1024.
- `mode7_floor(y, horizon, height, heading, x, z)` calculates one floor scanline.
  Height is positive, heading is in degrees, and `(x,z)` is the camera position
  on the texture. Its fixed focal length is 128 pixels. Near the horizon,
  scale is capped at 127 to stay within the matrix register range.

Both return a table with `a`, `b`, `c`, `d`, `cx`, `cy`, `scroll_x`, `scroll_y`,
and `visible`. They only calculate values; assignments make them take effect.

```luau
function frame(t, f)
  mode = 7
  brightness = 15
  m7.wrap = 0
  m7.flip_x = false
  m7.flip_y = false
  m7.extbg = false
  hdma(0, 223, function(y)
    local view = mode7_floor(y, 80, 64, 0, 512, 512 - t * 60)
    m7.a = view.a
    m7.b = view.b
    m7.c = view.c
    m7.d = view.d
    m7.cx = view.cx
    m7.cy = view.cy
    bg[1].scroll.x = view.scroll_x
    bg[1].scroll.y = view.scroll_y
    screen.main.bg1 = view.visible
  end)
end
```

For rotation, use `local view = mode7_transform(t * 30, 2, 512, 512)` and the
same assignments directly inside `frame()`, without the `hdma()` wrapper.
Release the corresponding visual pokes after moving their code into `main.lua`,
otherwise the panel overrides still take precedence.

## Transform · M7A–M7D `$211B–$211E`

The identity matrix is `a = 1`, `d = 1`, with `b` and `c` at zero. Rotation is
the familiar cosine-and-sine matrix. The PPU stores M7A–M7D as signed 8.8
fixed-point values; you write the equivalent ordinary numbers.

```luau
function frame(t, f)
  local angle = t * 0.25
  m7.a = math.cos(angle)
  m7.b = -math.sin(angle)
  m7.c = math.sin(angle)
  m7.d = math.cos(angle)
end
```

Scaling the matrix changes how quickly the source plane is sampled.

The matrix maps **screen coordinates to source texels**, not the reverse.
Ignoring fixed-point rounding (and with flips/mosaic disabled), the mapping is
below. The `+ 1` is hardware's line numbering: the top display line is line 1.

```text
px = screen_x + bg[1].scroll.x - m7.cx
py = screen_y + 1 + bg[1].scroll.y - m7.cy
source_x = m7.a * px + m7.b * py + m7.cx
source_y = m7.c * px + m7.d * py + m7.cy
```

Smaller matrix coefficients enlarge the image. To center a texture point
`(cx, cy)` at screen `(128, 112)`, set `m7.cx/cy` to that texture point and
`bg[1].scroll.x/y` to `cx - 128, cy - 113`. For perspective, change the matrix
and source center for each scanline with `hdma()`; the `ppu new` starter
contains a working floor projection.

```luau
function frame(t, f)
  m7.a = 0.5
  m7.d = 0.5
end
```

## A floor for the cartridge · bakeHdma

`mode7_floor` runs in the preview only: a perspective floor recomputed every
frame is too slow for the console's CPU. A floor that only turns can be baked
instead. [`bakeHdma`](luau.md#precompute-math-lookup-and-bakehdma) samples
the scanline program once for every heading, and each frame picks one:

```luau
local heading = makeBounded(0, 0, 63)

local ground = bakeHdma({angle = {0, 63}}, 0, 223, function(y, angle)
  if y < 96 then
    mode = 1
  else
    mode = 7
    local d = 64 / (y - 95)
    local turn = angle * math.pi / 32
    m7.a, m7.b = d * math.cos(turn), d * math.sin(turn)
    m7.c, m7.d = -d * math.sin(turn), d * math.cos(turn)
  end
end)

function frame(t, f)
  if pad.left then heading = (heading + 63) % 64 end
  if pad.right then heading = (heading + 1) % 64 end
  ground(heading)
end
```

## Center and edges · M7X, M7Y, M7SEL

`m7.cx` and `m7.cy` set the transformation center. `m7.flip_x` and
`m7.flip_y` reflect the plane. `m7.wrap` selects what appears outside it:
wrapped pixels, transparency, or tile zero. These are M7SEL bits 0, 1, and 6–7.

```luau
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

```luau
m7.extbg = true
screen.main.bg1 = true
screen.main.bg2 = true
```

See also: [Mode 7 source files](dma.md#mode-7), [scanline transforms](scanlines.md),
and [color math](color-math.md).
