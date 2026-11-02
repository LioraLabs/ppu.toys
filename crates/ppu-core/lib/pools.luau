--!strict
-- @ppu/pools: fixed pools of records, reused without bookkeeping.
-- local Pools = require("@ppu/pools"); local shots = Pools.new(8, make_shot)
-- local s = shots:spawn() (nil when every record is live); shots:free(s); for s in shots:live() do ... end
-- A recycled record keeps its old fields. Not checked: free(nil) or a record from another pool errors
-- or marks that record free; freeing twice is harmless.

local function spawn(pool)
  for _, x in pool.items do
    if not x.__pool_live then
      x.__pool_live = true
      return x
    end
  end
  return nil
end

local function free(pool, x)
  x.__pool_live = false
end

local function live(pool)
  return coroutine.wrap(function()
    for _, x in pool.items do
      if x.__pool_live then coroutine.yield(x) end
    end
  end)
end

local function new(n: number, factory)
  local items = {}
  for _ = 1, n do
    local x = factory()
    x.__pool_live = false
    table.insert(items, x)
  end
  return {items = items, spawn = spawn, free = free, live = live}
end

return {new = new}
