-- showcase-finale: 128 sprites and the sound chip build the ppu.toys logo.
-- One firefly per note at first, then a swarm, then every sprite finds its
-- pixel and the logo lands on the downbeat (step 96, 15 s at 96 BPM).
local PI = 3.141592653589793
local LAND = SONG_LAND * 60 / (SONG_TEMPO * 4)   -- 15 s
local LOGO_Y = 62                                -- logo.png row 0 on screen
local N = 128

local logo = dma("logo", { char = 0x0000, map = 0x3c00, pal = 0 })
local sparks = dma("sparks", { char = 0x6000, pal = 4 })

local function hash(n) local x = sin(n * 12.9898) * 43758.5453; return x - floor(x) end
local function smooth(v) v = clamp(v, 0, 1); return v * v * (3 - 2 * v) end

local tune
function init()
  dsp.echo = { delay = 6, feedback = 70, fir = { 96, 24, 8, 0, 0, 0, 0, 0 } }
  dsp.evol = { l = 52, r = -52 }
  dsp.mvol = { l = 90, r = 90 }
  local pad = bank("strings", { adsr = { a = 7, d = 7, s = 6, r = 9 }, vol = 34, echo = true })
  local tracks = {
    { voice = 0, inst = pad, pattern = PATTERNS.pad1 },
    { voice = 1, inst = bank("strings", { adsr = { a = 7, d = 7, s = 6, r = 9 }, vol = 30, pan = -0.4, echo = true }), pattern = PATTERNS.pad2 },
    { voice = 2, inst = bank("strings", { adsr = { a = 7, d = 7, s = 6, r = 9 }, vol = 30, pan = 0.4, echo = true }), pattern = PATTERNS.pad3 },
    { voice = 3, inst = bank("pluck", { vol = 46, pan = 0.25, echo = true }), pattern = PATTERNS.arp },
    { voice = 4, inst = bank("bell", { vol = 58, pan = -0.2, echo = true }), pattern = PATTERNS.bell },
    { voice = 5, inst = bank("bass", { vol = 80 }), pattern = PATTERNS.bass },
    { voice = 6, inst = bank("kick", { vol = 96 }), pattern = PATTERNS.kick },
    { voice = 7, inst = bank("snare", { vol = 50 }), pattern = PATTERNS.snare },
    { voice = 7, inst = bank("crash", { vol = 90, echo = true }), pattern = PATTERNS.crash },
  }
  tune = song{ tempo = SONG_TEMPO, steps = SONG_STEPS, tracks = tracks }

  mode = 1
  bg[1].char_base = logo.char
  bg[1].map_base = logo.map
  obj.char_base = sparks.char
  obj.size_sel = 0
end

-- Where sprite i flies in the swarm at time t: a tilted, turning ring of
-- fireflies, projected with perspective. Returns x, y and depth 0..1 (near).
local function swarm(i, t, grow)
  local a = i / N * PI * 2 * 3 + t * (0.55 + hash(i) * 0.25)
  local r = (54 + hash(i + 7) * 12 + sin(a * 3 - t) * 6) * (0.3 + 0.7 * grow)
  local h = sin(a * 2 - t * 1.3) * 12 + (hash(i + 3) - 0.5) * 6
  local x, z = cos(a) * r, sin(a) * r
  local tilt = 1.0 + sin(t * 0.3) * 0.2
  local y = h * cos(tilt) - z * sin(tilt)
  z = h * sin(tilt) + z * cos(tilt)
  local yaw = t * 0.21
  x, z = x * cos(yaw) - z * sin(yaw), x * sin(yaw) + z * cos(yaw)
  local s = 170 / (170 + z)
  local cx = 128 + sin(t * 0.37) * 18
  local cy = 104 + sin(t * 0.29) * 10
  return cx + x * s, cy + y * s, clamp(0.5 - z / 160, 0, 1)
end

local function put(i, x, y, cell, pal)
  local o = obj[i]
  if cell < 0 or x < -8 or x > 255 or y < -8 or y > 223 then o.on = false; return end
  o.on = true; o.x = floor(x) - 4; o.y = floor(y) - 4
  o.tile = sparks.tile + cell; o.pal = pal or 4; o.prio = 3; o.large = false
  o.flip_x = false; o.flip_y = false
end

-- Backdrop keys in 5-bit channels; gradient() dithers between neighbouring
-- 5-bit levels line by line so the dark ramp doesn't band.
local SKY = { { 0, 1, 0.5, 3 }, { 120, 3, 1.5, 6.5 }, { 223, 7, 2.8, 8.5 } }
local GLOW = { { 0, 1.5, 1, 4.5 }, { 104, 6, 3.5, 12 }, { 223, 9, 3.8, 10 } }
local DITHER = { 0, 0.5, 0.25, 0.75 }
local function key(keys, y, ch)
  for k = 2, #keys do
    local a, b = keys[k - 1], keys[k]
    if y <= b[1] then return lerp(a[ch], b[ch], smooth((y - a[1]) / (b[1] - a[1]))) end
  end
  return keys[#keys][ch]
end
local function gradient(y, lift)
  local d, c = DITHER[y % 4 + 1], 0
  for ch = 2, 4 do
    local v = floor(lerp(key(SKY, y, ch), key(GLOW, y, ch), lift) + d)
    c = c + clamp(v, 0, 31) * (ch == 2 and 1 or ch == 3 and 32 or 1024)
  end
  return c
end

function frame(t, f)
  if t >= LAND + 5 and tune.playing then tune.stop() end   -- one pass, no loop
  brightness = 15
  screen.main.obj = true
  local landed = t >= LAND
  screen.main.bg1 = landed
  screen.sub.bg1 = landed
  mosaic = landed and floor(15 * (1 - smooth((t - LAND) / 1.1))) or 0
  bg[1].mosaic = true
  bg[1].scroll.x = 0
  bg[1].scroll.y = -LOGO_Y

  -- sprites: bloom out of one point, swarm, converge, burst, then embers
  local grow = smooth(t / 10)
  for i = 0, N - 1 do
    local born = 0.15 + 9.6 * (i / N) ^ 0.85
    local tx, ty = TARGETS[i + 1][1], TARGETS[i + 1][2] + LOGO_Y
    if t < born then
      obj[i].on = false
    elseif not landed then
      local x, y, depth = swarm(i, t, grow)
      local age = t - born
      if age < 0.9 then                       -- fly out from the bloom point
        local k = smooth(age / 0.9)
        x, y = lerp(128, x, k), lerp(104, y, k)
      end
      local go = LAND - 4.2 + 1.6 * (i / N)
      local k = smooth((t - go) / (LAND - go))
      local swirl = sin(k * PI) * 26 * (hash(i + 11) - 0.5)
      x = lerp(x, tx, k) + swirl
      y = lerp(y, ty, k) - swirl * 0.5
      local cell = floor(1.5 + depth * 4.5 + sin(t * 9 + i) * 0.8)
      if age < 0.25 then cell = 7 end         -- birth flash
      if k > 0.97 then cell = 5 end
      put(i, x, y, clamp(cell, 0, 6))
    else
      local tau = t - LAND
      local dx, dy = tx - 128, ty - (LOGO_Y + 43)
      local len = sqrt(dx * dx + dy * dy) + 1
      local speed = 50 + hash(i + 5) * 70
      local d = (1 - 1 / (1 + tau * 1.8)) * speed
      local cell = 7 - floor(tau * 2.8)
      if cell >= 0 then
        put(i, tx + dx / len * d, ty + dy / len * d - tau * tau * 4, cell)
      elseif i % 5 == 0 then                  -- 26 embers drift up through the logo
        local life = (tau - 2.5 + hash(i) * 6) % 6
        local ex = tx + sin(life * 1.3 + i) * 10
        local ey = 230 - life * 42
        local tw = floor(2 + sin(t * 7 + i * 3) * 1.6)
        put(i, ex, ey, life < 0.4 and -1 or tw)
      else
        obj[i].on = false
      end
    end
  end

  -- colour math: sprites (OBJ pal 4-7) add onto whatever is behind them; the
  -- landing adds a white flash to everything for a third of a second
  local flash = landed and clamp(1 - (t - LAND) / 0.3, 0, 1) * 0.55 or 0
  color.op = "add"
  color.half = false
  color.on.obj = true
  color.on.bg1 = flash > 0
  color.on.backdrop = flash > 0
  color.addend = flash > 0 and "fixed" or "sub"
  local lift = landed and smooth((t - LAND) / 1.5) or 0
  hdma(0, 223, function(y)
    local c = gradient(y, lift)
    cgram[0] = c
    if flash > 0 then
      local w = floor(flash * 255)
      color.fixed = rgb(w, w, w)
    else
      color.fixed = c
    end
  end)
end
