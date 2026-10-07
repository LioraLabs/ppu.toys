-- Searchlights · the light is a mask.
-- Each beam is a window whose edges are recomputed on every scanline (WH0-WH3),
-- so the cone is geometry, not art. Fog (sub screen), rain and the airship are
-- windowed to the union of the two beams; colour math adds the fog to whatever
-- the light crosses.
local LOOP = 12
local A = { x = 72, y = 145 }   -- lamp lenses in city.png
local B = { x = 190, y = 133 }
local SPREAD, R0 = 0.09, 1.5   -- beam half-angle (as a slope) and lens radius

local rain = dma("rain", { char = 0x0000, map = 0x7800, pal = 4 })
local city = dma("city", { char = 0x1000, map = 0x7000, pal = 32 })
local fog = dma("fog", { char = city.next_char, map = 0x7400, pal = 48 })
local ship = dma("blimp", { char = 0x6000, pal = 0 })

-- CGRAM slots: the importer stores a palette's colours in ascending BGR555
-- order after the transparent entry, so a colour's rank is its slot.
local function ranked(cols, base)
  local s = {}
  for i, c in ipairs(cols) do s[i] = c end
  for i = 2, #s do                 -- no table library in toys: insertion sort
    local v, j = s[i], i - 1
    while j >= 1 and s[j] > v do s[j + 1] = s[j]; j = j - 1 end
    s[j + 1] = v
  end
  local at = {}
  for i, c in ipairs(s) do at[c] = base + i end
  return at, s
end
local TW = { rgb(248,200,96), rgb(240,168,72), rgb(224,216,160), rgb(160,208,232), rgb(248,224,128), rgb(232,144,64) }
local CITY = ranked({ rgb(24,20,48), rgb(64,48,72), rgb(8,8,24), rgb(32,32,56), rgb(56,48,80), rgb(248,248,240), rgb(232,32,24),
  TW[1], TW[2], TW[3], TW[4], TW[5], TW[6] }, city.pal)
local TWINKLE = {}
for k, c in ipairs(TW) do TWINKLE[k] = { i = CITY[c], c = c } end
local BEACON, NIGHT = CITY[rgb(232,32,24)], rgb(8,8,24)
local _, FOGBASE = ranked({ rgb(48,56,72), rgb(64,72,96), rgb(88,96,120), rgb(112,120,144), rgb(144,152,176), rgb(176,184,208), rgb(208,216,232) }, fog.pal)

local function smooth(e0, e1, x) local v = clamp((x - e0) / (e1 - e0), 0, 1) return v * v * (3 - 2 * v) end
local function hash(n) local x = sin(n * 12.9898) * 43758.5453 return x - floor(x) end

local function ship_pos(t) return 268 - 396 * t / LOOP, 46 + 3 * sin(t * 2 * pi / 6) end

-- beam direction as a slope (dx per scanline climbed): a slow search sweep,
-- blended onto the airship while both lamps have it.
local function aim(L, t, phase, amp)
  local w = 2 * pi / LOOP
  local sweep = amp * sin(w * t + phase) + 0.12 * sin(3 * w * t + phase * 2)
  local sx, sy = ship_pos(t)
  local lock = smooth(3.4, 5.2, t) * (1 - smooth(8.4, 10.2, t))
  local track = (sx + 60 - L.x) / (L.y - (sy + 26))
  return sweep + (track - sweep) * lock
end

local function edges(L, s, y)
  local d = L.y - y
  if d <= 0 then return 1, 0 end
  local k = SPREAD
  local lo = floor(L.x + d * (s - k) / (1 + s * k) - R0)
  local hi = floor(L.x + d * (s + k) / (1 - s * k) + R0)
  if hi < 0 or lo > 255 then return 1, 0 end
  return max(lo, 0), min(hi, 255)
end

-- per-line tables, built once: night sky, and light falling off with height
-- (fog palette and the sub-screen backdrop, COLDATA, dim upward)
local SKY, GLOW, FOGPAL = {}, {}, {}
for y = 0, 223 do
  SKY[y] = ramp_color({ { 0, rgb(6, 6, 22) }, { 90, rgb(28, 18, 56) }, { 150, rgb(72, 34, 72) }, { 200, rgb(96, 48, 56) } }, y)
  local k = clamp(0.25 + y / 170, 0.25, 1)
  GLOW[y] = lerp_color(0, rgb(72, 76, 96), k)
  FOGPAL[y] = {}
  for i = 1, 7 do FOGPAL[y][i] = lerp_color(0, FOGBASE[i], k) end
end

function frame(t, f)
  t = t % LOOP
  local lf = f % (LOOP * 60)
  mode = 1; brightness = 15

  bg[1].char_base, bg[1].map_base = city.char, city.map
  bg[2].char_base, bg[2].map_base = fog.char, fog.map
  bg[3].char_base, bg[3].map_base = rain.char, rain.map
  bg[2].scroll.x = t * 256 / LOOP                 -- stratus drifts one map width per loop
  bg[3].scroll.x = t * 256 / LOOP
  bg[3].scroll.y = -t * 1024 / LOOP               -- rain falls four map heights per loop
  bg3_priority = true                              -- BG3 tiles with prio... rain sits over the city

  screen.main.bg1, screen.main.bg2, screen.main.bg3, screen.main.bg4, screen.main.obj = true, false, true, false, true
  screen.sub.bg1, screen.sub.bg2, screen.sub.bg3, screen.sub.bg4, screen.sub.obj = false, true, false, false, false

  -- the beams: window 1 = lamp A, window 2 = lamp B. Visible only inside either:
  -- invert both, AND them -> masked outside the union.
  for _, l in ipairs({ win.bg2, win.bg3, win.obj }) do
    l.w1, l.w2, l.invert, l.combine = true, true, true, "AND"
  end
  win.bg2.main, win.bg2.sub = false, true
  win.bg3.main, win.bg3.sub = true, false
  win.obj.main, win.obj.sub = true, false
  win.bg1.w1, win.bg1.w2 = false, false
  -- colour math (main + sub fog) only inside the union
  win.color.w1, win.color.w2, win.color.invert, win.color.combine = true, true, false, "OR"
  color.region = "inside"
  color.addend = "sub"; color.op = "add"; color.half = false
  color.on.bg1, color.on.bg2, color.on.bg3, color.on.obj, color.on.backdrop = true, false, true, true, true

  -- the airship
  local sx, sy = ship_pos(t)
  obj.size_sel = 5; obj.char_base = ship.char
  for i = 0, 1 do
    local o, cell = obj[i], ship.cells[i + 1]
    o.on, o.large, o.prio = true, true, 2
    o.tile, o.pal = ship.tile + cell.tile, ship.pal + cell.pal
    o.x, o.y = floor(sx) + i * 64, floor(sy)
  end
  for i = 2, 127 do obj[i].on = false end

  -- city life: windows go dark and come back, beacons pulse
  for k, w in ipairs(TWINKLE) do
    local on = hash(k * 31 + floor(lf / 50) * 7) > 0.22
    cgram[w.i] = on and w.c or NIGHT
  end
  if BEACON then cgram[BEACON] = (lf % 90) < 12 and rgb(255, 48, 32) or rgb(72, 16, 24) end

  local sa, sb = aim(A, t, 0.6, 0.55), aim(B, t, 2.9, 0.6)
  -- below the lamps there is no beam: one frame-wide glow colour, empty windows
  cgram[0] = rgb(80, 40, 64)
  win.w1.lo, win.w1.hi, win.w2.lo, win.w2.hi = 1, 0, 1, 0
  hdma(0, 145, function(y)
    win.w1.lo, win.w1.hi = edges(A, sa, y)
    win.w2.lo, win.w2.hi = edges(B, sb, y)
    cgram[0] = SKY[y]
    color.fixed = GLOW[y]
    local p = FOGPAL[y]
    for i = 1, 7 do cgram[fog.pal + i] = p[i] end
  end)
end
