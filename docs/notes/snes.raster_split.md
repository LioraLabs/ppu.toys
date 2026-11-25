---
id: snes.raster_split
kind: diagnostic
title: Scanline effects that change many register groups per line share HDMA channels and cost CPU time each frame
codes: [snes.raster_split]
---
A cartridge gives each register a scanline effect changes its own HDMA channel, which the hardware
updates without the CPU. It has eight. The warning `Scanline effects share N HDMA channels with M
mid-frame channel switches` means the effects change more register groups than there are channels,
so some channels are switched partway down the screen, and an interrupt does that every frame,
taking CPU time.

Change fewer register groups per line in each band of the screen: one scroll per effect, say, not
scroll, colour and window together. [[bake-scanline-effects|Baked effects]] that each touch one
group keep every effect on its own channel.

```luau
local wave = bakeHdma({phase = {0, 31}}, 0, 223, function(y, phase)
  bg[1].scroll.x = math.floor(math.sin((y + phase * 2) * math.pi / 16) * 3)
end)

function frame(t, f)
  wave(f % 32)
end
```
