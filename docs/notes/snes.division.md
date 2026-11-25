---
id: snes.division
kind: diagnostic
title: A fractional division by zero is an error, so guard the divisor before dividing
codes: [snes.division]
---
`error[snes.division]: division by zero` means a `/` on fractional values (the fixed point the
cartridge computes in) had a divisor of zero. The preview stops on it rather than producing an
infinity a cartridge cannot hold.

Make zero impossible where the divisor is made: clamp it to at least one step, or branch around the
division when it is zero. A divisor that is a constant, or a power of two written as one, also
spares a cartridge the runtime divide.

```luau
local speed = makeFixed(0, 0, 4, 16)

function frame(t, f)
  speed = if pad.a then math.min(speed + 0.0625, 4) else 0
  local frames = if speed > 0 then 8 / speed else 0
  brightness = math.min(math.floor(frames), 15)
end
```
