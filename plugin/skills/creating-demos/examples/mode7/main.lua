-- Skyward :: Mode 7 at golden hour.
-- The sea is ONE flat 1024x1024 picture. Every scanline below the horizon
-- re-aims the Mode 7 matrix at a different strip of it (near lines read a
-- few texels, far lines read hundreds): that is the whole 3D. Above the
-- horizon each line switches to Mode 1 for real tile layers: a cloud
-- ceiling with its own per-line perspective, and a far bank with the sun.
-- Colour math adds the haze; a window cuts the craft's shadow into the sea.

local HORIZON = 88      -- first sea line is HORIZON + 1
local HEIGHT  = 30      -- eye height over the water, in texels
local FOCAL   = 128     -- lens length in pixels
local LOOP    = 720     -- frames; everything below repeats exactly
local SWING   = 0.5     -- heading swing either side, radians
local TURNS   = 2       -- S-turns per loop
local TRAVEL_X, TRAVEL_Y = 1024, 2048  -- texels flown per loop; the halves see different islands
local BIAS    = 0.4636476  -- atan(1024 / 2048): fly the diagonal
local Q       = 16      -- Mode 7 lever arm (see the floor below)

dma("world")
local ceil  = dma("ceiling", { char = 0x4000, map = 0x5C00, pal = 48 })
local bank  = dma("bank",    { char = 0x5000, map = 0x7C00, pal = 64 })
local craft = dma("craft",   { char = 0x6000, pal = 0 })

-- Heading and position for every frame of the loop, integrated once.
local HEAD, POSX, POSY, YAW = {}, {}, {}, {}
do
  local w = 2 * pi * TURNS / LOOP
  local x, y = 0, 0
  for f = 0, LOOP do
    HEAD[f] = BIAS + SWING * sin(w * f)
    YAW[f] = SWING * w * cos(w * f)    -- turn rate, radians per frame
    POSX[f], POSY[f] = x, y
    for k = 0, 7 do                    -- 8 substeps per frame
      local h = BIAS + SWING * sin(w * (f + k / 8 + 1 / 16))
      x, y = x + sin(h) / 8, y - cos(h) / 8
    end
  end
  local sx, sy = POSX[LOOP], POSY[LOOP]
  for f = 0, LOOP do                   -- land exactly a whole number of laps away: the loop closes
    POSX[f] = 512 + POSX[f] * TRAVEL_X / sx
    POSY[f] = 512 + POSY[f] * TRAVEL_Y / -sy
  end
end

local function mix(a, b, k) return a + (b - a) * k end

-- Sky backdrop, top to horizon: indigo, mauve, ember, pale gold.
local SKY = {
  { 0, rgb(40, 40, 104) }, { 30, rgb(96, 64, 136) }, { 58, rgb(200, 104, 120) },
  { 76, rgb(248, 152, 104) }, { 88, rgb(255, 216, 152) },
}

-- Per-line tables, computed once: the hook below only looks things up.
local SKYC, CEILV, HAZE, HALF, GLINT, SCALE = {}, {}, {}, {}, {}, {}
for y = 0, HORIZON do
  SKYC[y] = ramp_color(SKY, y)
  CEILV[y] = 2600 / (HORIZON + 6 - y)          -- cloud-ceiling perspective
end
for dy = 1, 223 - HORIZON do
  SCALE[dy] = HEIGHT / dy                       -- texels per pixel on this row
  if dy <= 12 then
    -- far rows: a true 50/50 blend toward the horizon glow (half math)
    local k = (dy - 1) / 11
    HALF[dy] = true
    HAZE[dy] = rgb(mix(255, 172, k), mix(216, 152, k), mix(160, 136, k))
  else
    -- nearer: a warm wash that fades out by row 40
    local h = clamp((40 - dy) / 28, 0, 1)
    h = h * h
    HALF[dy] = false
    HAZE[dy] = rgb(70 * h, 44 * h, 16 * h)
  end
  local g = clamp(1 - dy / 50, 0, 1)            -- sun glints: hot far away, cool up close
  local c = M7PAL.glint0
  GLINT[dy] = rgb(mix(c.r, 255, g), mix(c.g, 224, g), mix(c.b, 152, g))
end

local WATER = { M7PAL.deep0, M7PAL.deep1, M7PAL.deep2, M7PAL.deep3 }
local SHADOW_SUB = rgb(40, 32, 8)
local SHADE_AT = {}

function frame(t, f)
  local i = f % LOOP
  local th, yaw = HEAD[i], YAW[i]
  local cs, sn = cos(th), sin(th)
  local camx, camy = POSX[i], POSY[i]

  -- Frame-wide defaults; the hook overrides per line.
  brightness = 15
  mode = 7
  screen.main.obj = true
  bg[1].char_base, bg[1].map_base = ceil.char, ceil.map   -- read on Mode 1 lines only
  bg[2].char_base, bg[2].map_base = bank.char, bank.map
  m7.cx, m7.cy, m7.wrap = 0, 0, 0
  color.addend = "fixed"
  color.op = "add"
  win.color.w1 = false
  color.region = "everywhere"
  color.on.bg1 = false
  screen.main.bg1 = true
  screen.main.bg2 = false

  -- Water: four crest colours rotate through the wave dashes.
  local k = floor(i / 6)
  for p = 1, 4 do
    local c = WATER[(p + k) % 4 + 1]
    cgram[WATER[p].i] = rgb(c.r, c.g, c.b)
  end
  local twinkle = floor(i / 4) % 2
  local gon = twinkle == 0 and M7PAL.glint0.i or M7PAL.glint1.i
  local goff = twinkle == 0 and M7PAL.glint1 or M7PAL.glint0
  cgram[goff.i] = rgb(goff.r, goff.g, goff.b)

  -- The craft: bank into the turn, bob a little, slide toward the inside.
  local bankv = clamp(yaw / (SWING * 2 * pi * TURNS / LOOP), -1, 1)
  local pose = 4 - floor(bankv * 3 + 0.5)            -- 1..7
  local px = floor(128 + bankv * 14)
  local py = floor(132 + sin(i / LOOP * 2 * pi * 6) * 2.5)
  obj.size_sel = 3
  obj.char_base = craft.char
  for c = 0, 3 do
    local cell = craft.cells[(c // 2) * 14 + (pose - 1) * 2 + c % 2 + 1]
    local o = obj[c]
    o.on, o.large, o.prio = true, true, 3
    o.tile, o.pal = craft.tile + cell.tile, craft.pal + cell.pal
    o.flip_x, o.flip_y = cell.flip_x, cell.flip_y
    o.x, o.y = px - 32 + (c % 2) * 32, py - 32 + (c // 2) * 32
  end

  -- Its shadow: a window shaped per line, colour math subtracts inside it.
  for y in pairs(SHADE_AT) do SHADE_AT[y] = nil end
  local sy = py + 50
  for _, row in ipairs(SHADOW[pose]) do
    SHADE_AT[sy + row[1]] = { px + 4 + row[2], px + 4 + row[3] }
  end

  -- Clouds overhead stream toward us; everything far turns with the heading.
  local drift = i / LOOP * 256
  local turnx = (th - BIAS) * FOCAL     -- the sun sits on our mean course
  bg[2].scroll.x, bg[2].scroll.y = turnx, -40
  local ceilx = turnx * 1.15
  -- Mode 7 lever arm: with cx = cy = 0 and a per-line vertical scroll of
  -- Q - y, m7.b and m7.d multiply a constant Q, so they carry the row's
  -- centre point with 1/16-texel precision (M7X/M7Y alone are whole texels).
  local fx, fy = sn * FOCAL / Q, -cs * FOCAL / Q
  local bx, by = camx / Q, camy / Q

  hdma(0, 223, function(y)
    if y <= HORIZON then
      mode = 1
      cgram[0] = SKYC[y]
      screen.main.bg1 = y < 54
      screen.main.bg2 = y >= 38
      bg[1].scroll.x = ceilx
      bg[1].scroll.y = (drift + CEILV[y]) % 64 - y
    else
      local dy = y - HORIZON
      local s = SCALE[dy]
      m7.a, m7.c = s * cs, s * sn
      m7.b = (bx + s * fx) % 64
      m7.d = (by + s * fy) % 64
      bg[1].scroll.x = -128
      bg[1].scroll.y = Q - y
      screen.main.bg1 = true
      color.on.bg1 = true
      cgram[gon] = GLINT[dy]
      local shade = SHADE_AT[y]
      if shade then
        win.w1.lo, win.w1.hi = shade[1], shade[2]
        win.color.w1 = true
        color.region = "inside"
        color.op = "sub"
        color.half = false
        color.fixed = SHADOW_SUB
      else
        color.half = HALF[dy]
        color.fixed = HAZE[dy]
      end
    end
  end)
end
