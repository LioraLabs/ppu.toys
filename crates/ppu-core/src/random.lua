-- math.random's generator: a 16-bit xorshift (shifts 7, 9, 8; period 65535), so the
-- preview and a cartridge draw the same numbers from the same seed. The state is
-- never 0; seeding keeps its low 16 bits.
---@snes range __ppu_random_state 0 65535
__ppu_random_state = 0xACE1
function __ppu_random_next()
  ---@snes range x 0 65535
  local x = __ppu_random_state
  -- Masking before shifting keeps every step in 16 bits.
  x = x ~ ((x & 0x1FF) << 7)
  x = x ~ (x >> 9)
  x = x ~ ((x & 0xFF) << 8)
  __ppu_random_state = x
  return x
end

-- An integer from lo to hi, both included (hi - lo below 65535).
function __ppu_random(lo, hi)
  return lo + __ppu_random_next() % (hi - lo + 1)
end

-- Seed 0 (no low bits) picks the default seed. One step mixes a small seed.
function __ppu_randomseed(seed)
  local x = seed & 0xFFFF
  if x == 0 then x = 0xACE1 end
  __ppu_random_state = x
  __ppu_random_next()
end
