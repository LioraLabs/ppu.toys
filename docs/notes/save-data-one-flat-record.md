---
id: save-data-one-flat-record
kind: recipe
title: A battery save is one flat saveData record; the game works on a plain list and writes the fields back on change
codes: []
---
[[saveData]] binds one record at file level, once per program: fields are `makeBounded(default, min,
max)` with literal bounds, or `true`/`false`, read and assigned by name, one field per assignment. A
first power-on, a changed record or a damaged save starts from the defaults. The record holds no lists,
so a high-score table is numbered fields: load them into a plain list the game can sort and shift, and
write each field back when an entry changes. A `saveData` anywhere else, a second record or a field
the cartridge cannot lay out is refused: [[snes.save]].

```luau
local save = saveData({
  score1 = makeBounded(300, 0, 99999),
  score2 = makeBounded(200, 0, 99999),
  score3 = makeBounded(100, 0, 99999),
})

local board = {makeBounded(save.score1, 0, 99999), makeBounded(save.score2, 0, 99999), makeBounded(save.score3, 0, 99999)}

local function insert(score: number)
  for rank = 1, 3 do
    if score > board[rank] then
      for i = 3, rank + 1, -1 do board[i] = board[i - 1] end
      board[rank] = score
      save.score1 = board[1]
      save.score2 = board[2]
      save.score3 = board[3]
      return
    end
  end
end

function frame(t, f)
  if pad.pressed.a then insert(f % 1000) end
end
```
