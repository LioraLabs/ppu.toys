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
-- table (`#` and `pairs` are forwarded; `rawget` and identity comparisons
-- still give wrong answers inside ppuglobals.lua). Writing a table-valued READ
-- elsewhere (e.g. `obj[0] = obj[1]`) stores that PROXY onto the real
-- globals, not the underlying table — also unsupported. The generated
-- document only ever does scalar reads/assignments and `hdma(...)` calls,
-- plus the setup calls apply_setup makes, and those forms are the only
-- ones supported inside ppuglobals.lua.
local G = _ENV
local log = {}
local n = 0
-- Memoizes one proxy per real table so a nested read (e.g. `bg[1]` inside
-- `bg[1].scroll.x = 24`) doesn't allocate a fresh proxy every time it's
-- read. Cleared wholesale by `__ppu_controls_begin` (once per frame) rather
-- than kept forever, which sidesteps the memo holding a stale strong
-- reference to a table the program later replaces (e.g. `obj[0] = {}`).
-- Each log entry also carries the ROOT global its write landed under
-- (`bg` for `bg[1].scroll.x`, `TM` for `TM`), so `__ppu_controls_freeze`
-- can tell a register write from a memory-table one.
local cache = {}

-- ppuglobals.lua's own `vr`: the built-in writes the REAL `vram`, which would
-- skip the undo log and leave a released tile poke baked in. This one writes
-- through the tracked proxy, one logged write per word like the raw
-- `vram[addr] = word` lines it replaces. Assigned below, once `track` exists.
local tracked_vr

local function track(real, root)
  local proxy = cache[real]
  if proxy then
    return proxy
  end
  proxy = setmetatable({}, {
    __index = function(_, k)
      if real == G and k == "vr" then
        return tracked_vr
      end
      local v = real[k]
      if type(v) == "table" then
        return track(v, root or k)
      end
      return v
    end,
    __newindex = function(_, k, v)
      n = n + 1
      log[n] = { real, k, real[k], root or k }
      real[k] = v
    end,
    -- `#` and pairs() see the real table (values still come back proxied),
    -- so a setup call made from ppuglobals.lua — `midi{ data = tune }` in
    -- apply_setup, walking `#tune.tracks` — reads the data chunk it names.
    __len = function()
      return #real
    end,
    __pairs = function()
      return function(_, k)
        local nk, v = next(real, k)
        if type(v) == "table" then
          v = track(v, root or nk)
        end
        return nk, v
      end, proxy, nil
    end,
  })
  cache[real] = proxy
  return proxy
end

G.__ppu_controls_env = track(G)

-- While `apply_vram()` is being captured (once per load, see below), `vr`
-- records its words instead of writing them. Outside that window it is a
-- tracked write like any other line in this file.
local capture

tracked_vr = function(addr, words)
  if capture then
    for i = 1, #words do
      capture[addr + i - 1] = words[i]
    end
    return
  end
  local v = track(G).vram
  for i = 1, #words do
    v[addr + i - 1] = words[i]
  end
end

-- The painted tiles are constant literals, so they never need to run as Lua
-- at frame time: a 32x32 map through the tracked proxy cost ~20 ms EVERY
-- frame. Instead `apply_vram()` runs ONCE per load with `vr` capturing, and
-- the engine overlays the captured words onto VRAM in Rust each frame, after
-- everything else (pokes always win). Leaves `__ppu_controls_vram` as a
-- sparse {addr = word} table. Raises whatever apply_vram raises.
function G.__ppu_controls_capture_vram()
  G.__ppu_controls_vram = {}
  local v = G.apply_vram
  if type(v) ~= "function" then
    return
  end
  capture = {}
  local ok, err = pcall(v)
  local got = capture
  capture = nil
  if not ok then
    error(err, 0)
  end
  G.__ppu_controls_vram = got
end

-- Frame-only roots (mirrors FRAME_ONLY_ROOTS in scanlinePokes.ts, plus
-- cgram, whose frame-wide pokes are baked into the row baseline before any
-- hook runs): the per-row replay below never touches them, since the line
-- table reads none of them and a tilemap paint is thousands of `vram[]`
-- writes that would otherwise re-run 224x per frame.
local FRAME_ONLY = {
  obj = true, vram = true, aram = true, voice = true, dsp = true,
  sram = true, pad = true, cgram = true,
}
local frozen = {}

function G.__ppu_controls_freeze()
  frozen = {}
  local m = 0
  for i = 1, n do
    local e = log[i]
    if not FRAME_ONLY[e[4]] then
      m = m + 1
      frozen[m] = { e[1], e[2], e[1][e[2]] }
    end
  end
  return m > 0
end

-- Unlogged on purpose: the end-of-frame `write_state(defaults)` rebaselines
-- every register, and `__ppu_controls_restore` still undoes the Phase A
-- write (the earliest logged old value) for anything it doesn't cover.
function G.__ppu_controls_replay()
  for i = 1, #frozen do
    local e = frozen[i]
    e[1][e[2]] = e[3]
  end
end

function G.__ppu_controls_begin()
  log = {}
  n = 0
  cache = {}
end

function G.__ppu_controls_restore()
  for i = n, 1, -1 do
    local e = log[i]
    e[1][e[2]] = e[3]
  end
  log = {}
  n = 0
end
