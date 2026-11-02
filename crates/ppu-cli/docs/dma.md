# Memory and `dma()`

A [source](sources.md) owns graphics data but no hardware address. `dma()` places
that data into VRAM and CGRAM, then returns the addresses that the display
registers need.

Call `dma()` at the top level or in `init()`. From `frame()` it is a
[stage load](#load-a-stage), allowed only while the screen is blanked; it is
never available from an `hdma()` callback or a `timer()` hook.

## Place from the Studio

The Studio's DMA panel does the same thing without code: it draws what every
`dma()` call has claimed in VRAM and CGRAM, and **Place** writes one
`dma("name", { char = …, map = …, pal = … })` line into `ppuglobals.lua` at the
first free aligned addresses. **bg1**–**bg4** then poke that layer's
`char_base`/`map_base` and turn it on; **sprites** does the same for
`obj.char_base`. Uploaded samples and the built-in bank get a **Place** too,
chaining into sound RAM. A program's own `dma()` calls show hatched and stay
read-only. Delete the line in `ppuglobals.lua` (or press ×) to unplace.

## Place a background

Backgrounds need character tiles, a tilemap, and palettes.

```luau
local sky = dma("sky", {
  char = 0x1000,
  map = 0x0000,
  pal = 0,
})

function init()
  bg[1].char_base = sky.char
  bg[1].map_base = sky.map
end
```

All VRAM addresses are word addresses. Background `char` placement aligns to
`0x1000`; `map` placement aligns to `0x400`. `pal` is the first CGRAM entry.

The result also includes `bit_depth`, `screen_size`, and the next available
aligned addresses.

```luau
local sky = dma("sky", { char = 0x1000, map = 0x0000, pal = 0 })
local next_free = { char = sky.next_char, map = sky.next_map, pal = sky.next_pal }
```

## Chain placements

Use the returned `next_*` address instead of repeating size calculations.

```luau
local sky = dma("sky", { char = 0x1000, map = 0x0000, pal = 0 })
local hills = dma("hills", {
  char = sky.next_char,
  map = sky.next_map,
  pal = sky.pal,
})
```

Normal layered backgrounds share a palette base because their tilemap palette
bits choose colors from the same CGRAM bands. `next_pal` is useful when the
sources truly need separate palette space; it is not automatically right for
every background mode.

## Place a tilesheet

A tilesheet has no tilemap allocation, so it accepts only `char` and `pal`.

```luau
local tiles = dma("tiles", { char = 0x1000, pal = 0 })
```

Its result has `char`, `pal`, `next_char`, `next_pal`, and `bit_depth`.

## Place sprites

All OBJ sources share the character base selected by **OBSEL**. The first
source must align to `0x2000`; later sources can chain by tile.

```luau
local hero = dma("hero", { char = 0x6000, pal = 0 })
local foes = dma("foes", {
  char = hero.next_char,
  pal = hero.next_pal,
})

obj.char_base = hero.char
```

Each result includes `tile`, its offset from the shared OBJ base, plus
`cell_w`, `cell_h` (`cell_size` is the width) and `cells` for animation.
Cells wider than 8 pixels fill whole 16-tile rows: place the next sheet at a
char address that starts a row (a multiple of 0x100 words from
`obj.char_base`), as the sprites library requires.

For **OBJ only**, placement `pal` and returned `pal`/`next_pal` are sprite
palette slots (0–7), not absolute CGRAM entries. Slot `p` starts at CGRAM
`128 + p * 16`; `cell.pal` is relative to the source's first slot. Use
`obj[i].pal = hero.pal + cell.pal`. Background/tilesheet `pal` values are
absolute CGRAM entries instead. For example, OBJ `pal = 4` places its first
palette at CGRAM 192, which can keep it clear of a small Mode 7 palette.

## Place Mode 7

Mode 7 has a fixed hardware layout, so it accepts no placement options.

```luau
local plane = dma("floor")
```

Its result is `char = 0`, `map = 0`, `pal = 1`, plus `tiles_w` and `tiles_h`.
Every Mode 7 picture lives at that one address, so setup places only one; a
[stage load](#load-a-stage) can put another in its place, the way a game
swaps its ground between stages.

## Load a stage

A game with several stages swaps graphics between them the way SNES games do:
blank the screen, copy the next stage's graphics, show it again. Call `dma()`
from `frame()` (or a helper it calls) with `force_blank = true`:

```luau
function frame(t, f)
  if pad.a then
    -- The next stage: blank, then load its graphics.
    force_blank = true
    dma("tiles", { char = 0x1000, pal = 0 })
    dma("foes", { char = 0x4000, pal = 4 })
  else
    force_blank = false -- the update after the load shows the new stage
  end
end
```

The rules:

- `force_blank` must be true when `dma()` runs and still true when `frame()`
  returns; otherwise the update is an error. Unblank on a later update.
- The data is in VRAM and CGRAM from that update on, and stays until another
  load replaces it. The blanked update shows nothing, so the first picture with
  the new graphics is the first unblanked one.
- A load writes over whatever its addresses held, like hardware DMA: setup
  placements, earlier loads and earlier `vram[]`/`cgram[]` writes alike. Writes
  made after the load win again. Cells drawn with `bg[n].map[col][row]` are kept
  by the program and redrawn every frame, so a load does not clear them; don't
  load over a tilemap you draw cell by cell.
- The result table is the one a setup call returns (`char`, `next_char`, `tile`,
  `cells`, ...). A sprite source's `tile` counts from the sprite base the setup
  placements chose; with no sprite placed at setup, from the load's own `char`,
  which must then align to `0x2000`.
- Placement is checked as at setup (alignment, memory bounds), but a load may
  overlap anything: replacing memory is its purpose. Samples load only at setup.
- The DMA panel's memory map shows the setup layout; loads are not part of it.

A real console copies about 21K words (42 KB) per blanked video frame, so a 16K-word
Mode 7 ground takes most of a frame. The update that loads is simply longer; it
stays blanked, so nothing shows while the copy runs.

## Placement errors

`dma()` rejects missing sources, invalid alignment, memory overflow, overlapping
VRAM ranges, and overlapping offset CGRAM ranges. Sources may intentionally
share the same CGRAM base; later placements win, like hardware DMA writes.

## Direct memory writes

Use direct writes when your code generates the data itself.

```luau
vram[0x0000] = 0x1234
cgram[1] = rgb(255, 96, 32)
```

`vr(addr, words)` writes a run of consecutive VRAM words in one call, so a
tilemap row reads as a row:

```luau
vr(0x4000, { 0x0001, 0x0002, 0x0003 })   -- same as vram[0x4000..0x4002]
```

In a Studio-generated `ppuglobals.lua`, painted tiles live in `apply_vram()`.
The engine evaluates it once when the file loads and overlays those words
onto VRAM every frame, after all of the toy's code has run, so painted tiles cost no frame
time and still win over the program's own writes.

VRAM spans word addresses `0x0000` through `0x7fff`. CGRAM has 256 colors.
Each frame rebuilds VRAM from sources, friendly tilemap writes, and finally raw
`vram[]` writes, so raw writes win when addresses overlap.

See also: [Sources and palettes](sources.md), [Backgrounds](backgrounds.md),
[Sprites](sprites.md), and [scanline palette writes](scanlines.md#per-line-palette-writes).
