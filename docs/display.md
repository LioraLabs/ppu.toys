# Display

The display stage takes the PPU's finished pixel and decides whether it reaches
the screen. **INIDISP** controls brightness and forced blanking; **BGMODE**
chooses how the background layers interpret VRAM.

## Brightness · INIDISP `$2100`

`brightness` runs from 0 for black to 15 for full brightness. It affects the
finished image, so backgrounds, sprites, and color math all fade together.
It is the low nibble of INIDISP; `force_blank` is bit 7.

```lua
function frame(t, f)
  brightness = floor((sin(t) + 1) * 7.5)
end
```

`force_blank` immediately blacks out the display. It exists mainly to match the
hardware; normal toys can fade with `brightness`.

```lua
force_blank = true
```

## Background mode · BGMODE `$2105`

`mode` selects the number of background layers and how many colors each tile
can use. Mode 1 is the everyday choice: two 4bpp layers and one 2bpp layer.
It occupies BGMODE bits 0–2; each `tile_size` selects one of bits 4–7.

```lua
mode = 1
screen.main.bg1 = true
```

Each background can independently use 8×8 or 16×16 tiles.

```lua
bg[1].tile_size = 16
```

## Mosaic · MOSAIC `$2106`

Mosaic holds one sampled pixel across a square block. `mosaic` sets the block
size from 0 through 15 in MOSAIC bits 4–7; bits 0–3 enable BG1 through BG4.

```lua
mosaic = 7
bg[1].mosaic = true
```

See also: [Backgrounds](backgrounds.md), [Mode 7](mode7.md), and
[scanline effects](scanlines.md).
