---
id: snes.timer
kind: diagnostic
title: timer is setup-only and takes timer 0 to 2, a divider of 1 to 255 and a function
codes: [snes.timer]
---
`error[snes.timer]: ...` names what [[timer]] refused:

- `timer runs during setup`: it was called from `frame()` or a hook. Register every timer at the
  top level or in `init()`, like [[dma]].
- `n must be an integer in 0..2`: the console has three sound-CPU timers; 0 and 1 tick at 8 kHz, 2 at 64 kHz.
- `div must be an integer in 1..255`: the divider is how many ticks make one fire, so 8000 / div fires
  a second on timer 0. A slower beat counts fires in the hook.
- `third argument must be a function`: the hook, called as `fn(off, t)`.

A hook writes only globals; `frame()` reads them and pokes the registers.

A cartridge has no `timer`: it refuses the call with `snes.unsupported`. The fence shows the beat a cartridge can keep, counted in `frame()`; a timer hook is the preview's.

```luau
local beat = makeBounded(0, 0, 15)

function frame(t, f)
  if f % 8 == 0 then beat = (beat + 1) % 16 end
  brightness = beat
end
```
