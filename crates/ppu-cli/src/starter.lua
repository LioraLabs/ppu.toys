-- Edit assets/floor.png, then use ppu render to inspect your changes.
local plane = dma("floor")
FLIGHT_SPEED = 16
CAMERA_HEIGHT = 40
LOOP_SECONDS = 8

function frame(t, f)
  local u = t % LOOP_SECONDS
  local angle = sin(u * 6.283185307179586 / LOOP_SECONDS) * 0.3
  local ca, sa = cos(angle), sin(angle)
  mode, brightness, force_blank = 7, 15, false
  screen.main.bg1 = true
  m7.wrap, m7.b, m7.d = 0, 0, 0
  cgram[0] = rgb(8, 8, 32)
  hdma(0, 223, function(y)
    local down = y - 76
    screen.main.bg1 = down > 0
    if down > 0 then
      local scale = min(16, CAMERA_HEIGHT / down)
      local distance = scale * 128
      m7.a, m7.c = ca * scale, sa * scale
      m7.cx = 512 - sa * distance
      m7.cy = (512 - u * FLIGHT_SPEED - ca * distance) % 1024
      bg[1].scroll.x, bg[1].scroll.y = m7.cx - 128, 0
    else
      cgram[0] = rgb(8 + y * 0.4, 8 + y * 0.2, 32 + y * 0.7)
    end
  end)
end
