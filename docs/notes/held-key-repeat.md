---
id: held-key-repeat
kind: recipe
title: A held direction repeats in a menu through a countdown that resets when it is let go
codes: []
---
A menu that moves only on [[detect-a-press|pad.pressed]] makes the player tap for every step; one that
moves on the held button races. Keep a bounded countdown: step at once when it is 0 and set it to the
repeat delay, count it down while the button stays held, and reset it to 0 when nothing is held, so the
next press steps at once.

```luau
local letter = makeBounded(0, 0, 25)
local repeat_wait = makeBounded(0, 0, 20)

function frame(t, f)
  local up, down = pad.up or pad2.up, pad.down or pad2.down
  if up or down then
    if repeat_wait == 0 then
      letter = (letter + (if up then 1 else 25)) % 26
      repeat_wait = 8
    else
      repeat_wait -= 1
    end
  else
    repeat_wait = 0
  end
end
```
