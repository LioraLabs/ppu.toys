---
id: keyboard-and-gamepad-controls
kind: concept
title: The Studio maps the SNES pad onto keys and gamepads, so a game names buttons, never keys
codes: []
---
A game reads `pad` (controller 1) and `pad2` (controller 2) by SNES button name: `up`, `down`, `left`,
`right`, `a`, `b`, `x`, `y`, `l`, `r`, `start`, `select`. The Studio and the player map keys and
gamepads onto them: arrows for the d-pad, Z and X for B and A, A and S for Y and X, Q and W for L and
R, Enter for Start, Shift for Select. Controller 2 is I J K L for the d-pad, N and M for B and A, H and
Y for Y and X, U and O for L and R, and P for Start. A standard-mapping gamepad (d-pad or left stick,
face buttons by position) works without a click, and the second connected one is controller 2. The
output must be clicked once to give keyboard controls focus.

```luau
function frame(t, f)
  local dx = (if pad.right then 1 else 0) - (if pad.left then 1 else 0)
  local jump = pad.pressed.b
  local dash = pad.x or pad2.x
  brightness = if dash then 8 else 15
end
```
