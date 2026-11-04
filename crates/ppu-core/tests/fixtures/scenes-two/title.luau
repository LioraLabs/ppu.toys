-- The title: a picture at char 0x0000 / map 0x1000 / pal 0, a pulsing tone, and every register
-- the game leaves alone set to something else, so the game shows only what its cold boot gives it.
local picture = dma("title", {char = 0x0000, map = 0x1000, pal = 0})
local tone = dma("piano")
ticks = 0

-- A timer belongs to the scene that registered it: it stops at the switch.
timer(0, 160, function()
  ticks += 1
end)

local function init()
  mode = 1
  bg[1].char_base = picture.char
  bg[1].map_base = picture.map
  screen.main.bg1 = true
  screen.main.bg2 = true
  screen.main.obj = true
  bg[2].scroll.x = 3
  mosaic = 2
  bg[1].mosaic = true
  color.op = "sub"
  color.half = true
  color.fixed = rgb(40, 0, 40)
  color.on.bg1 = true
  win.w1.lo, win.w1.hi = 32, 200
  win.bg1.w1, win.bg1.main = true, true
  m7.a = 0.5
  cgram[200] = rgb(255, 0, 255)
  vram[0x7000] = 0x1234
  bg[1].map[3] = {[3] = {tile = 5, pal = 1}}
  obj[0].x, obj[0].y, obj[0].tile, obj[0].on = 100, 100, 1, true
  voice[0].sample, voice[0].pitch = tone.id, 0x1000
  voice[0].vol.l, voice[0].vol.r = 100, 100
  dsp.mvol = {l = 127, r = 127}
  kon(0)
end

local function frame(t: number, f: number)
  bg[1].scroll.y = -(f % 32)
  brightness = 15 - f % 4
  if pad.a then
    scenes.go(GAME)
  end
end

return {init = init, frame = frame}
