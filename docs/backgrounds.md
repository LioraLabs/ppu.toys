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
