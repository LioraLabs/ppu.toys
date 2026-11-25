---
id: mode-7-perspective-per-heading
kind: recipe
title: A behind-the-car view bakes one set of per-line matrices per heading, turning the plane about the car
codes: []
---
A line `n` lines under the vanishing line shows ground `HEIGHT * FOCAL / n` pixels ahead, `HEIGHT / n`
plane pixels to a screen pixel. Give `a` and `c` that scale turned by the heading, and `b` and `d` the
offset that puts the line its distance ahead of the car, measured from the car's own line `CAR_Y`, so
that line gets no offset and the plane turns about the car. Then only the heading needs baking: 128
views, one per two headings. Each frame sets the centre on the car once and calls the bake. Keep every
entry in range ([[mode-7-matrix-is-signed-8-8]]), and keep the farthest ground within the loaded cells
when the map streams ([[mode-7-track-streams-within-radius]]). The sky above is
[[mode-7-floor-under-a-sky]].

```luau
local HORIZON, CAR_Y, HEIGHT, FOCAL, FAR = 80, 196, 48, 192, 21

local ground = bakeHdma({view = {0, 127}}, 0, 223, function(y, view)
  if y < HORIZON then
    mode = 1
    win.w1.lo, win.w1.hi = 0, 255
  else
    mode = 7
    win.w1.lo, win.w1.hi = 255, 0
    local n, n_car = y - HORIZON + FAR, CAR_Y - HORIZON + FAR
    local scale, ahead = HEIGHT / n, FOCAL * HEIGHT / (n * n_car)
    local angle = view * math.pi / 64
    local s, c = math.sin(angle), math.cos(angle)
    m7.a, m7.b, m7.c, m7.d = c * scale, -s * ahead, s * scale, c * ahead
  end
end)

local heading = makeBounded(0, 0, 255)

function init()
  mode = 7
  screen.main.bg1 = true
  win.bg1.w1, win.bg1.main = true, true
end

function frame(t, f)
  if pad.right then heading = (heading + 1) % 256 end
  if pad.left then heading = (heading + 255) % 256 end
  m7.cx, m7.cy = 512, 512
  bg[1].scroll.x, bg[1].scroll.y = 512 - 128, 512 - CAR_Y - 1
  ground(heading // 2)
end
```
