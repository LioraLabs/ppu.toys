---
id: sorted-group-draws-lower-in-front
kind: recipe
title: A sort = true group orders its sprites by depth, so the actor lower on screen is drawn in front
codes: []
fixture: twinstar
---
Inside one group, sprites overlap in draw order. For actors that pass each other (enemies, a crowd), make
the group with `{sort = true}` and give each draw a `depth`: the smaller depth is in front. `depth = 255 - y`
puts the one lower on the screen in front, which reads as nearer. Between groups,
[[sprite-groups-stack-in-creation-order|creation order]] still decides. On a cartridge sorting adds about 1% of a frame to each draw in the group, more than the draw itself, so sort only the group whose actors overlap.

```luau
local sheet = dma("ships", {palette = "ships_pal", char = 0x4000, pal = 0})
local DRONE = sprites.art({{size = 16, columns = 1, cells = {6}}}, sheet)
local enemies = sprites.group({sort = true})
local ys = {100, 108}

function init()
  mode = 1
  screen.main.obj = true
  obj.char_base = sheet.char
end

function frame(t, f)
  for i = 1, 2 do
    local y = ys[i] + (if i == 1 then f // 30 % 2 * 16 else 0)
    enemies:draw(DRONE, 112 + i * 8, y, {depth = 255 - y})
  end
end
```
