---
id: snes.mode7
kind: diagnostic
title: mode7_transform stops on a zoom below 1/127 or an argument that is not a finite number
codes: [snes.mode7]
---
`error[snes.mode7]: ...` names what [[mode7_transform]] or [[mode7_floor]] refused:

- `<name> must be a finite number`: an angle, zoom, position, scanline, horizon or height is `nil`,
  a string, `inf` or `nan`, often a division by a value that reached zero. Clamp the value before the call.
- `zoom must be at least 1/127`: the matrix would overflow the Mode 7 registers. Keep zoom at or above
  1/127 (2 is twice as large).
- `height must be positive`: a [[mode7_floor]] camera at or below the plane.

A cartridge has neither helper (it refuses both as [[snes.unsupported]]): there you set the matrix
yourself from a zoom you clamp, and a floor is a baked per-line matrix, as in [[mode-7-floor-under-a-sky]].

```luau
local zoom = makeFixed(1, 1, 4, 256)

function init()
  mode = 7
  screen.main.bg1 = true
end

function frame(t, f)
  if pad.up then zoom = math.min(zoom + 0.01, 4) end
  if pad.down then zoom = math.max(zoom - 0.01, 1) end
  m7.a, m7.d = 1 / zoom, 1 / zoom -- a zoom below 1/127 would overflow the registers: clamp it first
  m7.cx, m7.cy = 512, 512
  bg[1].scroll.x, bg[1].scroll.y = 384, 400
end
```
