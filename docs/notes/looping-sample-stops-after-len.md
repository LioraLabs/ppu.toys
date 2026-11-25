---
id: looping-sample-stops-after-len
kind: gotcha
title: A looping sample keyed by sfx stops after len, 0.15 s by default, while a one-shot drum ends with its sample
codes: []
---
Some samples loop on the chip (`pluck`, `bell`): [[sfx]] keys them off after `len` seconds, 0.15 unless
the [[bank]] preset or the call says otherwise. A bell that should ring needs a longer `len`, on the
preset (`bank("bell", {len = 0.4})`) or per call (`sfx(bell, 5, "C5", {len = 0.4})`); `len = false`
holds the note until the voice plays again (a cartridge has no `koff()`: [[cart-has-no-key-on-or-off]]). One-shot drum samples (`crash`, `tom`) end when the sample does, and
`len` changes nothing. An envelope only shapes the sound; it does not end a looping one.

```luau
local bell = bank("bell", {vol = 80, len = 0.4})
local hum = bank("pluck", {vol = 40, len = false})

function init()
  dsp.mvol.l, dsp.mvol.r = 100, 100
end

function frame(t, f)
  if pad.pressed.a then sfx(bell, 5, "C5") end
  if pad.pressed.b then sfx(hum, 6, "C4") end
end
```
