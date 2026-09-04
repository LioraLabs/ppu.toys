# Sprites

The PPU calls sprites **objects**, or OBJ. Their tile graphics live in VRAM;
their positions and attributes live in OAM. **OBSEL** chooses the shared tile
region and size pair, then each OAM entry places one sprite.

ppu.toys exposes the 128 OAM entries as `obj[0]` through `obj[127]`.

## Put a sprite on screen

```lua
local hero = dma("hero", { char = 0x6000, pal = 0 })

function init()
  obj.char_base = hero.char
  screen.main.obj = true

  obj[0].tile = hero.tile
  obj[0].pal = hero.pal
  obj[0].x = 120
  obj[0].y = 96
  obj[0].on = true
end
```

`obj.char_base`, `obj.name_select`, and `obj.size_sel` control **OBSEL `$2101`**.
Every visible object shares those settings. OBSEL packs them into bits 0–2,
3–4, and 5–7 respectively.

## Position and appearance · OAM

Each object chooses a tile, palette, priority, flips, and one of the two sizes
selected by `obj.size_sel`.

```lua
function frame(t, f)
  obj[0].x = 128 + sin(t) * 48
  obj[0].y = 104
  obj[0].prio = 3
  obj[0].flip_x = false
  obj[0].flip_y = false
  obj[0].large = false
end
```

Object indices must be integers. Use `obj[floor(i)]` when an index is computed.

## Animation cells

An imported sprite sheet exposes `cells`. Pick a cell, then apply its tile and
flip information to an OAM entry.

```lua
local cell = hero.cells[floor(t * 8) % #hero.cells + 1]
obj[0].tile = hero.tile + cell.tile
obj[0].pal = hero.pal + cell.pal
obj[0].flip_x = cell.flip_x
obj[0].flip_y = cell.flip_y
```

## Priority rotation · OAMADD `$2102–$2103`

Normally lower OAM indices are considered first. `obj.first` starts evaluation
at another sprite and enables priority rotation.

```lua
obj.first = 32
```

The explicit form is `obj.oam_addr = 64` with `obj.priority_rotate = true`:
each sprite consumes two OAM words, so sprite 32 begins at word address 64.

See also: [Sprite source placement](dma.md#sprites-and-animation-sheets),
[screen designation](color-math.md#main-and-sub-screens), and [windows](windows.md).
