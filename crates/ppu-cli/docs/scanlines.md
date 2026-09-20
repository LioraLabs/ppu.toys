# Scanline effects

Most register writes affect the whole frame. `hdma()` runs a Lua callback for
an inclusive range of scanlines, letting the same registers hold a different
value on each line.

That is the PPU's raster-effects superpower: the picture changes while the beam
moves down the screen.

## Change a register per line

```lua
function init()
  mode = 1
  screen.main.bg1 = true
end

function frame(t, f)
  hdma(0, 223, function(y)
    brightness = floor(y / 14)
  end)
end
```

The callback receives `y`. Assignments inside it affect only that line;
registers you do not touch keep their frame-wide values. Hooks live for one
frame: register them inside `frame()`, every frame, like the register writes
they refine — a hook registered in `init()` or at the top level never runs.
`scanline()` is an alias for `hdma()`.

## Built-in interpolation helpers

These helpers are always available in `main.lua` and other sketch files;
no generated `ppuglobals.lua` or imports are needed. The Studio panels use
the same helpers when generating ramps.

| Function | Result |
| --- | --- |
| `clamp(value, min, max)` | Number limited to inclusive bounds; requires `min <= max`, does not round |
| `lerp(a, b, t)` | `a + (b - a) * t`; extrapolates outside `t = 0..1` |
| `lerp_color(a, b, t)` | Blend of two packed 15-bit colors, with `t` clamped to 0..1 |
| `ramp(keys, position)` | Interpolated number, including fractions |
| `ramp_int(keys, position)` | Integer rounded with `floor(value + 0.5)` |
| `ramp_color(keys, position)` | Packed 15-bit color, interpolating each RGB channel separately |
| `ease(progress, easing)` | Eased progress; pass progress from 0 to 1 |

For a simple blend, no keyframe table is needed:

```lua
local progress = clamp(t / 2, 0, 1)
bg[1].scroll.x = floor(lerp(20, 200, progress) + 0.5)
cgram[0] = lerp_color(rgb(255, 0, 0), rgb(0, 0, 255), progress)
```

`lerp_color` uses the same rounded 5-bit RGB channel interpolation as
`ramp_color`. Combine `lerp` with `ease(progress, "smooth")` for an eased blend.

Keys are a nonempty table of `{position, value, optional easing}` entries,
sorted by position. Values hold before the first key and after the last.
Each key's easing controls the segment to the next key: `"linear"` (default),
`"smooth"`, `"ease-in"`, `"ease-out"`, or `"step"`.

```lua
local fade = {{0, 0, "smooth"}, {2, 15}}
local sky = {{0, rgb(16, 32, 96)}, {223, rgb(240, 128, 64)}}

function frame(t, f)
  -- Positions can be seconds as well as scanlines.
  brightness = ramp_int(fade, t)
  hdma(0, 223, function(y)
    cgram[0] = ramp_color(sky, y)
    m7.a = ramp({{0, 1}, {223, 2}}, y)
  end)
end
```

Use `ramp_int` for integer registers such as brightness, scroll, and window
edges; `ramp` preserves fractions for Mode 7 matrix fields. Color ramps take
`rgb(...)`, `hsl(...)`, or packed 15-bit color values, not 24-bit hex colors.

## Raster effects

Offset each line differently for waves, perspective, or a split screen.

```lua
hdma(96, 223, function(y)
  bg[1].scroll.x = sin(y / 12) * 8
end)
```

The callback can change display, background, Mode 7, screen, window, color-math,
and CGRAM state. It cannot call `dma()` or rewrite frame-global OAM.

## Per-line palette writes

A CGRAM assignment inside `hdma()` changes that palette entry for the current
line only.

```lua
hdma(0, 223, function(y)
  cgram[0] = hsl(y * 2, 0.7, 0.35)
end)
```

The Studio's Scanlines panel generates the same shape — a named band is an
`hdma()` hook over an inclusive scanline range (0–223), stored in
`ppuglobals.lua` and applied automatically after your own code runs. That
range is screen space; it is unrelated to the timeline's markers, which name
points in seconds.

See also: [Background scrolling](backgrounds.md#scroll-bgnhofs-and-bgnvofs-210d-2114),
[Mode 7](mode7.md), [windows](windows.md), and [color math](color-math.md).
