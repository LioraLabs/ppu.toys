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

  hdma(0, 223, function(y)
    brightness = floor(y / 14)
  end)
end
```

The callback receives `y`. Assignments inside it affect only that line;
registers you do not touch keep their frame-wide values. `scanline()` is an
alias for `hdma()`.

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

See also: [Background scrolling](backgrounds.md#scroll-bgnhofs-and-bgnvofs-210d-2114),
[Mode 7](mode7.md), [windows](windows.md), and [color math](color-math.md).
