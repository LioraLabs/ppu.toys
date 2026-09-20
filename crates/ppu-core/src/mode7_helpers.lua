-- Pure views: callers assign the returned fields. This also lets generated
-- ppuglobals.lua writes go through its normal tracking/undo environment.
local function finite(n, name)
  if type(n) ~= "number" or n ~= n or n == math.huge or n == -math.huge then
    error(name .. " must be a finite number")
  end
end

local function pose(angle, x, z)
  finite(angle, "angle")
  finite(x, "x")
  finite(z, "z")
  local r = angle * pi / 180
  return cos(r), sin(r), floor(x + 0.5) % 1024, floor(z + 0.5) % 1024
end

-- Degrees, visual magnification (2 = twice as large), texture center.
function mode7_transform(angle, zoom, x, z)
  finite(zoom, "zoom")
  if zoom < 1 / 127 then error("zoom must be at least 1/127") end
  local c, s, cx, cy = pose(angle, x, z)
  return {
    a = c / zoom, b = -s / zoom, c = s / zoom, d = c / zoom,
    cx = cx, cy = cy, scroll_x = cx - 128, scroll_y = cy - 112,
    visible = true,
  }
end

-- A level camera with focal length 128 pixels. Positions wrap over the
-- 1024x1024 plane; rows at/above the horizon expose the existing backdrop.
function mode7_floor(y, horizon, height, heading, x, z)
  finite(y, "scanline")
  finite(horizon, "horizon")
  finite(height, "height")
  if height <= 0 then error("height must be positive") end
  local c, s = pose(heading, x, z)
  -- The signed Q8.8 matrix tops out below 128.
  local scale = min(127, height / max(1, y - horizon))
  local depth = 128 * scale
  local cx = floor(x - s * depth + 0.5) % 1024
  local cy = floor(z + c * depth + 0.5) % 1024
  return {
    a = c * scale, b = 0, c = s * scale, d = 0,
    cx = cx, cy = cy, scroll_x = cx - 128, scroll_y = 0,
    visible = y > horizon,
  }
end
