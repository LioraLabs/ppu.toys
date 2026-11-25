---
id: sprite-group-slots-for-nested-art
kind: gotcha
title: A group's sprite count is inferred from its draws; name slots when the draws cannot tell it
codes: []
fixture: twinstar
---
A cartridge gives each [[sprites.group|group]] a fixed number of hardware sprites, worked out from the
draws into it: one draw of 16x16 art is one slot. Some draws do not say: art picked out of a table of tables
(`SHIP[player][drawing]`), or a loop over a list of records ([[actors-that-come-and-go-are-a-fixed-list|actors
that come and go]]). Then make the group with `slots =`, the most sprites it shows at once, and
keep the draws within it. Unkeyed draws past a group's slots are dropped without an error, the last drawn first, so draw what must not vanish before what may.

```luau
local sheet = dma("ships", {palette = "ships_pal", char = 0x4000, pal = 0})
local SHIP = {
  {sprites.art({{size = 16, columns = 1, cells = {0}}}, sheet), sprites.art({{size = 16, columns = 1, cells = {1}}}, sheet)},
  {sprites.art({{size = 16, columns = 1, cells = {3}}}, sheet), sprites.art({{size = 16, columns = 1, cells = {4}}}, sheet)},
}
-- One ship per player; SHIP is a table of tables, too deep to size.
local ships = sprites.group({slots = 2})

function init()
  mode = 1
  screen.main.obj = true
  obj.char_base = sheet.char
end

function frame(t, f)
  local drawing = f // 30 % 2 + 1
  ships:draw(SHIP[1][drawing], 104, 176)
  ships:draw(SHIP[2][drawing], 152, 176)
end
```
