---
id: snes.halt
kind: diagnostic
title: A cartridge stops on the first value outside its proven range, where the preview runs on
codes: [snes.halt]
---
`error[snes.halt]: the exported checked program halts at update N` means the cartridge build of the
toy, run for the same frames and input as the preview, stored a value outside the range the compiler
sized it for (the message names the value, the range and the source line). The preview keeps
counting; a cartridge stops there. The fix is the same as for a broken [[makeBounded]] bound: clamp
or wrap the value itself so every value the code can produce stays inside the range.

```luau
local count = makeBounded(0, 0, 3)

function frame(t, f)
  if pad.a then
    count = math.min(count + 1, 3)
  end
  brightness = 12 + count
end
```
