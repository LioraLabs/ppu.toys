-- Built-in keyframe interpolation, available before any sketch file runs.
-- Keys are {position, value, optional easing}, sorted by position.
-- Positions can be scanlines, seconds, or any other numeric coordinate.
function clamp(value, lo, hi)
  if lo > hi then error("clamp: minimum must not exceed maximum") end
  return max(lo, min(hi, value))
end

function lerp(a, b, t) return a + (b - a) * t end

-- Colors hold at the endpoints rather than overflowing their 5-bit channels.
function lerp_color(a, b, t)
  t = clamp(t, 0, 1)
  local result = 0
  for c = 0, 2 do
    local scale = 32 ^ c
    local av, bv = floor(a / scale) % 32, floor(b / scale) % 32
    result = result + floor(lerp(av, bv, t) + 0.5) * scale
  end
  return result
end

function ease(t, easing)
  if easing == "smooth" then return t * t * (3 - 2 * t) end
  if easing == "ease-in" then return t * t end
  if easing == "ease-out" then return t * (2 - t) end
  if easing == "step" then return t < 1 and 0 or 1 end
  return t
end
function ramp(kf, y)
  local n = #kf
  if n == 0 then error("ramp: at least one keyframe is required") end
  if y <= kf[1][1] then return kf[1][2] end
  for i = 2, n do
    local a, b = kf[i - 1], kf[i]
    if y <= b[1] then
      local d = b[1] - a[1]
      if d <= 0 then return b[2] end
      return lerp(a[2], b[2], ease((y - a[1]) / d, a[3]))
    end
  end
  return kf[n][2]
end
function ramp_int(kf, y) return floor(ramp(kf, y) + 0.5) end

function ramp_color(kf, y)
  if #kf == 0 then error("ramp_color: at least one keyframe is required") end
  local a, b = kf[1], kf[1]
  for i = 2, #kf do
    if y <= b[1] then break end
    a, b = b, kf[i]
  end
  local t = 0
  if b[1] > a[1] then t = clamp((y - a[1]) / (b[1] - a[1]), 0, 1) end
  t = ease(t, a[3])
  return lerp_color(a[2], b[2], t)
end
