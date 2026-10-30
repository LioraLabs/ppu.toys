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

| `obj.size_sel` | `large = false` | `large = true` |
| -------------- | --------------- | -------------- |
| 0              | 8×8             | 16×16          |
| 1              | 8×8             | 32×32          |
| 2              | 8×8             | 64×64          |
| 3              | 16×16           | 32×32          |
| 4              | 16×16           | 64×64          |
| 5              | 32×32           | 64×64          |
| 6              | 16×32           | 32×64          |
| 7              | 16×32           | 32×32          |

Match the source's `cell_size` to the displayed size. Large sprites fetch
8×8 subtiles in a 16-tile-wide character layout, not as a tightly packed
image. The OBJ importer builds that layout for you.

The 128 entries share per-scanline limits of 32 sprites and 34 eight-pixel
slivers. A 32-pixel-wide sprite uses four slivers on each covered line.
`ppu check` reports actual range/sliver overflow; reducing total OAM usage
alone may not fix a crowded line.

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

## Draw sprites in order · the sprites library

`sprites.draw` places art and leaves the OAM bookkeeping to the library. Make
art once, at file level, from a sprite sheet and its levels of detail: each
level is a grid of 16x16 sheet cells, `columns` wide, centered on the drawn
point.

```lua
local sheet = dma("ship", {char = 0x4000, pal = 0})
local ship = sprites.art({
  {size = 16, columns = 1, cells = {0}},
  {size = 32, columns = 2, cells = {1, 2, 3, 4}},
}, sheet)
local dot = sprites.art({{size = 16, columns = 1, cells = {5}}}, sheet)

function frame(t, f)
  obj[0].x, obj[0].y, obj[0].tile, obj[0].on = 8, 8, sheet.tile, true
  sprites.sort()
  sprites.draw(dot, 128, 100, {layer = 0})
  for i = 0, 3 do
    sprites.draw(ship, 40 + i * 50, 120, {layer = 1, depth = i * 100, size = 8 + i * 8})
  end
end
```

- `sprites.draw(art, x, y, {layer, depth, size, prio})`: positions round down
  to whole pixels. `size` picks the first level at least that large (the
  largest otherwise; omitted, the largest). `prio` is the OBJ priority
  against the backgrounds, 0..3 (default 3).
- After `frame()` the frame's sprites get OAM slots in draw order: the first
  drawn takes the lowest free slot, and a lower slot draws on top.
- `sprites.sort()`, called anywhere in a frame, orders that frame's sprites
  instead: lower `layer` (0..7, default 0) first, then lower `depth` (whole
  numbers, -32768..32767, default 0), then draw order. `layer` and `depth`
  matter only in a frame that sorts. Sorting costs CPU time: draw in the
  order you want shown, and sort when depth must decide it.
- Slots the program indexes itself, like `obj[0]` above, stay its own. Every
  other slot belongs to the library, which hides the ones not drawn this
  frame. Index `obj` with constants, constant `for` bounds, or offsets of
  them, so the slots are known before the program runs.
- A frame keeps the first 128 sprites it draws on screen; later draws are
  dropped.
  Past the free slots, the last sprites in order are dropped. Set
  `sprites.flicker = true` to rotate the slot order every frame, so a line
  with more than 32 sprites flickers instead of losing the same ones.
- `sprites.hold()` keeps last frame's sprites as they were, for a frame that
  draws none (a pause screen). A frame that holds may not also draw.
- Cells are large sprites; keep `obj.size_sel` at 0 for 16x16. The library
  does not flip cells.

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

## Inspect and poke

The Sprites panel opens with a pixel-preview grid. Click an object to inspect
and poke it; **All sprites** returns to the grid. **Show disabled** includes
unused OAM entries. In detail, use the arrows or index selector to cycle
through all 128 objects.

**Inspect line** is shared with the other inspectors. The grid and detail
previews use that line's palette, with each object's flips. Each object is
marked as outside the line, accepted by OAM evaluation, or dropped by sprite
limits. The line summary reports its counts; the frame summary retains the
whole-frame overflow flags. Accepted objects can still be clipped, masked,
or covered by higher-priority pixels.

OAM attributes remain frame-wide. Selecting a line changes inspection, not
sprite placement or poke scope.

Edit position, tile, palette, priority, size, flips, or Enabled to write that
field into `ppuglobals.lua`, which applies it automatically every frame.
Numbers apply on Enter or blur; Escape cancels.
Click a poke marker to remove that field's poke. A hollow marker means the
live value differs: quantization has changed it, not a script override —
controls apply last, so a script write cannot override a poke.
