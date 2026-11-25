---
id: shadow-with-subtractive-fixed-colour
kind: recipe
title: A shadow or dusk subtracts a fixed colour from the layers colour math is on for
codes: []
---
With `color.addend = "fixed"` and `color.op = "sub"`, each layer listed in `color.on` has
[[color.fixed]] taken away from its pixels, channel by channel: a grey darkens, a blue-heavy colour
removes blue and leaves warm light. Turn the effect on for the layers and the backdrop that should
dim, and drive the strength with one bounded number; a fade to dusk is that number counting up.
`rgb` takes 0 to 255 and keeps 5 bits a channel, so step by 8 for a visible change. Each colour channel stops at black when subtracting and at white when adding, so a strong shadow over dark ground cannot go darker than black.

```luau
local dusk = makeBounded(0, 0, 31)

function init()
  mode = 1
  screen.main.bg1 = true
  color.on.bg1, color.on.backdrop = true, true
  color.addend, color.op = "fixed", "sub"
end

function frame(t, f)
  if f % 4 == 0 and dusk < 31 then dusk += 1 end
  color.fixed = rgb(dusk * 8, dusk * 8, dusk * 4)
end
```
