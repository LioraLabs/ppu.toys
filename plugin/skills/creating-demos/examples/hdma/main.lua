-- Mirror :: a lake at dusk. Nothing below the horizon is drawn.
-- Every water scanline points the backgrounds' vertical scroll back up at
-- the row it mirrors (HDMA), nudges it sideways for ripples, and darkens it
-- with fixed-colour subtraction. The sun's stripes and the sky gradient are
-- per-line CGRAM writes, so the reflection gets them for free.

local FAR = dma("far", { char = 0x0000, map = 0x6000, pal = 0 })    -- BG3 2bpp: sun 1, range 2, clouds 3
local MID = dma("mid", { char = 0x1000, map = 0x6800, pal = 32 })   -- BG2: tones 1..7
local NEAR = dma("near", { char = 0x2000, map = 0x7000, pal = 48 }) -- BG1: body 1, rim 2, windows 3
local LAN = dma("lantern", { char = 0x4000, pal = 4 })              -- OBJ slot 4 = CGRAM 192

HORIZON = 132     -- first water line
LOOP = 12         -- seconds; every motion below is periodic in it
LANTERNS = 12
TAU = 6.283185307179586

local SKY_A = { { 0, rgb(22, 16, 64) }, { 40, rgb(58, 30, 104) }, { 78, rgb(150, 56, 120) },
                { 108, rgb(232, 100, 98) }, { 124, rgb(255, 156, 84) }, { 131, rgb(255, 206, 128) } }
local SKY_B = { { 0, rgb(14, 14, 52) }, { 44, rgb(70, 28, 108) }, { 84, rgb(176, 58, 110) },
                { 110, rgb(246, 116, 88) }, { 125, rgb(255, 176, 92) }, { 131, rgb(255, 222, 150) } }
local SUN = { { 50, rgb(255, 246, 170) }, { 88, rgb(255, 184, 72) }, { 132, rgb(255, 76, 112) } }
local RANGE = rgb(64, 26, 86)
local CLOUD = rgb(255, 178, 160)
local GOLD = rgb(48, 34, 10)

local MID_TONES = { rgb(36, 16, 54), rgb(46, 20, 64), rgb(58, 26, 76), rgb(84, 38, 94),
                    rgb(118, 54, 108), rgb(156, 78, 118), rgb(255, 150, 108) }

local function hash(n) local x = sin(n * 12.9898) * 43758.5453; return x - floor(x) end

-- Colour tables, built once. The sky breathes between two gradients; that
-- blend is quantised to 32 steps so a scanline is a lookup, not a ramp.
local STEPS = 32
local SKY, RNG, CLD, SUNC, WATER = {}, {}, {}, {}, {}
for q = 0, STEPS do
  SKY[q], RNG[q], CLD[q] = {}, {}, {}
  for r = 0, HORIZON - 1 do
    local c = lerp_color(ramp_color(SKY_A, r), ramp_color(SKY_B, r), q / STEPS)
    SKY[q][r], RNG[q][r], CLD[q][r] = c, lerp_color(c, RANGE, 0.62), lerp_color(c, CLOUD, 0.45)
  end
end
for r = 0, HORIZON - 1 do SUNC[r] = ramp_color(SUN, r) end
for d = 1, 91 do
  local k = d / 91 -- deeper water = darker, with green pulled hardest so gold turns rose
  WATER[d] = rgb(30 + 60 * k, 56 + 96 * k, 16 + 30 * k)
end

-- Sun stripes: gaps march down the lower half and thicken toward the water.
local function sun_gap(r, t)
  if r < 84 then return false end
  return (r - t * 2.25) % 9 < (r - 84) / 48 * 7
end

-- Horizontal ripple at water depth d (0 at the horizon): perspective-spaced
-- waves, plus one gust that rolls toward the viewer each loop. The wind drops
-- to a near-glassy calm at the loop seam, then picks up.
local function ripple(d, t, calm, gust_at)
  local amp = (0.35 + d / 91 * 4.5) * calm
  local w = sin(150 / (d + 5) - t * TAU) * 0.7 + sin(d * 0.37 + t * TAU * 7 / LOOP) * 0.3
  local q = (d - gust_at) / 10
  return w * (amp + 3 * calm / (1 + q * q * q * q))
end

function frame(t, f)
  mode = 1
  brightness = 15
  obj.char_base, obj.size_sel = LAN.char, 0
  for n, src in pairs({ [1] = NEAR, [2] = MID, [3] = FAR }) do
    bg[n].char_base, bg[n].map_base, bg[n].screen_size = src.char, src.map, src.screen_size
  end
  screen.main.bg1, screen.main.bg2, screen.main.bg3, screen.main.obj = true, true, true, true
  color.addend = "fixed"
  color.on.bg1, color.on.bg2, color.on.bg3, color.on.backdrop, color.on.obj = true, true, true, true, true
  color.op = "sub"
  color.fixed = 0

  local ph = t * TAU / LOOP
  local q = floor((0.5 + 0.5 * sin(ph)) * STEPS + 0.5)
  local sky, rng, cld = SKY[q], RNG[q], CLD[q]
  local calm = 0.3 + 0.7 * (0.5 - 0.5 * cos(ph))
  local gust_at = (t / LOOP) % 1 * 150 - 30
  local nx, mx, fx = 128 + 16 * sin(ph), 128 + 8 * sin(ph), 128 + 2 * sin(ph)
  local s1, s2, s3 = bg[1].scroll, bg[2].scroll, bg[3].scroll
  s1.x, s2.x, s3.x = nx, mx, fx
  s1.y, s2.y, s3.y = 0, 0, 0

  -- frame-wide palettes: mountains, shore, lit windows, lanterns
  for i = 1, 7 do cgram[32 + i] = MID_TONES[i] end
  cgram[49] = rgb(14, 6, 24)
  cgram[50] = rgb(170, 72, 84)
  local glow = 0.85 + 0.15 * sin(ph * 9) * sin(ph * 5)
  cgram[51] = rgb(255 * glow, 196 * glow, 96 * glow)
  cgram[193] = rgb(150, 44, 32)
  cgram[194] = rgb(255, 120 * (0.9 + 0.1 * sin(ph * 23)), 40)
  cgram[195] = rgb(255, 236, 150)

  -- one scanline's palette: backdrop, sun (or a gap the colour of the sky),
  -- distant range, clouds. Rows are SOURCE rows, so the lake reuses them.
  local function paint(r)
    local c = sky[r]
    cgram[0] = c
    if r >= 50 then cgram[1] = sun_gap(r, t) and c or SUNC[r] end
    if r >= 100 then cgram[2] = rng[r] elseif r >= 38 and r < 94 then cgram[3] = cld[r] end
  end

  hdma(0, HORIZON - 1, function(y)
    paint(y)
    if y > 112 then s3.x = fx + sin(y * 2.1 + ph * 36) * (y - 112) / 20 end -- heat shimmer
  end)

  hdma(HORIZON, 223, function(y)
    local d = y - HORIZON
    -- vertical jitter tears the reflection into horizontal strips
    local jit = floor(sin(d * 1.7 + ph * 24) * min(d, 50) / 50 * 1.6 + 0.5)
    local r = clamp(HORIZON - 1 - d + jit, 0, HORIZON - 1)
    paint(r)
    local wob = ripple(d, t, calm, gust_at)
    s1.x, s2.x, s3.x = nx + wob, mx + wob, fx + wob
    local vy = r - y -- read row r on line y: the mirror
    s1.y, s2.y, s3.y = vy, vy, vy
    if d == 0 then
      color.op = "add" -- a thin gold seam where the sky meets its reflection
      color.fixed = GOLD
    else
      color.fixed = WATER[d]
    end
  end)
  -- Sky lanterns rise from the far shore. Sprites aren't scrolled, so each
  -- one gets an explicit upside-down twin in the water.
  local slot = 0
  for i = 0, LANTERNS - 1 do
    local far = i % 2 == 1
    local p = (t / LOOP + hash(i + 1)) % 1
    local y = HORIZON - 8 - p * (far and 90 or 140)
    local x = 20 + hash(i + 7) * 216 + sin(t * TAU / LOOP * 2 + i) * 6 + (far and mx or nx) - 128
    local cell = LAN.cells[far and 2 or 1]
    for twin = 0, 1 do
      local o = obj[slot]
      local yy = y
      local xx = x
      if twin == 1 then
        yy = 2 * HORIZON - y - 8
        xx = x + ripple(yy - HORIZON + 4, t, calm, gust_at)
      end
      o.on = y > -8 and (twin == 0 or yy < 224)
      o.x, o.y = xx, yy
      o.tile, o.pal = LAN.tile + cell.tile, LAN.pal + cell.pal
      o.flip_x, o.flip_y, o.large = false, twin == 1, false
      o.prio = far and 1 or 3
      slot = slot + 1
    end
  end
  -- a handful of stars in the deep sky; they twinkle by swapping cells
  for i = 0, 17 do
    local o = obj[slot]
    local tw = sin(t * TAU / LOOP * (2 + i % 3) + i * 1.7)
    o.on = tw > -0.6
    o.x = hash(i + 40) * 256 + (fx - 128)
    o.y = hash(i + 80) * 34 - 3
    local cell = LAN.cells[tw > 0.55 and 4 or 3]
    o.tile, o.pal, o.prio = LAN.tile + cell.tile, LAN.pal + cell.pal, 0
    o.flip_x, o.flip_y, o.large = false, false, false
    slot = slot + 1
  end
  for i = slot, 127 do obj[i].on = false end
end
