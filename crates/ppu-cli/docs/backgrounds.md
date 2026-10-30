# Backgrounds

A background is a tilemap plus a tileset. The tilemap says which tile appears
in each cell; the tileset supplies its pixels. The BG registers tell the PPU
where both live in VRAM and how to move the layer across the screen.

ppu.toys exposes four layers as `bg[1]` through `bg[4]`. Which ones are usable
depends on the active [background mode](display.md#background-mode-bgmode-2105).

## Put a background on screen

[`dma()`](dma.md) places an imported [source](sources.md) in PPU memory and returns the
addresses its registers need.

```lua
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

```lua
function frame(t, f)
  bg[1].scroll.x = t * 16
  bg[1].scroll.y = sin(t) * 8
end
```

Different speeds create parallax: distant layers move less than nearby ones.

```lua
bg[1].scroll.x = t * 4
bg[2].scroll.x = t * 12
```

## Write a tilemap

For a tilesheet without an imported map, write cells directly. Tilemap entries
select a tile, palette, priority, and optional flips.

```lua
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

`hud_text`, `hud_number` and `hud_clear` share a map descriptor. Coordinates
are zero-based cells in a single 32-cell-stride background map. Load a font
sheet whose tiles contain printable ASCII in order, with space at tile zero:

```lua
local font = dma("font", {char=0x6000, pal=112})
local hud = {map=0x7000, width=32, height=4, palette=7, padding="0"}

function init()
  bg[1].char_base, bg[1].map_base = font.char, hud.map
  screen.main.bg1 = true
  hud_clear(hud)
  hud_text(hud, 1, 1, "SCORE")
end

function frame(t, f)
  hud_number(hud, 7, 1, f % 10000, 6)
end
```

- `hud_text(map, x, y, label, palette)` writes a string. The palette override
  is optional; otherwise it uses `map.palette` (0..7).
- `hud_number(map, x, y, value, width, palette)` writes a right-aligned
  nonnegative integer up to 2,147,483,647 in 1..10 cells. Padding defaults to
  spaces; `map.padding="0"` selects leading zeroes. A value that does not fit
  displays `#` in every field cell. Set `map.wrap=true` to deliberately keep
  only the last digits instead.
- `hud_clear(map, x, y, width, height)` clears a rectangle to tile zero. Omit
  the rectangle to clear the descriptor's whole map.

Fields must fit the descriptor's 1..32-cell width and height. Invalid positions,
fractional numbers, palettes, absent characters and VRAM extents raise errors.
The descriptor names an existing VRAM area; these helpers do not allocate a
font or map. The same Lua helpers run in preview and compile with the game.
For sprites drawn without OAM bookkeeping, in draw order or sorted by layer
and depth, see
[the sprites library](sprites.md#draw-sprites-in-order--the-sprites-library).
Bounded writes participate in automatic VBlank planning. Calls still cost CPU
time; clear static labels once in `init()` or when the screen changes.
