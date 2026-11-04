--!strict
-- The game: its own picture at the title's addresses, a sprite group, and a bell from f = 30.
local picture = dma("game", {char = 0x0000, map = 0x1000, pal = 0})
local hero = dma("hero", {char = 0x6000, pal = 0})
local bell = dma("bell")
local ship = sprites.art({{size = 16, columns = 1, cells = {0}}}, hero)
local ships = sprites.group()

local function init()
  bg[1].char_base = picture.char
  bg[1].map_base = picture.map
  obj.char_base = hero.char
  screen.main.bg1 = true
  screen.main.obj = true
  voice[1].sample, voice[1].pitch = bell.id, 0x1000
  voice[1].vol.l, voice[1].vol.r = 90, 90
  dsp.mvol = {l = 127, r = 127}
end

local function frame(t: number, f: number)
  bg[1].scroll.x = f
  ships:draw(ship, 40 + f, 100)
  if f == 30 then
    kon(1)
  end
end

return {init = init, frame = frame}
