# PPU registers → Lua

ppu.toys is Lua and registers. Every PPU register the engine models is a Lua
global or a field on one of a few tables. This page is the map. Addresses are
the SNES `$21xx` write registers; the Lua column is what you assign in
`init()`, `frame(t, f)`, or an `hdma()` hook.

**Numbers.** Every register value and every memory-table key floors floats:
`brightness = 7.9` is 7, `cgram[i / 2]` is entry `floor(i / 2)`. Out-of-range
values wrap or mask to the register's width the way the hardware ignores high
bits. The only true floats are `bg[n].scroll.x/.y`, `obj[n].x/.y` (sub-pixel,
quantized at render) and the Mode 7 matrix `m7.a/.b/.c/.d/.cx/.cy`.

**Two dialects, one byte.** Raw mnemonics (`TM`, `CGADSUB`, `WH0`…) and the
friendly namespaces (`screen`, `color`, `win`) write the same register. Bits you
move through a friendly field win over a same-frame raw write; bits you never
touch keep the raw byte. Use either, or both.

## Display

| Register | Lua | Range | Notes |
|---|---|---|---|
| `$2100` INIDISP | `brightness` | 0..15 | default 15 |
| `$2100` INIDISP.7 | `force_blank` | bool | screen black, VRAM writable any time |
| `$2105` BGMODE | `mode` | 0..7 | default 1; modes 0–4 and 7 render, 5 and 6 are not rendered yet |
| `$2105` BGMODE.4-7 | `bg[n].tile_size` | 8 or 16 | per layer |
| `$2106` MOSAIC | `mosaic` | 0..15 | block size; enable per layer with `bg[n].mosaic = true` |
| `$2133` SETINI.6 | `m7.extbg` | bool | Mode 7 EXTBG per-pixel priority |

## Backgrounds (`bg[1..4]`)

| Register | Lua | Range | Notes |
|---|---|---|---|
| `$2107`–`$210A` BGnSC | `bg[n].map_base` | VRAM word address | snapped to `0x400` |
| `$2107`–`$210A` BGnSC.0-1 | `bg[n].screen_size` | 0..3 | 32×32, 64×32, 32×64, 64×64 |
| `$210B`/`$210C` BG12NBA/BG34NBA | `bg[n].char_base` | VRAM word address | snapped to `0x1000` |
| `$210D`–`$2114` BGnHOFS/BGnVOFS | `bg[n].scroll.x`, `bg[n].scroll.y` | float | sub-pixel kept, quantized at render |
| — | `bg[n].visible` | bool | playground toggle, not a register |
| tilemap words | `bg[n].map[col][row] = { tile, pal, prio, flip_x, flip_y }` | tile 0..1023, pal 0..7, prio 0/1 | packs the real 16-bit entry at `map_base`; create the row table first: `bg[1].map[0] = {}` |

## Mode 7 (`m7`)

| Register | Lua | Range | Notes |
|---|---|---|---|
| `$211A` M7SEL.0-1 | `m7.flip_x`, `m7.flip_y` | bool | |
| `$211A` M7SEL.6-7 | `m7.wrap` | 0..3 | screen-over: wrap, wrap, transparent, tile 0 |
| `$211B`–`$211E` M7A–M7D | `m7.a`, `m7.b`, `m7.c`, `m7.d` | float | 1.0 = identity; hardware 8.8 fixed |
| `$211F`/`$2120` M7X/M7Y | `m7.cx`, `m7.cy` | float | rotation center |
| map bytes | `m7.map[ty][tx] = tile` | 0..255 | low byte of the interleaved word; 128 tiles wide |
| char bytes | `m7pixel(tile, x, y, index)` | index 0..255 | high byte lane, 8bpp |

## Sprites (`obj`)

| Register | Lua | Range | Notes |
|---|---|---|---|
| `$2101` OBSEL.0-2 | `obj.char_base` | VRAM word address | snapped to `0x2000` |
| `$2101` OBSEL.3-4 | `obj.name_select` | 0..3 | second name table gap |
| `$2101` OBSEL.5-7 | `obj.size_sel` | 0..7 | small/large pair |
| `$2102`/`$2103` OAMADD | `obj.oam_addr`, `obj.priority_rotate` | 0..511, bool | or the sugar `obj.first = N` |
| OAM | `obj[i].x`, `obj[i].y` | float | i = 0..127; sub-pixel quantized |
| OAM | `obj[i].tile` | 0..511 | |
| OAM | `obj[i].pal` | 0..7 | CGRAM 128 + 16·pal |
| OAM | `obj[i].prio` | 0..3 | |
| OAM | `obj[i].flip_x`, `obj[i].flip_y`, `obj[i].large` | bool | |
| — | `obj[i].on` | bool | playground enable; off = not in OAM |

`obj[i]` needs an integer `i`; index with `obj[floor(k)]` when `k` is computed.

## Screen designation (`screen`)

| Register | Raw | Friendly | Notes |
|---|---|---|---|
| `$212C` TM | `TM` bitmask | `screen.main.bg1..bg4`, `screen.main.obj` | bool each; power-on: all off |
| `$212D` TS | `TS` bitmask | `screen.sub.bg1..bg4`, `screen.sub.obj` | |

## Windows (`win`)

| Register | Raw | Friendly | Notes |
|---|---|---|---|
| `$2126`/`$2127` WH0/WH1 | `WH0`, `WH1` | `win.w1.lo`, `win.w1.hi` | 0..255 |
| `$2128`/`$2129` WH2/WH3 | `WH2`, `WH3` | `win.w2.lo`, `win.w2.hi` | 0..255 |
| `$2123` W12SEL | `W12SEL` | `win.bg1.w1/.w2/.invert`, `win.bg2.…` | bool |
| `$2124` W34SEL | `W34SEL` | `win.bg3.…`, `win.bg4.…` | bool |
| `$2125` WOBJSEL | `WOBJSEL` | `win.obj.…`, `win.color.…` | bool |
| `$212A` WBGLOG | `WBGLOG` | `win.bg1..bg4.combine` | `"OR"`, `"AND"`, `"XOR"`, `"XNOR"` |
| `$212B` WOBJLOG | `WOBJLOG` | `win.obj.combine`, `win.color.combine` | |
| `$212E` TMW | `TMW` | `win.<layer>.main` | bool; mask on main screen (BG/OBJ only) |
| `$212F` TSW | `TSW` | `win.<layer>.sub` | bool; mask on sub screen |

## Color math (`color`)

| Register | Raw | Friendly | Notes |
|---|---|---|---|
| `$2130` CGWSEL.0 | `CGWSEL` | `direct_color` | bool, 8bpp direct color |
| `$2130` CGWSEL.1 | | `color.addend` | `"sub"` (sub screen) or `"fixed"` |
| `$2130` CGWSEL.4-5 | | `color.region` | `"everywhere"`, `"inside"`, `"outside"`, `"never"` |
| `$2131` CGADSUB.0-5 | `CGADSUB` | `color.on.bg1..bg4/.obj/.backdrop` | bool each |
| `$2131` CGADSUB.6 | | `color.half` | bool |
| `$2131` CGADSUB.7 | | `color.op` | `"add"` or `"sub"` |
| `$2132` COLDATA | `COLDATA` 15-bit, or `coldata(byte)` | `color.fixed = rgb(r, g, b)` | `coldata()` is the authentic per-channel write |

## Memory

| Hardware | Lua | Notes |
|---|---|---|
| VRAM `$2116`–`$2119` | `vram[addr] = word` | word address 0..0x7FFF; final authority over everything below |
| VRAM, tiles + maps + palettes | `dma("name", { char =, map =, pal = })` | init-only placement of an imported source; see [dma.md](dma.md) |
| CGRAM `$2121`/`$2122` | `cgram[i] = rgb(r, g, b)` | i = 0..255, 15-bit BGR; `hsl(h, s, l)` too; inside an `hdma()` hook the write is that line only |
| OAM `$2102`–`$2104` | `obj[i].*` | see Sprites |

VRAM is rebuilt every frame in a fixed order: zero, `dma()` placements, then
`bg[n].map`, `m7.map`, `m7pixel`, then raw `vram[]` last.

## Per-scanline state

`hdma(y0, y1, function(y) … end)` (alias `scanline`) runs the callback for
scanlines `y0..y1` with every register above re-baselined to that line. Assign
inside it and only that line changes, which is what hardware HDMA does.

## Not registers, but read every frame

| Lua | What |
|---|---|
| `t`, `f` | seconds (float) and frame index, as `frame(t, f)` arguments |
| `pad.up/.down/.left/.right/.a/.b/.x/.y/.l/.r/.start/.select` | controller, held booleans; see [pad.md](pad.md) |
| `rgb(r, g, b)`, `hsl(h, s, l)` | 0..255 / degrees+unit to 15-bit BGR |
| `sin cos tan floor ceil abs min max sqrt pi` | `math.*` as flat globals |

## Not modelled

`$2115` VMAIN (VRAM increment mode; `vram[]` is addressed directly), `$2134`–
`$213F` read registers, modes 5 and 6, interlace and overscan bits of SETINI,
and the H/V counter latch. Toys that would need them are not toys ppu.toys can
render yet.
