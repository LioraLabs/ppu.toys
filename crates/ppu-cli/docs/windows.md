# Windows

A window is a horizontal mask. Two pairs of coordinates define window 1 and
window 2; selection registers decide which layers they affect; logic registers
combine them. Nothing is drawn—the windows only decide where another stage is
allowed to act.

## Define the regions · WH0–WH3 `$2126–$2129`

Each window covers an inclusive horizontal span from `lo` to `hi`.

```lua
win.w1.lo = 48
win.w1.hi = 208

win.w2.lo = 96
win.w2.hi = 160
```

The raw form writes the same four bytes directly:

```lua
WH0, WH1 = 48, 208
WH2, WH3 = 96, 160
```

## Attach a window · W12SEL, W34SEL, WOBJSEL

Layers opt into window 1 or window 2. Inversion selects the area outside a
window instead of inside it.

```lua
win.bg1.w1 = true
win.bg1.invert = false
```

For BG1, that is `W12SEL = 0x02`: bit 1 enables window 1 and bit 0 controls
its inversion.

The same shape works for `bg1` through `bg4`, `obj`, and `color`.

## Combine two windows · WBGLOG, WOBJLOG

When both windows are enabled for a layer, choose how their masks combine.

```lua
win.bg1.w1 = true
win.bg1.w2 = true
win.bg1.combine = "XOR"
```

BG1 occupies WBGLOG bits 0–1, so the raw equivalent is `WBGLOG = 0x02`.

The choices are `"OR"`, `"AND"`, `"XOR"`, and `"XNOR"`.

## Mask a screen · TMW `$212E`, TSW `$212F`

Enable the completed mask separately on the main or sub screen.

```lua
win.bg1.main = true
win.bg1.sub = false
```

The raw equivalent is `TMW = 0x01` and `TSW = 0x00`.

This controls whether BG1 participates in that screen at each horizontal pixel.

## The color window

The color window does not hide a layer. It limits where [color math](color-math.md)
or the main-screen result is allowed.

```lua
win.w1.lo = 64
win.w1.hi = 192
win.color.w1 = true

color.region = "inside"
```

The raw controls are `WOBJSEL = 0x20` for color-window 1 and
`CGWSEL = 0x10` to apply math inside it.

See also: [Main and sub screens](color-math.md#main-and-sub-screens) and
[scanline effects](scanlines.md)—changing window edges per line makes shapes
that are not rectangular.
