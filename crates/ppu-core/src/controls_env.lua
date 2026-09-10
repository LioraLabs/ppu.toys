-- Tracked environment for ppuglobals.lua (see LuaEngine::load_controls/frame's
-- Phase A). ppuglobals.lua is compiled with `__ppu_controls_env` as its _ENV,
-- so every global read/write it makes (including nested table fields, e.g.
-- `obj[0].x = 96`) passes through `track`'s proxy. A write is logged as
-- (real table, key, old value) before it lands on the REAL table, so
-- `__ppu_controls_restore` can undo every write made since the last
-- `__ppu_controls_begin` in reverse order — returning the program's own
-- globals to exactly where they were, so an overridden-then-released
-- property never bakes into the program's baseline or compounds an
-- accumulator (see frame()'s Phase A doc comment). Reads do NOT pass
-- through untouched: a nested-table read returns a PROXY, not the real
-- table, so `#proxy`, `pairs(proxy)`, `rawget`, and identity comparisons all
-- give wrong answers inside ppuglobals.lua. Writing a table-valued READ
-- elsewhere (e.g. `obj[0] = obj[1]`) stores that PROXY onto the real
-- globals, not the underlying table — also unsupported. The generated
-- document only ever does scalar reads/assignments and `hdma(...)` calls,
-- and those forms are the only ones supported inside ppuglobals.lua — no
-- `__len`/`__pairs` is implemented.
local G = _ENV
local log = {}
local n = 0
-- Memoizes one proxy per real table so a nested read (e.g. `bg[1]` inside
-- `bg[1].scroll.x = 24`) doesn't allocate a fresh proxy every time it's
-- read — this matters because Phase A re-runs `apply_pokes` once per
-- scanline row (224x/frame). Cleared wholesale by `__ppu_controls_begin`
-- (once per frame) rather than kept forever, which sidesteps the memo
-- holding a stale strong reference to a table the program later replaces
-- (e.g. `obj[0] = {}`) — a distinct-tables-touched-per-frame number of
-- allocations, not rows x pokes, with no leak to manage.
-- ponytail: apply_pokes still re-runs once per scanline row (224x/frame) for
-- the frame-wide pass; if profiling ever blames it, run it once against the
-- defaults row and diff the written fields instead of replaying per row.
local cache = {}

local function track(real)
  local proxy = cache[real]
  if proxy then
    return proxy
  end
  proxy = setmetatable({}, {
    __index = function(_, k)
      local v = real[k]
      if type(v) == "table" then
        return track(v)
      end
      return v
    end,
    __newindex = function(_, k, v)
      n = n + 1
      log[n] = { real, k, real[k] }
      real[k] = v
    end,
  })
  cache[real] = proxy
  return proxy
end

G.__ppu_controls_env = track(G)

function G.__ppu_controls_begin()
  log = {}
  n = 0
  cache = {}
end

function G.__ppu_controls_dirty()
  return n > 0
end

function G.__ppu_controls_restore()
  for i = n, 1, -1 do
    local e = log[i]
    e[1][e[2]] = e[3]
  end
  log = {}
  n = 0
end
