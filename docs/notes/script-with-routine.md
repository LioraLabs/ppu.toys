---
id: script-with-routine
kind: recipe
title: A timed sequence reads top to bottom as a routine that yields, stepped once a frame
codes: []
---
"Announce the wave, wait a second and a half, send eight enemies 14 frames apart, wait until they are
gone" is a state machine when written per frame. As a [[routine]] it is the code itself:
[[yield|yield(n)]] pauses for n steps and keeps the loop's place and the locals. The scene calls
`script:step()` once a frame; a new game calls `script:cancel()`, puts its state back, and starts it again.

Keep what the rest of the game reads (the wave number, a banner) in [[bound-every-lasting-value|bounded]]
module fields the body writes, not in the body's locals.

```luau
local state = {wave = makeBounded(0, 0, 99), banner = makeBounded(0, 0, 2)}
local alive = makeBounded(0, 0, 8)

local script = routine(function()
  for wave = 1, 99 do
    state.wave, state.banner = wave, 1
    yield(90)
    state.banner = 0
    for _ = 1, math.min(8, 3 + wave) do
      alive = math.min(8, alive + 1)
      yield(14)
    end
    while alive > 0 do
      yield(10)
    end
    state.banner = 2
    yield(60)
  end
end)

local function start()
  script:cancel()
  state.wave, state.banner, alive = 0, 0, 0
  script:start()
end

function init()
  start()
end

function frame(t, f)
  if pad.pressed.a then alive = math.max(0, alive - 1) end
  script:step()
end
```
