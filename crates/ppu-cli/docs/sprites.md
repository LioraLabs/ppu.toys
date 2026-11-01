# Sprites

The PPU calls sprites **objects**, or OBJ. Their tile graphics live in VRAM;
their positions and attributes live in OAM. **OBSEL** chooses the shared tile
region and size pair, then each OAM entry places one sprite.

ppu.toys exposes the 128 OAM entries as `obj[0]` through `obj[127]`.

## Put a sprite on screen

```luau
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

- A sprite counts toward a line's 32 only when some of it shows on that line
  horizontally, with one hardware quirk: a sprite at `x = -256` counts too.
- Only slivers on screen count toward the 34 (all of an `x = -256` sprite's).
  The PPU fetches them from the last counted sprite back to the first, so a
  line past 34 loses slivers of its *first* sprites, the ones that draw on
  top.
- A rectangular sprite (16×32, 32×64) flips vertically as two squares, each
  flipped in place.

```luau
function frame(t, f)
  obj[0].x = 128 + math.sin(t) * 48
  obj[0].y = 104
  obj[0].prio = 3
  obj[0].flip_x = false
  obj[0].flip_y = false
  obj[0].large = false
end
```

Object indices must be integers. Use `obj[math.floor(i)]` when an index is computed.

## Animation cells

An imported sprite sheet exposes `cells`. Pick a cell, then apply its tile and
flip information to an OAM entry.

```luau
local hero = dma("hero", { char = 0x6000, pal = 0 })

function frame(t, f)
  local cell = hero.cells[math.floor(t * 8) % #hero.cells + 1]
  obj[0].tile = hero.tile + cell.tile
  obj[0].pal = hero.pal + cell.pal
  obj[0].flip_x = cell.flip_x
  obj[0].flip_y = cell.flip_y
end
```

## Draw sprites in groups · the sprites library

`sprites.draw` places art and leaves the OAM bookkeeping to the library. Make
art once, at file level, from a sprite sheet and its levels of detail: each
level is a grid of the sheet's cells, `columns` wide, centered on the drawn
point. Groups set what draws on top: make them at file level (or in
`init()`), the first on top, and draw into them.

```luau
local sheet = dma("ship", {char = 0x4000, pal = 0})
local ship = sprites.art({
  {size = 16, columns = 1, cells = {0}},
  {size = 32, columns = 2, cells = {1, 2, 3, 4}},
}, sheet)
local dot = sprites.art({{size = 16, columns = 1, cells = {5}}}, sheet)
-- Groups draw in the order they are made: the first on top.
local top = sprites.group()
local world = sprites.group({sort = true})

function frame(t, f)
  obj[0].x, obj[0].y, obj[0].tile, obj[0].on = 8, 8, sheet.tile, true
  top:draw(dot, 128, 100)
  for i = 0, 3 do
    world:draw(ship, 40 + i * 50, 120, {depth = 100 - i * 30, size = 8 + i * 8})
  end
  sprites.draw(dot, 20, 200)
end
```

- `group:draw(art, x, y, {size, prio, key, depth, flip_x, flip_y, pal})`:
  positions round down to whole pixels. `size` picks the first level at least
  that large (the largest otherwise; omitted, the largest). `prio` is the OBJ
  priority against the backgrounds, 0..3 (default 3). `flip_x`, `flip_y` and
  `pal`: see [sizes, flips and palettes](#sizes-flips-and-palettes).
- Groups take the free slots in the order they are made (`sprites.group`
  calls in source order), so a group made earlier draws on top of one made
  later. Plain `sprites.draw` and `sprites.keep` use the default group, which
  draws behind every other group and takes the slots they leave.
- A group has as many slots as its draws can fill in a frame: each draw's
  loops (constant bounds, or a file-level table) times its function's calls,
  by the most cells its art (made at file level, or a file-level table of
  it) can show. When that can't be told, the draw's line is an error: give
  the group `sprites.group({slots = n})`. Groups that need more than the free
  slots are an error at the first that does not fit.
- In a group, draws take its slots in call order; a frame that draws more
  than fit drops that group's last ones. `sprites.group({sort = true})`
  orders its entries by `depth` instead (whole numbers, -32768..32767,
  nearer and lower on top, then call order); `depth` matters only there.
- Slots the program indexes itself, like `obj[0]` above, stay its own and
  are never a group's. Index `obj` with constants, constant `for` bounds,
  or offsets of them, so the slots are known before the program runs.
- `sprites.flicker = true` rotates each unsorted group's unkeyed sprites
  every frame, so a line with more than 32 sprites flickers instead of
  losing the same ones.
- `sprites.hold()` keeps every group's sprites as they were, for a frame
  that draws none (a pause screen). A frame that holds may not also draw.
- A group's slots count sprites: a cell of a hardware size is one, a larger
  cell several (see below).

## Keep sprites that did not move · keys

A keyed draw gives its sprites slots of their own in its group, so a frame
can leave them as they are instead of drawing them again. Give each sprite
that persists a key, an integer 0..127 (keys are the group's own), and keep
the ones that did not move:

```luau
local sheet = dma("ship", {char = 0x4000, pal = 0})
local star = sprites.art({{size = 16, columns = 1, cells = {5}}}, sheet)
local ship = sprites.group()
local stars = sprites.group()
local field: {{x: number, y: number}} = {}
for i = 1, 8 do table.insert(field, {x = i * 28, y = -1}) end

function frame(t, f)
  ship:draw(star, 128, 100 + f % 20)
  for i = 1, 8 do
    local s, y = field[i], (i * 37 + f // 4) % 200
    if y ~= s.y then
      s.y = y
      stars:draw(star, s.x, y, {key = i})
    else
      stars:keep(i)
    end
  end
end
```

- The first draw of key `k` in a group gives it a block of the group's
  slots, as many as the art's largest level has cells, after the blocks
  before it: blocks come first in their group, in the order keys are first
  drawn, then the group's unkeyed sprites. Cell *n* of every later draw of
  `k` goes to slot *n* of its block; a smaller level, or a cell off screen,
  hides its slot. A later draw of `k` with art larger than its block is an
  error.
- `group:keep(k)` leaves key `k`'s sprites exactly as they were. In a sorted
  group a kept entry takes its place by its depth again.
- A key a frame neither draws nor keeps is hidden; keeping it then keeps it
  hidden until it is drawn again.
- A key's first draw shows at the end of its frame; from the next frame on
  the group's unkeyed draws take only the slots after its blocks. Keyed
  sprites are never rotated by `sprites.flicker` or dropped.
- Each of these is an error: drawing or keeping a key twice in one frame,
  drawing and keeping it in one frame, keeping a key never drawn in that
  group, a key outside 0..127, and a first draw that needs more slots than
  its group has left.

## Sizes, flips and palettes

A sheet's cells can be any size from 8×8 to 128×128 in steps of 8
(`cell_size`, and `cell_height` for a cell that is not square). Art takes its
cells' size, and a level can take its cells from another sheet (`sheet =`), so
one art can be an 8×8 sprite far away and a 32×32 one near.

```luau
local small = dma("shots", {char = 0x4000, pal = 0})  -- 8x8 cells
local big = dma("planes", {char = 0x4800, pal = 1})   -- 32x32 cells
local long = dma("barges", {char = 0x5000, pal = 2})  -- 64x32 cells
local shot = sprites.art({{size = 8, columns = 1, cells = {0}}}, small)
local plane = sprites.art({
  {size = 8, columns = 1, cells = {1}, sheet = small},  -- far: one 8x8 sprite
  {size = 32, columns = 1, cells = {1}},                -- near: one 32x32 sprite
}, big)
local barge = sprites.art({{size = 64, columns = 1, cells = {0}}}, long)
local air = sprites.group()

function frame(t, f)
  -- Four headings from one drawing: flips mirror the whole art.
  local heading = f // 8 % 4
  air:draw(plane, 128, 100, {key = 1, flip_x = heading == 1 or heading == 2, flip_y = heading >= 2})
  air:draw(barge, 60, 160, {flip_x = f // 30 % 2 == 1})
  for i = 0, 3 do
    sprites.draw(shot, 40 + i * 20, 60, {pal = 3 + i % 2})
  end
end
```

- **The size pair.** OBSEL shows two sprite sizes at once (`obj.size_sel`).
  When the program never sets `obj.size_sel`, the library sets it once, as
  the program loads: of the pairs that hold every hardware-sized cell the art
  uses (8, 16, 32 or 64 square, 16×32, 32×64), the one that draws the other
  cells with the fewest sprites, the lowest-numbered on a tie. Above, 8×8 and
  32×32 make pair 1. Art in three hardware sizes is an error on the art that
  adds the third: set `obj.size_sel` to the pair to draw with, a constant
  (the art in other sizes is then built from that pair's sprites), or
  re-import a sheet. A program that sets `obj.size_sel` itself, to one
  constant, chooses the pair; the raw `obj[...]` slots draw with it too.
- **Cells of other sizes** are several sprites, one unit to every draw, key,
  keep, flip and group: the pair's large sprites in a grid from the cell's
  corner, then its small ones beside and below them. The 64×32 barge above is
  two 32×32 sprites; a 24×24 cell with pair 0 is one 16×16 and five 8×8.
  Sprites whose tiles are all blank are left out when the art is made. Each
  sprite is clipped on its own: those off screen take no slot.
- **Flips.** `flip_x` and `flip_y` mirror the whole art around the drawn
  point: every sprite's position mirrors and each sprite flips (an 8×8 sheet's
  cell that the importer stored flipped flips back). Rectangular sprites flip
  vertically as two squares (above), so pair 6 and 7 art drawn with `flip_y`
  does not mirror.
- **Palettes.** Each cell draws in its own palette, the sheet's `pal` plus
  the palette the importer gave the cell. `pal = n` (0..7) draws every cell
  of the art in palette n instead.
- A 64-pixel-tall sprite shows only from y = -32 to y = 192: the PPU wraps
  a sprite's rows past line 255 to the top of the screen, so the library
  hides it where it would wrap onto the picture.
- A cell's tiles run right within one 16-tile row of OBJ tiles. Place each
  sheet at a char address that starts a row (a multiple of 0x100 words from
  `obj.char_base`); art whose sprite would cross a row is an error.

## Priority rotation · OAMADD `$2102–$2103`

Normally lower OAM indices are considered first. `obj.first` starts evaluation
at another sprite and enables priority rotation.

```luau
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
