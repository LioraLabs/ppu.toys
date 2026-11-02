--!strict
-- @ppu/blink: on/off timing for flashing text and invulnerable sprites.
-- local blink = require("@ppu/blink"); if blink.on(f, 8) then ... end

-- Whether frame f is in the "on" half of a blink: on for `half` frames, then off for `half`.
local function on(f: number, half: number): boolean
  return (f // half) % 2 == 0
end

return {on = on}
