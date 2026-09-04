# Controller input

`pad` is the first SNES controller. Each field is `true` while its button is
held and refreshes before every `frame()`.

## Move something

```lua
local x, y = 128, 112

function frame(t, f)
  if pad.left then x = x - 1 end
  if pad.right then x = x + 1 end
  if pad.up then y = y - 1 end
  if pad.down then y = y + 1 end

  obj[0].x = x
  obj[0].y = y
end
```

Available fields are `up`, `down`, `left`, `right`, `a`, `b`, `x`, `y`, `l`,
`r`, `start`, and `select`.

## Detect one press

Because `pad.a` stays true while held, remember the previous frame when an
action should happen only once.

```lua
local was_a = false

function frame(t, f)
  local pressed_a = pad.a and not was_a
  was_a = pad.a

  if pressed_a then
    brightness = 15 - brightness
  end
end
```

## Controls

| SNES | Keyboard | Gamepad |
|---|---|---|
| D-pad | Arrow keys | D-pad or left stick |
| B / A | Z / X | Bottom / right face button |
| Y / X | A / S | Left / top face button |
| L / R | Q / W | Left / right shoulder |
| Start | Enter | Start |
| Select | Shift | Select |

Click the output once to give keyboard controls focus. Standard-mapping
gamepads work without focus. Recorded clips and thumbnails use released input,
so toys that wait for a button show their idle state.

See also: [Sprites](sprites.md) and [the PPU pipeline](registers.md).
