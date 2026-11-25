---
id: snes.bank
kind: diagnostic
title: bank() takes a built-in sample's name or an uploaded sample's, and instrument{} needs a sample
codes: [snes.bank]
---
`error[snes.bank]: bank: no built-in sample 'x' and none uploaded` means the name is neither one of
the built-in samples (`piano`, `bass`, `lead`, `strings`, `organ`, `bell`, `flute`, `pluck`, `kick`,
`snare`, `hat`, `ohat`, `tom`, `clap`, `crash`) nor a sample source in the project's ppu.json.
`instrument: sample is required` means an `instrument{}` table has no `sample`.

[[bank]] places the sample in sound RAM, so it is setup work: call it at file level and keep the
result in a named local, then play it with [[sfx]]. Options override the preset's fields; a looping
sample stops after 0.15 s unless `len` says otherwise.

```luau
local zap = bank("pluck", {vol = 36, len = 0.3})

function frame(t, f)
  if pad.a then
    sfx(zap, 5, "C6")
  end
end
```
