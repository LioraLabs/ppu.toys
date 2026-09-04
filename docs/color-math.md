# Screens and color math

The PPU builds two pictures: the **main screen** and the **sub screen**. The
main screen becomes the visible image. Color math can combine its winning pixel
with either the sub-screen pixel or a fixed color.

This is how the SNES makes translucency, lighting, fades, and many water and
fog effects—without alpha blending.

## Main and sub screens · TM `$212C`, TS `$212D`

Each screen independently chooses which backgrounds and sprites participate.

```lua
screen.main.bg1 = true
screen.main.obj = true

screen.sub.bg2 = true
```

The same setup as raw register bytes is:

```lua
TM = 0x11 -- BG1 + OBJ on main
TS = 0x02 -- BG2 on sub
```

The highest-priority eligible pixel wins on each screen. Putting BG2 on the sub
screen does not display it by itself; it makes BG2 available as a color-math
operand.

## Choose participating layers · CGADSUB `$2131`

Color math only runs when the main-screen winner belongs to an enabled layer.

```lua
color.on.bg1 = true
color.on.obj = true
color.on.backdrop = true
```

The raw equivalent is `CGADSUB = 0x31`: bits 0, 4, and 5 enable BG1, OBJ,
and the backdrop.

Backgrounds use `bg1` through `bg4`. Sprites share `obj`; the empty-screen color
is `backdrop`.

## Fixed-color math · CGWSEL `$2130`, COLDATA `$2132`

Choose `"fixed"` to combine the main pixel with one constant RGB color.

```lua
screen.main.bg1 = true
color.on.bg1 = true
color.addend = "fixed"
color.fixed = rgb(0, 0, 96)
color.op = "add"
```

The engine's raw mirror for this setup is `CGWSEL = 0x00`,
`CGADSUB = 0x01`, and `COLDATA = 0x3000`.

This adds blue to BG1. Change the operation to subtraction for shadows.

```lua
color.fixed = rgb(64, 64, 64)
color.op = "sub"
```

## Sub-screen math · CGWSEL `$2130`

Choose `"sub"` to combine the main winner with the sub-screen winner at the
same pixel.

```lua
screen.main.bg1 = true
screen.sub.bg2 = true

color.on.bg1 = true
color.addend = "sub"
color.op = "add"
color.half = true
```

The raw control bytes are `CGWSEL = 0x02` and `CGADSUB = 0x41`.

Half addition approximates a 50/50 blend. `color.half` and `color.op` are bits
of **CGADSUB**.

## Limit the effect · CGWSEL `$2130`

A [color window](windows.md#the-color-window) can restrict math to the inside
or outside of a horizontal region.

```lua
color.region = "inside"
```

The choices are `"everywhere"`, `"inside"`, `"outside"`, and `"never"`.
Their raw CGWSEL region bits are `0x00`, `0x10`, `0x20`, and `0x30`.

`COLDATA` in ppu.toys is the combined 15-bit fixed-color mirror. For authentic
per-channel `$2132` writes, use `coldata(byte)`.

See also: [Windows](windows.md), [display brightness](display.md#brightness-inidisp-2100),
and [per-scanline color changes](scanlines.md).
