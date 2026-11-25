---
id: a-river-from-a-lookup-along-one-axis
kind: recipe
title: A long shape such as a river or a road is a lookup table along one axis, and the pick lays the template whose edges meet its neighbours'
codes: []
---
A river or a coast cannot come from per-chunk random picks, because its pieces must join. Give the shape a table that says where it crosses each row of chunks (a [[turn-tables-with-lookup|lookup]], one column either way at most from row to row), and
let `pick` return a straight piece where the next row's column is the same and a bend where it differs. Draw the pieces so the shape leaves each template at the middle of an edge, and index the table modulo its length so it joins itself at the seam
([[generated-ground-wraps-at-its-period]]). Other chunks take a meadow chosen by the seed, as in [[generated-ground-picks-a-template-per-chunk]].

```luau
local seed = makeBounded(0, 0, 9999)
local picked = makeBounded(0, 0, 3)

-- Where the river crosses each row of chunks: a chunk column, one either way at most from row to row.
local river = lookup({row = {0, 255}}, function(row)
  return 16 + math.floor(5 * math.sin(row * math.pi / 32) + 0.5)
end)

-- 0 the river, 1 where it bends, else one of two meadows by the seed.
local function pick(cx: number, cy: number): number
  local here, below = river(cy), river((cy + 1) % 256)
  if cx == here then return if below == here then 0 else 1 end
  if cx == below then return 1 end
  return 2 + math.hash(seed, cx, cy) % 2
end

function frame(t, f)
  picked = pick(f % 32, f // 32 % 64)
end
```
