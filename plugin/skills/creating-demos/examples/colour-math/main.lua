-- UNDERSEA :: a colour-math showcase.
-- Everything bright in this scene is ADDED by the PPU, not painted:
--   BG3 lives only on the SUB screen (light.lua): surface ripples, sun shafts
--   and caustics. Colour math adds that sub screen onto the main screen
--   (reef, ruins, fish), and where BG3 is empty it adds COLDATA instead: a
--   per-scanline blue-green haze. One HDMA pass re-aims every line.

local reef = dma("reef", { char = 0x2000, map = 0x0000, pal = 32 })
local ruins = dma("ruins", { char = 0x4000, map = 0x0400, pal = 80 })
local fish = dma("fish", { char = 0x6000, pal = 4 })
local bub = dma("bubbles", { char = fish.next_char, pal = 5 })

local TAU = 6.283185307179586
local LOOP = 12
local SURFACE, RAYS, CAUSTICS = 22, 176, 223 -- band edges

-- 0..255 RGB -> packed BGR555
local function C(r, g, b)
  r, g, b = clamp(floor(r / 8), 0, 31), clamp(floor(g / 8), 0, 31), clamp(floor(b / 8), 0, 31)
  return r + g * 32 + b * 1024
end

-- per-line tables, built once
local BACK, FOG, VD, CROW, SFIX = {}, {}, {}, {}, {}
local BAY = { [0] = 0.125, 0.625, 0.375, 0.875 }
local back_keys = { { 0, C(120, 214, 226) }, { 20, C(70, 176, 204) }, { 60, C(34, 122, 160) },
  { 120, C(18, 80, 126) }, { 180, C(10, 52, 94) }, { 223, C(6, 30, 64) } }
for y = 0, 223 do
  BACK[y] = ramp_color(back_keys, y)
  -- haze: strongest mid-water where the ruins stand, none near the camera
  local h = clamp(1 - abs(y - 100) / 90, 0, 1)
  FOG[y] = C(4 + 10 * h, 26 * h + 6, 30 * h + 8)
  SFIX[y] = ramp_color({ { 0, C(56, 110, 112) }, { SURFACE - 1, C(14, 40, 46) } }, y)
  VD[y] = clamp((y - SURFACE) / 150, 0, 1) * 5.2
  -- caustics: compress the top rows so cells shrink into the distance
  local s = clamp((y - RAYS) / (CAUSTICS - RAYS), 0, 1)
  CROW[y] = 96 + floor(47 * s ^ 0.75)
end

-- additive light ramps: R[level][k] for k = 0..16
local function ramp3(c1, c2, c3)
  local R, cs = { {}, {}, {} }, { c1, c2, c3 }
  for l = 1, 3 do
    for k = 0, 16 do
      local c = cs[l]
      R[l][k] = C(c[1] * k / 16, c[2] * k / 16, c[3] * k / 16)
    end
  end
  return R
end
local RAY = ramp3({ 30, 46, 40 }, { 64, 90, 72 }, { 120, 150, 112 })
local CAU = ramp3({ 20, 30, 22 }, { 50, 66, 44 }, { 110, 128, 84 })
local SURF = { C(120, 190, 200), C(190, 240, 240), C(255, 255, 248) }

-- sprite choreography
local SCHOOL = {}
for i = 0, 17 do
  local a = i * 2.39996
  local r = sqrt(i / 18)
  SCHOOL[i] = { ox = cos(a) * r * 54, oy = sin(a) * r * 17, ph = i * 0.7 }
end
local STREAMS = { { 84, 0 }, { 172, 0.5 }, { 226, 0.25 } }

-- per-line hooks (registered every frame; read the frame's ph/cam/slope)
ph, cam, slope = 0, 0, 0
local function sway(y)
  local amp = clamp((170 - y) / 130, 0, 1) * 3.5
  bg[1].scroll.x = cam + amp * sin(ph * 3 - y * 0.045)
end
function surface_line(y)
  -- translucent surface: half of (scene + ripples)
  cgram[0] = BACK[y]
  sway(y)
  color.half = true
  color.fixed = SFIX[y]
  bg[3].scroll.x = cam * 0.2 + 3 * sin(y * 0.35 + ph * 6)
  bg[3].scroll.y = floor(1.5 * sin(y * 0.5 + ph * 4))
end
function ray_line(y)
  -- sun shafts: vertical tile rows slanted by a per-line skew; the row
  -- picked per line (a dimmer depth variant) fades them, a pulse runs down
  cgram[0] = BACK[y]
  sway(y)
  color.fixed = FOG[y]
  bg[3].scroll.x = cam * 0.25 - (y - SURFACE) * slope
  local v = clamp(floor(VD[y] - 0.9 * sin(y * 0.05 - ph * 6) + BAY[y % 4]), 0, 5)
  bg[3].scroll.y = 32 + v * 8 - (y - y % 8)
end
function caustic_line(y)
  -- caustics ride the sand's own scroll and ripple per line
  cgram[0] = BACK[y]
  bg[1].scroll.x = cam
  color.fixed = FOG[y]
  bg[3].scroll.x = cam + 2 * sin(y * 0.33 + ph * 5)
  bg[3].scroll.y = CROW[y] - y + floor(1.2 * sin(y * 0.6 + ph * 5))
end

function frame(t, f)
  t = (f % (LOOP * 60)) / 60 -- exact loop: derive time from the frame count
  mode = 1
  brightness = 15
  screen.main.bg1, screen.main.bg2, screen.main.bg3, screen.main.bg4, screen.main.obj = true, true, false, false, true
  screen.sub.bg1, screen.sub.bg2, screen.sub.bg3, screen.sub.bg4, screen.sub.obj = false, false, true, false, false
  bg[1].char_base, bg[1].map_base = reef.char, reef.map
  bg[2].char_base, bg[2].map_base = ruins.char, ruins.map
  bg[3].char_base, bg[3].map_base = 0x1000, 0x0800
  obj.char_base, obj.size_sel = fish.char, 0

  color.addend = "sub"
  color.op = "add"
  color.on.bg1, color.on.bg2, color.on.obj, color.on.backdrop = true, true, true, true
  color.on.bg3, color.on.bg4 = false, false

  ph = TAU * t / LOOP
  cam = 14 * sin(ph)
  slope = 0.42 + 0.1 * sin(ph + 1.1)
  -- four ray groups (BG3 palettes 0-3) breathe independently: frame-wide
  local I = { 0.6 + 0.4 * sin(ph + 0.3), 0.6 + 0.4 * sin(ph * 2 + 2.0),
    0.55 + 0.45 * sin(ph * 3 + 4.1), 0.62 + 0.38 * sin(ph * 2 + 5.0) }
  for g = 1, 4 do
    local k, b = floor(16 * I[g] + 0.5), (g - 1) * 4
    cgram[b + 1], cgram[b + 2], cgram[b + 3] = RAY[1][k], RAY[2][k], RAY[3][k]
  end
  local k = floor(16 * (0.85 + 0.15 * sin(ph * 4)) + 0.5)
  cgram[21], cgram[22], cgram[23] = CAU[1][k], CAU[2][k], CAU[3][k]
  cgram[17], cgram[18], cgram[19] = SURF[1], SURF[2], SURF[3]

  bg[2].scroll.x = cam * 0.4
  bg[1].scroll.y, bg[2].scroll.y = 0, 0
  color.half = false
  hdma(0, SURFACE - 1, surface_line)
  hdma(SURFACE, RAYS - 1, ray_line)
  hdma(RAYS, 223, caustic_line)

  -- OAM
  local slot = 0
  local function spr(tile, pal, x, y, large)
    local o = obj[slot]
    o.on, o.tile, o.pal, o.x, o.y, o.large, o.prio = true, tile, pal, floor(x), floor(y), large, 2
    o.flip_x, o.flip_y = false, false
    slot = slot + 1
  end

  -- the school sweeps right-to-left once per loop
  local u = (t % LOOP) / LOOP
  local lx = 300 - u * 430 - cam * 0.9
  local ly = 104 + 12 * sin(u * TAU * 1.5) - 10 * u
  for i = 0, 17 do
    local s = SCHOOL[i]
    local x = lx + s.ox + 3 * sin(ph * 4 + s.ph)
    local y = ly + s.oy + 2 * sin(ph * 6 + s.ph * 1.3)
    if x > -16 and x < 256 then
      local c = fish.cells[floor(t * 10 + i * 1.7) % 4 + 1]
      spr(fish.tile + c.tile, fish.pal + c.pal, x, y, true)
    end
  end
  -- a distant school milling about the temple, deep in the haze
  for i = 0, 9 do
    local x = 150 + (i % 5) * 14 + 22 * sin(ph + i * 0.4) - cam * 0.45
    local y = 72 + floor(i / 5) * 9 + (i % 3) * 3 + 4 * sin(ph * 2 + i)
    if x > -8 and x < 256 then
      local c = bub.cells[5 + floor(t * 6 + i) % 2]
      spr(bub.tile + c.tile, bub.pal + c.pal, x, y, false)
    end
  end
  -- bubbles: three streams, rising every 4 s, growing as they rise
  for si = 1, #STREAMS do
    local sx, off = STREAMS[si][1], STREAMS[si][2]
    for i = 0, 4 do
      local p = ((t / 4) + off + i / 5) % 1
      local y = 182 - p * 170
      if y > SURFACE then
        local x = sx + 3 * sin(y * 0.09 + i * 2) - cam
        local c = bub.cells[4 - floor(p * 3.99)]
        spr(bub.tile + c.tile, bub.pal + c.pal, x - 4, y, false)
      end
    end
  end
  for i = slot, 127 do obj[i].on = false end
end
