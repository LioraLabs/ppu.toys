---
id: actors-that-come-and-go-are-a-fixed-list
kind: recipe
title: Actors that come and go (shots, enemies, blasts) are a fixed list of records with a live flag, sized to what the screen holds
codes: []
fixture: twinstar
---
Make the records once, at file level: a loop fills a list of as many as the screen holds at once, each from
a factory whose fields are made with [[makeBounded]] or [[makeFixed]], plus `live = false`. A new actor takes
the first record that is not live (none free: a seventh shot simply waits), and leaving play sets `live` back
to false. Every loop over the list skips the records that are not live. A reused record keeps its old fields:
set every field when it comes back. Draw them into a group made with [[sprite-group-slots-for-nested-art|slots]]
for the list's length: a loop over a list of records does not size a group by itself. Actors placed on a map
are read from it once instead ([[map-placed-actors-are-record-lists|map actors]]).

```luau
local _ships = dma("ships", {palette = "ships_pal", char = 0x4000, pal = 0})
local bits = dma("bits", {palette = "bits_pal", char = 0x4800, pal = 6})
local SHOT = sprites.art({{size = 8, columns = 1, cells = {0}}}, bits)
local group = sprites.group({slots = 6})

local function make_shot()
  return {live = false, x = makeBounded(0, 0, 255), y = makeBounded(0, 0, 223)}
end
local shots = {}
for i = 1, 6 do shots[i] = make_shot() end
local cooldown = makeBounded(0, 0, 12)

function init()
  mode = 1
  screen.main.obj = true
  obj.char_base = _ships.char
end

function frame(t, f)
  cooldown = math.max(0, cooldown - 1)
  if pad.a and cooldown == 0 then
    for _, s in shots do
      if not s.live then
        s.live, s.x, s.y, cooldown = true, 128, 200, 12
        break
      end
    end
  end
  for _, s in shots do
    if s.live then
      if s.y < 22 then s.live = false else s.y -= 6 end
    end
  end
  for _, s in shots do
    if s.live then group:draw(SHOT, s.x, s.y) end
  end
end
```
