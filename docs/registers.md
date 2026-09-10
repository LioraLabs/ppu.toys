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
modelled yet. Sound is modelled too: the S-DSP's eight voices, BRR samples,
timers, and echo — see [Audio](audio.md).

## Register spreadsheet

The Studio Registers panel shows all 40 resolved PPU registers at the selected
scanline. Edit the hex or decimal cell, or click a bit, to write a frame-wide
poke. Enter and Tab apply, Escape cancels, and the up/down arrows move between
rows. Scroll fields accept 13-bit values; Mode 7 matrices accept 16-bit signed
Q8 bit patterns (`FF00` means `-1.0`). COLDATA is a resolved 15-bit color.
Disabled bits are unused or not modelled; raw value entry still follows the
core's masking rules. The selected scanline changes the readout, not the poke's
scope. Use the Backgrounds or Mode 7 panel for scanline keyframes.

Search by address, register, or Lua field; filter by subsystem or existing pokes.
Hover the decoded fields or bit controls for their full meaning. The Poke column
marks registers with pokes and lets you reset them. Cells and bit controls
always use the current live register value. Values are saved in `controls.lua`,
which applies itself automatically every frame as the final override (then
undoes it before the next) — no `apply_pokes()` call is needed, and a poke
wins over both the program's own frame writes and its `hdma()`. `bg3_priority`
now exposes BGMODE bit 3 in Lua.
