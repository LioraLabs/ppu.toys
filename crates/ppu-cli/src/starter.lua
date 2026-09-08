-- Edit assets/floor.png, then use ppu render to inspect your changes.
local plane = dma("floor")
FLIGHT_SPEED = 16
CAMERA_HEIGHT = 40

function frame(t, f)
  local u = t % markers.loop_end
  local angle = sin(u * 6.283185307179586 / markers.loop_end) * 0.3
  local ca, sa = cos(angle), sin(angle)
  mode, brightness, force_blank = 7, 15, false
  screen.main.bg1 = true
  m7.wrap = 0
  cgram[0] = rgb(8, 8, 32)
  hdma(0, 223, function(y)
    local down = y - 76
    screen.main.bg1 = down > 0
    if down > 0 then
      local scale = min(16, CAMERA_HEIGHT / down)
      local distance = scale * 128
      m7.a, m7.b, m7.c, m7.d = ca * scale, 0, sa * scale, 0
      m7.cx = 512 - sa * distance
      m7.cy = (512 - u * FLIGHT_SPEED - ca * distance) % 1024
      bg[1].scroll.x, bg[1].scroll.y = m7.cx - 128, 0
    else
      cgram[0] = rgb(8 + y * 0.4, 8 + y * 0.2, 32 + y * 0.7)
    end
  end)
end
