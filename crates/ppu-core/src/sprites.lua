-- Managed sprites (see sprites.rs for the static plan). sprites.draw records a sprite; after each
-- frame() the engine calls __ppu_sprites_flush, which assigns OAM slots to the frame's sprites in
-- draw order, or, in a frame that called sprites.sort(), ordered by layer (lower on top), then depth
-- (nearer, lower depth, on top), then draw order: a lower slot draws on top. A frame keeps its first
-- RECORDS sprites on screen. Slots the program indexes through obj[...] are its own; the rest are
-- the library's, and the ones not drawn this frame are hidden. Cells are 16x16 large sprites: keep
-- obj.size_sel at 0 (8x8 small, 16x16 large).
--
-- Keyed sprites (sprites.draw(art, x, y, {key = k})): key k, an integer 0..127, owns a block of the
-- library's slots, as many as the largest level of the art it was first drawn with. Blocks are taken
-- from the last free slots down, in the order keys are first drawn, and the first frame's sprites
-- show at that frame's end; from the next frame on, unkeyed sprites use only the slots no block owns.
-- Cell i of a keyed draw always takes slot i of its block (a cell off screen, or past the level's
-- cells, hides its slot). A frame draws or keeps (sprites.keep(k): leave its sprites as they are) a
-- key at most once; a key neither drawn nor kept is hidden.
sprites = {flicker = false}

local ARTS = {}
local records, rotation = {}, 0
-- Every kept sprite shows a cell, so this many fill the 128 slots; later draws are dropped.
local RECORDS = 128
-- sprites.sort() orders this frame's sprites by layer and depth.
local sorted = false
-- sprites.hold keeps last frame's managed sprites; a frame either holds or draws.
local held, drawn = false, false
local CONFLICT = "sprites.draw and sprites.hold cannot both run in one frame"
-- Keys: blocks[k] = {first = free position, size = slots}; touched[k] = "draw" or "keep" this frame.
local blocks, touched = {}, {}
-- This frame's keyed draws (written at its end, after the unkeyed sprites), and the free slots blocks own.
local keyed, owned = {}, 0
-- Free slots the unkeyed sprites take this frame: those no block owned when it began.
local unkeyed = nil
-- Flicker advances the slot rotation by this many sprites a frame, so the sprites a crowded
-- scanline drops differ from frame to frame.
local STRIDE = 23

local function integer(v, lo, hi)
  return type(v) == "number" and v == floor(v) and v >= lo and v <= hi
end

function sprites.art(levels, sheet)
  assert(type(sheet) == "table" and integer(sheet.tile, 0, 511) and integer(sheet.pal, 0, 7)
    and type(sheet.cells) == "table", "sprites.art needs a sprite sheet from dma()")
  assert(type(levels) == "table" and #levels >= 1 and #levels <= 16, "sprites.art needs 1..16 levels")
  local art = {pal = sheet.pal}
  for i, level in ipairs(levels) do
    local columns, cells = level.columns, level.cells
    assert(integer(level.size, 0, 1024), "sprites.art level size must be an integer 0..1024")
    assert(type(cells) == "table" and #cells >= 1 and #cells <= 64, "sprites.art level needs 1..64 cells")
    assert(integer(columns, 1, 16) and #cells % columns == 0, "sprites.art level cells must fill its columns")
    local rows = #cells // columns
    local out = {size = level.size, x0 = -columns * 8, y0 = -rows * 8,
      x1 = columns * 8 - 16, y1 = rows * 8 - 16}
    for index, cell in ipairs(cells) do
      local entry = integer(cell, 0, 1023) and sheet.cells[cell + 1]
      assert(entry, "sprites.art cell " .. tostring(cell) .. " is not in the sheet")
      local tile = sheet.tile + entry.tile
      assert(integer(tile, 0, 511), "sprites.art cell tile must be 0..511")
      out[index] = {dx = out.x0 + (index - 1) % columns * 16, dy = out.y0 + (index - 1) // columns * 16, tile = tile}
    end
    art[i] = out
  end
  ARTS[art] = true
  return art
end

function sprites.sort()
  sorted = true
end

function sprites.hold()
  if drawn then error(CONFLICT, 2) end
  held = true
end

local function key_of(k, call)
  if not integer(k, 0, 127) then error(call .. " key must be an integer 0..127", 3) end
  local was = touched[k]
  if was == "draw" and call == "sprites.draw" then error("sprites key " .. k .. " is drawn twice in one frame", 3) end
  if was == "keep" and call == "sprites.keep" then error("sprites key " .. k .. " is kept twice in one frame", 3) end
  if was ~= nil then error("sprites key " .. k .. " is both drawn and kept in one frame", 3) end
  return k
end

function sprites.keep(k)
  key_of(k, "sprites.keep")
  if blocks[k] == nil then error("sprites.keep(" .. k .. "): the key was never drawn", 2) end
  touched[k] = "keep"
end

function sprites.draw(art, x, y, options)
  if held then error(CONFLICT, 2) end
  drawn = true
  assert(ARTS[art], "sprites.draw needs art made by sprites.art")
  assert(type(x) == "number" and type(y) == "number" and x == x and y == y, "sprites.draw position must be numbers")
  local layer, depth, size, prio, key = 0, 0, nil, 3, nil
  if options ~= nil then
    layer, depth, size, prio, key = options.layer or 0, options.depth or 0, options.size, options.prio or 3, options.key
  end
  if key ~= nil then
    key_of(key, "sprites.draw")
    local need = 0
    for _, level in ipairs(art) do need = max(need, #level) end
    local block = blocks[key]
    if block == nil then
      local left = #__ppu_sprites_free - owned
      if need > left then
        error("sprites key " .. key .. " needs " .. need .. " slots; " .. left .. " are left to take", 2)
      end
      owned = owned + need
      block = {first = #__ppu_sprites_free - owned + 1, size = need}
      blocks[key] = block
    elseif need > block.size then
      error("sprites key " .. key .. " has " .. block.size .. " slots, from its first art; this art needs " .. need, 2)
    end
    touched[key] = "draw"
  end
  assert(integer(layer, 0, 7), "sprites.draw layer must be an integer 0..7")
  assert(type(depth) == "number" and depth >= -32768 and depth < 32768, "sprites.draw depth must be a number in -32768..32767")
  assert(size == nil or (type(size) == "number" and size == size), "sprites.draw size must be a number")
  assert(integer(prio, 0, 3), "sprites.draw prio must be an integer 0..3")
  -- The first level at least as large as size, else the largest.
  local level = art[#art]
  if size ~= nil then
    for _, candidate in ipairs(art) do
      if size <= candidate.size then level = candidate; break end
    end
  end
  x, y, depth = floor(x), floor(y), floor(depth)
  if key ~= nil then
    keyed[#keyed + 1] = {block = blocks[key], level = level, x = x, y = y, prio = prio, pal = art.pal}
    return
  end
  -- Whole art off screen: no record. Cells tile the box, so a kept record shows a cell.
  if x + level.x1 <= -16 or x + level.x0 >= 256 or y + level.y1 <= -16 or y + level.y0 >= 224 then return end
  if #records == RECORDS then return end
  records[#records + 1] = {level = level, x = x, y = y, layer = layer, depth = depth, prio = prio,
    pal = art.pal}
end

function __ppu_sprites_flush()
  local sort, was = sorted, touched
  sorted, touched = false, {}
  if held then
    held, drawn = false, false
    return
  end
  drawn = false
  local free = __ppu_sprites_free
  local slots = unkeyed or #free
  -- Stable insertion sort: equal keys keep draw order.
  for i = 2, sort and #records or 0 do
    local r, j = records[i], i - 1
    while j >= 1 and (records[j].layer > r.layer or (records[j].layer == r.layer and records[j].depth > r.depth)) do
      records[j + 1] = records[j]
      j = j - 1
    end
    records[j + 1] = r
  end
  local cells = {}
  for _, r in ipairs(records) do
    for _, cell in ipairs(r.level) do
      local x, y = r.x + cell.dx, r.y + cell.dy
      if #cells < slots and x > -16 and x < 256 and y > -16 and y < 224 then
        cells[#cells + 1] = {x = x, y = y % 256, tile = cell.tile, pal = r.pal, prio = r.prio}
      end
    end
  end
  records = {}
  local count = #cells
  if slots > 0 then
    -- Item i takes position (start + i) of the unkeyed slots, wrapping: a rotation of the drawn order.
    local start = (slots - rotation) % slots
    for i, c in ipairs(cells) do
      local sprite = obj[free[(start + i - 1) % slots + 1]]
      sprite.x, sprite.y, sprite.tile, sprite.pal, sprite.prio = c.x, c.y, c.tile, c.pal, c.prio
      sprite.large, sprite.flip_x, sprite.flip_y, sprite.on = true, false, false, true
    end
    for k = count, slots - 1 do
      obj[free[(start + k) % slots + 1]].on = false
    end
  end
  if sprites.flicker == true and count > 0 then
    rotation = (rotation + STRIDE) % count
  else
    rotation = 0
  end
  -- Blocks neither drawn nor kept hide; then this frame's keyed draws, each cell in its slot of the block.
  for k, block in pairs(blocks) do
    if not was[k] then
      for i = 0, block.size - 1 do obj[free[block.first + i]].on = false end
    end
  end
  for _, d in ipairs(keyed) do
    for i = 1, d.block.size do
      local sprite, cell = obj[free[d.block.first + i - 1]], d.level[i]
      local x, y = cell and d.x + cell.dx, cell and d.y + cell.dy
      if cell and x > -16 and x < 256 and y > -16 and y < 224 then
        sprite.x, sprite.y, sprite.tile, sprite.pal, sprite.prio = x, y % 256, cell.tile, d.pal, d.prio
        sprite.large, sprite.flip_x, sprite.flip_y, sprite.on = true, false, false, true
      else
        sprite.on = false
      end
    end
  end
  keyed = {}
  -- Blocks first drawn this frame leave the unkeyed sprites fewer slots from the next frame on.
  if #free - owned < slots then
    slots = #free - owned
    if slots > 0 then rotation = rotation % slots end
  end
  unkeyed = slots
end
