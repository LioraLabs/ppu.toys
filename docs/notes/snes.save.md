---
id: snes.save
kind: diagnostic
title: saveData is one record of bounded fields, declared once at file level and assigned by field name
codes: [snes.save]
---
A cartridge lays the [[saveData]] record out in battery RAM when it builds, so it must see the whole
record in one place. `error[snes.save]` refuses a second record, a `saveData` call not bound to a
file-level name, a table that is not written inline, a field the record does not declare, and a
`true`/`false` field assigned anything else.

Declare the record once with each field a `makeBounded(default, min, max)` with literal bounds, or a
boolean. Read fields as `save.best` and assign them one per statement; each assignment writes
through to the battery. A first power-on, a changed record or a damaged save starts from the
defaults.

```luau
local save = saveData({best = makeBounded(0, 0, 9999), music = true})
local score = makeBounded(0, 0, 9999)

function frame(t, f)
  if pad.a then score = math.min(score + 1, 9999) end
  if score > save.best then save.best = score end
end
```
