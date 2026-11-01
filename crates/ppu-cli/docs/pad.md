# Controller input

`pad` is the first SNES controller. Each field is `true` while its button is
held and refreshes before every `frame()`.

## Move something

```luau
local hero = {x = makeBounded(128, 0, 240), y = makeBounded(112, 0, 208)}

function frame(t, f)
  local dx = (if pad.right then 1 else 0) - (if pad.left then 1 else 0)
  local dy = (if pad.down then 1 else 0) - (if pad.up then 1 else 0)
  hero.x = math.clamp(hero.x + dx, 0, 240)
  hero.y = math.clamp(hero.y + dy, 0, 208)

  obj[0].x = hero.x
  obj[0].y = hero.y
end
```

[`makeBounded`](luau.md#numbers-with-a-range-makebounded-and-makefixed) states
the range each coordinate must stay in; `math.clamp` keeps it there.

Available fields are `up`, `down`, `left`, `right`, `a`, `b`, `x`, `y`, `l`,
`r`, `start`, and `select`.

## Detect one press

Because `pad.a` stays true while held, remember the previous frame when an
action should happen only once.

```luau
local was_a = false

function frame(t, f)
  local pressed_a = pad.a and not was_a
  was_a = pad.a

  if pressed_a then
    brightness = 15 - brightness
  end
end
```

## Two players

`pad2` is the second controller, with the same fields as `pad`.

```luau
function frame(t, f)
  if pad2.left then obj[1].x = obj[1].x - 1 end
end
```

## Controls

| SNES   | Keyboard   | Gamepad                    |
| ------ | ---------- | -------------------------- |
| D-pad  | Arrow keys | D-pad or left stick        |
| B / A  | Z / X      | Bottom / right face button |
| Y / X  | A / S      | Left / top face button     |
| L / R  | Q / W      | Left / right shoulder      |
| Start  | Enter      | Start                      |
| Select | Shift      | Select                     |

Controller 2 on the keyboard: IJKL for the D-pad, N/M for B/A, H/Y for Y/X,
U/O for L/R, and P for Start. The second connected gamepad is controller 2.

Click the output once to give keyboard controls focus. Standard-mapping
gamepads work without focus. Recorded clips and thumbnails use released input,
so toys that wait for a button show their idle state.

See also: [Sprites](sprites.md) and [the PPU pipeline](registers.md).
