# The PPU is a pipeline

The SNES PPU does not draw shapes. It reads tiles and sprites from memory,
decides which pixel is in front, optionally combines two colors, and sends the
result to the display. Registers control every step.

In ppu.toys, those controls have friendly Lua names. Writing `brightness = 8`
is writing **INIDISP**. Writing `screen.main.bg1 = true` is changing **TM**.
The names are friendlier; the machine underneath is still the PPU.

```lua
function frame(t, f)
  brightness = 15
  mode = 1
  screen.main.bg1 = true
end
```

For screen designation, windows, and color math, ppu.toys also exposes the raw
register mnemonics: `TM`, `TS`, `WH0`–`WH3`, `W12SEL`, `W34SEL`, `WOBJSEL`,
`WBGLOG`, `WOBJLOG`, `TMW`, `TSW`, `CGWSEL`, `CGADSUB`, and `COLDATA`.
The related chapters show both forms.

Raw bytes and friendly fields control the same state. A friendly field changed
in the same frame wins for its bits; untouched bits keep the raw value. The
register inspector always shows the final value sent through the PPU.

## Follow a pixel

1. [Sources](sources.md) become tiles, maps, and palettes in PPU memory.
2. [Backgrounds](backgrounds.md) and [sprites](sprites.md) fetch pixels from VRAM and OAM.
3. [Screens](color-math.md#main-and-sub-screens) decide which layers may appear.
4. [Windows](windows.md) can mask those layers by horizontal region.
5. [Color math](color-math.md) combines the winning main pixel with another color.
6. [Display](display.md) controls the final brightness.

Registers normally apply to the whole frame. Put the same assignments inside
an [`hdma()` hook](scanlines.md) and they can change on each scanline.

## Lua rules

Register values are numbers or booleans. Number writes floor fractional values
and ignore high bits like the hardware. Scroll, sprite positions, and Mode 7
matrix values keep fractions until rendering.

Call `init()` for setup that only needs to happen once. Use `frame(t, f)` for
animation; `t` is elapsed seconds and `f` is the frame number.

```lua
function init()
  mode = 1
end

function frame(t, f)
  bg[1].scroll.x = t * 24
end
```

## What is modelled

ppu.toys renders background modes 0–4 and Mode 7, sprites, windows, main and
sub screens, color math, VRAM, CGRAM, OAM, and per-scanline register changes.
Modes 5 and 6, interlace, overscan, and read-only counter registers are not
modelled yet.
