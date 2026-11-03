# Backgrounds

A background is a tilemap plus a tileset. The tilemap says which tile appears
in each cell; the tileset supplies its pixels. The BG registers tell the PPU
where both live in VRAM and how to move the layer across the screen.

ppu.toys exposes four layers as `bg[1]` through `bg[4]`. Which ones are usable
depends on the active [background mode](display.md#background-mode-bgmode-2105).

## Put a background on screen

[`dma()`](dma.md) places an imported [source](sources.md) in PPU memory and returns the
addresses its registers need.

```luau
local sky = dma("sky", { char = 0x1000, map = 0x0000, pal = 0 })

function init()
  mode = 1
  bg[1].char_base = sky.char
  bg[1].map_base = sky.map
  screen.main.bg1 = true
end
```

`char_base` controls **BG12NBA/BG34NBA**. `map_base` and `screen_size` control
**BGnSC**. Imported sources provide the right values, so most toys do not need
to calculate the hardware layout. In BGnSC, bits 0–1 are `screen_size` and bits
2–7 are `map_base >> 10`; BGnNBA stores `char_base >> 12` in a four-bit field.

## Scroll · BGnHOFS and BGnVOFS `$210D–$2114`

Scroll moves the tilemap behind the 256×224 display. Values wrap naturally
when the map wraps.

As on hardware, whose first visible scanline is line 1, the top display line
shows tilemap row `scroll.y + 1`. Set `scroll.y = -1` to put row 0 at the top.

```luau
function frame(t, f)
  bg[1].scroll.x = t * 16
  bg[1].scroll.y = math.sin(t) * 8
end
```

Different speeds create parallax: distant layers move less than nearby ones.

```luau
bg[1].scroll.x = t * 4
bg[2].scroll.x = t * 12
```

## Write a tilemap

For a tilesheet without an imported map, write cells directly. Tilemap entries
select a tile, palette, priority, and optional flips.

```luau
bg[1].map[0] = {}
bg[1].map[0][0] = {
  tile = 3,
  pal = 0,
  prio = 1,
  flip_x = false,
  flip_y = false,
}
```

The indices are `bg[layer].map[column][row]`. Create the column table before
assigning its rows.

See also: [Source placement](dma.md), [main and sub screens](color-math.md#main-and-sub-screens),
and [scanline scrolling](scanlines.md#raster-effects).

## Draw a HUD

The `@ppu/hud` library draws text and numbers on a background map. Its calls
share a map descriptor. Coordinates are zero-based cells in a single
32-cell-stride background map. Load a font sheet whose tiles contain printable
ASCII in order, with space at tile zero:

```luau
local hud = require("@ppu/hud")
local font = dma("font", {char = 0x6000, pal = 112})
local HUD = {map = 0x7000, width = 32, height = 4, palette = 7, padding = "0"}
local score, message

function init()
  bg[1].char_base, bg[1].map_base = font.char, HUD.map
  screen.main.bg1 = true
  hud.text(HUD, 1, 1, "SCORE")
  score = hud.field(HUD, 7, 1, 6)
  message = hud.line(HUD, 1, 2, 12)
end

function frame(t, f)
  score:set(f % 10000)
  message:set(if f % 120 < 60 then "GET READY" else "")
end
```

The HUD is retained: write a label once, and keep the fields and lines a screen
shows. Setting one to the value it already shows only compares.

- `hud.text(map, x, y, label, palette)` writes a string once. The palette
  override is optional; otherwise it uses `map.palette` (0..7). Writing spaces
  blanks cells.
- `hud.field(map, x, y, width, palette)` draws a right-aligned number field of
  1..9 cells showing 0, and returns it. `field:set(value)` shows a nonnegative
  integer that fits the width, rewriting only the digits that changed. Padding
  is spaces; `map.padding="0"` selects leading zeroes.
- `hud.line(map, x, y, width, palette)` returns a line of up to `width` cells,
  blank until set. `line:set(text)` writes the new text and blanks the cells the
  previous text covered beyond it; `line:set("")` clears it.

Fields must fit the descriptor's 1..32-cell width and height. Invalid positions,
fractional or oversized numbers, palettes, absent characters and VRAM extents
raise errors. The descriptor names an existing VRAM area; the library does not
allocate a font or map. The same library runs in preview and compiles with the
game. Making a field draws its cells, so make it again after something else
overwrote them. Bounded writes participate in automatic VBlank planning.

The built-in `hud_text`, `hud_number` and `hud_clear` are deprecated in favour
of `@ppu/hud`. They still run in the preview, so older toys keep working, but a
cartridge build refuses them.

For sprites drawn without OAM bookkeeping, in draw order or sorted by layer
and depth, see
[the sprites library](sprites.md#draw-sprites-in-order--the-sprites-library).
