# Sources and palettes

A source is a PNG translated into data the SNES PPU can actually read: tiles,
tilemaps, and BGR555 palette colors. Add one from the Studio's Sources panel,
choose what the image represents, then place it in memory with [`dma()`](dma.md).

The preview shows the quantized image, its palettes, tile use, and any hardware
limit the conversion reached.

## Choose a source kind

**Background** builds a complete scrolling layer: 8×8 tiles, a tilemap, and
palettes. Use it when the PNG already has the layout you want.

**Tilesheet** reads 8×8 cells from left to right, top to bottom. It preserves
that tile order and creates no tilemap; Lua decides where each tile goes.

**Sprite sheet** divides the image into equally sized OBJ cells. Each cell
becomes an animation frame with its tile, palette, and flip information kept.

**Mode 7** builds one 8bpp plane with its fixed hardware memory layout. An
optional black-and-white EXTBG image supplies per-pixel priority.

## Bit depth and palettes

Bit depth controls how many palette entries one tile can address.

| Format | Colors per palette | Palettes | Typical use |
|---|---|---|---|
| 2bpp | 4 including transparency | Up to 8 | Mode 0 and simple art |
| 4bpp | 16 including transparency | Up to 8 | Mode 1 backgrounds and sprites |
| 8bpp | 256 including transparency | 1 | Rich backgrounds and Mode 7 |

Transparent pixels use entry 0. The remaining colors are converted from PNG
RGB to the SNES's 15-bit BGR555 color format. If the image has too many colors,
the importer reduces them and reports the overflow instead of hiding it.

In Lua, `pal` selects a sub-palette. The actual colors live in CGRAM.

```lua
bg[1].map[0][0] = { tile = 3, pal = 2 }
obj[0].pal = 4
```

You can also write a CGRAM color directly.

```lua
cgram[1] = rgb(255, 96, 32)
cgram[2] = hsl(210, 0.7, 0.5)
```

## Dithering

Dithering trades clean color regions for smoother-looking gradients. Ordered
dither keeps patterns regular. Diffusion usually makes gradients smoother but
can create many unique tiles, using more VRAM.

Start with no dithering. Add it when visible banding matters, then check the
source preview's tile count.

## Alpha threshold

PNG alpha becomes a binary hardware decision: transparent or opaque. The alpha
threshold chooses the dividing line. Raise it to discard faint edge pixels;
lower it to keep them.

## Background sources

```lua
local sky = dma("sky", { char = 0x1000, map = 0x0000, pal = 0 })

function init()
  bg[1].char_base = sky.char
  bg[1].map_base = sky.map
  screen.main.bg1 = true
end
```

## Tilesheet sources

```lua
local tiles = dma("tiles", { char = 0x1000, pal = 0 })

bg[1].char_base = tiles.char
bg[1].map_base = 0
bg[1].map[0] = {}
bg[1].map[0][0] = { tile = 3, pal = 0 }
```

## Sprite sources

Sprite sheets must be a uniform grid with no margins. Choose the cell size that
matches one sprite; crop irregular downloaded sheets before importing them.

```lua
local hero = dma("hero", { char = 0x6000, pal = 0 })

obj.char_base = hero.char
local cell = hero.cells[1]
obj[0].tile = hero.tile + cell.tile
obj[0].pal = hero.pal + cell.pal
```

## Mode 7 sources

```lua
local floor = dma("floor")

mode = 7
screen.main.bg1 = true
```

See also: [`dma()` placement](dma.md), [Backgrounds](backgrounds.md),
[Sprites](sprites.md), and [Mode 7](mode7.md).
