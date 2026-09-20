# Memory and `dma()`

A [source](sources.md) owns graphics data but no hardware address. `dma()` places
that data into VRAM and CGRAM, then returns the addresses that the display
registers need.

Call `dma()` at the top level or in `init()`. Placement is setup work, so it is
not available from `frame()` or an `hdma()` callback.

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

```lua
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

```lua
sky.char
sky.map
sky.pal
sky.next_char
sky.next_map
sky.next_pal
```

## Chain placements

Use the returned `next_*` address instead of repeating size calculations.

```lua
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

```lua
local tiles = dma("tiles", { char = 0x1000, pal = 0 })
```

Its result has `char`, `pal`, `next_char`, `next_pal`, and `bit_depth`.

## Place sprites

All OBJ sources share the character base selected by **OBSEL**. The first
source must align to `0x2000`; later sources can chain by tile.

```lua
local hero = dma("hero", { char = 0x6000, pal = 0 })
local foes = dma("foes", {
  char = hero.next_char,
  pal = hero.next_pal,
})

obj.char_base = hero.char
```

Each result includes `tile`, its offset from the shared OBJ base, plus
`cell_size` and `cells` for animation.

For **OBJ only**, placement `pal` and returned `pal`/`next_pal` are sprite
palette slots (0–7), not absolute CGRAM entries. Slot `p` starts at CGRAM
`128 + p * 16`; `cell.pal` is relative to the source's first slot. Use
`obj[i].pal = hero.pal + cell.pal`. Background/tilesheet `pal` values are
absolute CGRAM entries instead. For example, OBJ `pal = 4` places its first
palette at CGRAM 192, which can keep it clear of a small Mode 7 palette.

## Place Mode 7

Mode 7 has a fixed hardware layout, so it accepts no placement options.

```lua
local floor = dma("floor")
```

Its result is `char = 0`, `map = 0`, `pal = 1`, plus `tiles_w` and `tiles_h`.
Only one Mode 7 source can be placed.

## Placement errors

`dma()` rejects missing sources, invalid alignment, memory overflow, overlapping
VRAM ranges, and overlapping offset CGRAM ranges. Sources may intentionally
share the same CGRAM base; later placements win, like hardware DMA writes.

## Direct memory writes

Use direct writes when Lua generates the data itself.

```lua
vram[0x0000] = 0x1234
cgram[1] = rgb(255, 96, 32)
```

`vr(addr, words)` writes a run of consecutive VRAM words in one call, so a
tilemap row reads as a row:

```lua
vr(0x4000, { 0x0001, 0x0002, 0x0003 })   -- same as vram[0x4000..0x4002]
```

VRAM spans word addresses `0x0000` through `0x7fff`. CGRAM has 256 colors.
Each frame rebuilds VRAM from sources, friendly tilemap writes, and finally raw
`vram[]` writes, so raw writes win when addresses overlap.

See also: [Sources and palettes](sources.md), [Backgrounds](backgrounds.md),
[Sprites](sprites.md), and [scanline palette writes](scanlines.md#per-line-palette-writes).
