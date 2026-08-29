# Source placement and `dma()`

`dma(name, options)` copies an imported source into PPU memory and returns the
resolved placement. Call it in top-level code or `init()`, never in `frame()` or
an `hdma()` callback. Setup reruns when a source is added, replaced, or removed,
so returned sizes and chained addresses stay current.

All VRAM addresses are 16-bit word addresses. `dma()` rejects missing sources,
invalid alignment, memory overflow, VRAM overlap, and offset CGRAM overlap.
Sources may intentionally share the same CGRAM base; later DMA calls win, like
the hardware.

## Background

An assembled `bg` source contains character tiles, a tilemap, and palettes.

```lua
local sky = dma("sky", { char = 0x1000, map = 0x0000, pal = 0 })
local hills = dma("hills", {
  char = sky.next_char,
  map = sky.next_map,
  pal = sky.pal, -- normal BG layers share a palette base
})

function frame(t, f)
  mode = 1
  screen.main.bg1 = true
  bg[1].char_base = sky.char
  bg[1].map_base = sky.map
end
```

The result is:

```lua
{
  char, map, pal,
  next_char, next_map, next_pal,
  bit_depth, screen_size,
}
```

`char` is aligned to `0x1000`; `map` is aligned to `0x400`. `next_*` values
include the alignment required by the next placement. `next_pal` reports the
end of the copied CGRAM block, but BG palette selection still follows the
active mode and tilemap palette bits; it is not blindly chainable. Normal
layered backgrounds share the same `pal` base and are authored with compatible
colors. Mode 0 can use its per-layer CGRAM bands. An 8bpp background consumes
the full BG palette, so another independent BG palette cannot coexist.

## Tilesheet

A `sheet` source contains BG character tiles and palettes, but no tilemap.
The program owns tilemap geometry and entries.

```lua
local tiles = dma("tiles", { char = 0x1000, pal = 0 })

function frame(t, f)
  bg[1].char_base = tiles.char
  bg[1].map_base = 0
  bg[1].map[0] = {}
  bg[1].map[0][0] = { tile = 3, pal = 0 }
end
```

The result is `{ char, pal, next_char, next_pal, bit_depth }`. There is no
`map` or `next_map` because the source does not allocate a tilemap.

## Sprites and animation sheets

An `obj` source contains 4bpp OBJ characters, OBJ palettes, and its imported
cell mapping. All OBJ sources in a frame share one hardware `obj.char_base`.

```lua
local hero = dma("hero", { char = 0x6000, pal = 0 })
local foes = dma("foes", {
  char = hero.next_char,
  pal = hero.next_pal,
})

function frame(t, f)
  obj.char_base = hero.char
  screen.main.obj = true

  local cell = hero.cells[floor(t * 8) % #hero.cells + 1]
  obj[0].tile = hero.tile + cell.tile
  obj[0].pal = hero.pal + cell.pal
  obj[0].flip_x = cell.flip_x
  obj[0].flip_y = cell.flip_y
  obj[0].on = true
end
```

The result is:

```lua
{
  char, pal, tile,
  next_char, next_pal,
  cell_size,
  cells = { { tile, pal, flip_x, flip_y }, ... },
}
```

`tile` is the source's offset from the shared OBJ base. Each image cell is an
animation frame at the imported `cell_size`; animation timing and state remain
ordinary Lua code. The first OBJ `char` must align to `0x2000`. Chained OBJ
characters are tile-aligned and must fit the shared 512-tile region. OBJ `pal`
is a palette number from 0 through 7.

## Mode 7

A Mode 7 source has fixed hardware placement and accepts no options.

```lua
local floor = dma("floor")

function frame(t, f)
  mode = 7
  screen.main.bg1 = true
  m7.cx = 128
  m7.cy = 112
end
```

The result is `{ char = 0, map = 0, pal = 1, tiles_w, tiles_h }`. Mode 7
characters and map bytes are interleaved in VRAM beginning at `0x0000`; palette
index 0 stays transparent and colors begin at CGRAM 1. Only one Mode 7 source
can be placed, so it deliberately has no `next_*` fields.

### EXTBG priority mask

EXTBG sources pair the color PNG with an equally sized black/white PNG:

```json
{
  "name": "floor",
  "kind": "m7",
  "options": { "extbg": true },
  "file": "assets/floor.png",
  "priority_file": "assets/floor-priority.png"
}
```

The converter limits the color image to 127 colors. Mask luminance below 128
clears pixel bit 7 for BG1; luminance 128 or above sets bit 7 for BG2. Transparent
color pixels remain index 0 regardless of the mask. The bit is applied before
tile deduplication, and both images compile into one self-contained payload.

```lua
local floor = dma("floor")

function frame(t, f)
  mode = 7
  m7.extbg = true
  screen.main.bg1 = true -- low-priority mask pixels
  screen.main.bg2 = true -- high-priority mask pixels
end
```
