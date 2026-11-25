---
id: snes.table
kind: diagnostic
title: A cartridge table has one number type and a shape it can see where the table is written
codes: [snes.table]
---
`error[snes.table]: ...` means a table literal has no layout on a cartridge, which stores every
table as fixed slots of one type:

- `Array elements must have the same scalar type.`: a list mixes whole numbers and fractions, such
  as `{0, 0.5, 0.25}`. Write every entry as a fraction, `{0.0, 0.5, 0.25}`, or every entry as a whole
  number in finer units, `{0, 2, 1}` quarters.
- `This table has no storage plan (computed keys, or values that never reach a variable).`: the
  table is built with keys computed at run time (`{[k] = 1}`), or is assigned somewhere that is not
  a variable, so the cartridge never learns its shape. Give it literal keys, a list or named fields,
  in a variable, and write entries by index afterwards. A `dsp.echo = {...}` or `dsp.evol = {...}`
  line is the echo's own record, not a table ([[echo-is-the-snes-texture]]); one a cartridge cannot
  take refuses with [[snes.sound]].

```luau
local DITHER = {0.0, 0.5, 0.25, 0.75} -- not {0, 0.5, 0.25, 0.75}

function frame(t, f)
  vram[0] = floor(DITHER[f % 4 + 1] * 4)
end
```
