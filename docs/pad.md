# Controller input: `pad`

`pad` is a global table of booleans, one per SNES controller button, refreshed
before every `frame()`. It holds the *held* state: a button reads `true` for as
long as it is down. Edge detection (pressed this frame) is ordinary Lua.

```lua
local x, y = 128, 112
local was_a = false

function frame(t, f)
  if pad.left then x = x - 1 end
  if pad.right then x = x + 1 end
  if pad.up then y = y - 1 end
  if pad.down then y = y + 1 end

  local pressed_a = pad.a and not was_a
  was_a = pad.a
  if pressed_a then brightness = 15 - brightness end

  obj[0].x, obj[0].y = x, y
end
```

Fields: `up`, `down`, `left`, `right`, `a`, `b`, `x`, `y`, `l`, `r`, `start`,
`select`. `init()` can read `pad` too; every button is released there.

## Where input comes from

Click the output to focus it, then use the keyboard. Any standard-mapping
gamepad works alongside, no focus needed.

| SNES | Keyboard | Gamepad (standard mapping) |
|------|----------|----------------------------|
| d-pad | arrow keys | d-pad or left stick |
| B / A | Z / X | bottom / right face button |
| Y / X | A / S | left / top face button |
| L / R | Q / W | left / right shoulder |
| Start | Enter | start |
| Select | Shift | select |

The Studio output, the permalink player, and the landing page TV all take
input. Clips and thumbnails are recorded with every button released, so a toy
that waits for input shows its idle state on the wall.
