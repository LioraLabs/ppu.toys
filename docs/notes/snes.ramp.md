---
id: snes.ramp
kind: diagnostic
title: ramp and ramp_color stop on an empty keyframe list
codes: [snes.ramp]
---
`error[snes.ramp]: ramp: at least one keyframe is required` (or `ramp_color: ...`): [[ramp]],
[[ramp_int]] or [[ramp_color]] was given `{}`, often a keyframe table built at run time that came
out empty. A ramp needs at least one `{position, value}` key; one key holds that value everywhere.
Write the keys as a literal at file level, so the table is built once and is never empty
([[ramp-a-value-down-the-screen]]).

```luau
local SKY = {{0, 0x3082}, {223, 0x221e, "smooth"}}

function init()
  mode = 1
  screen.main.bg1 = true
end

function frame(t, f)
  hdma(0, 223, function(y)
    cgram[0] = ramp_color(SKY, y)
  end)
end
```
