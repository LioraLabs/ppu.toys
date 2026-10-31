-- Managed sprites in groups (see sprites.rs for the static plan). Groups draw in the order they are
-- made (sprites.group calls in source order), the first on top, then the default group of plain
-- sprites.draw; each owns a run of the free slots (those the program never indexes through obj[...]),
-- as many as its draws can fill in a frame. A lower slot draws on top. Cells are 16x16 large sprites:
-- keep obj.size_sel at 0 (8x8 small, 16x16 large).
--
-- In a group, a keyed draw (draw(art, x, y, {key = k}), k an integer 0..127, keys local to the group)
-- owns a block of the group's slots, as many as the largest level of the art it was first drawn with,
-- taken from the group's first slots up in the order keys are first drawn; the first frame's sprites
-- show at that frame's end. Cell i of a keyed draw always takes slot i of its block (a cell off
-- screen, or past the level's cells, hides its slot). keep(k) leaves the block as it is; a key a frame
-- neither draws nor keeps is hidden. Unkeyed draws fill the group's slots after the blocks in call
-- order (rotated by sprites.flicker), and the frame's last ones are dropped when they do not fit.
-- A sorted group (sprites.group({sort = true})) has no blocks: each frame its entries, drawn or kept,
-- take its slots in order of depth (nearer, lower depth, on top), then of call.
sprites = {flicker = false}

local ARTS = {}
-- The most cells each art shows, by its sprites.art call (the plan numbers them; the engine rewrites
-- each call to pass its number).
__ppu_sprites_cells = {}
__ppu_sprites_arts = {}
local held, drawn = false, false
local CONFLICT = "sprites.draw and sprites.hold cannot both run in one frame"
-- Flicker advances a group's rotation by this many sprites a frame, so the sprites a crowded scanline
-- drops differ from frame to frame.
local STRIDE = 23
-- Per group number: its state, made on first use from the engine's layout.
local groups = {}

local function integer(v, lo, hi)
  return type(v) == "number" and v == floor(v) and v >= lo and v <= hi
end

function sprites.art(levels, sheet)
  assert(type(sheet) == "table" and integer(sheet.tile, 0, 511) and integer(sheet.pal, 0, 7)
    and type(sheet.cells) == "table", "sprites.art needs a sprite sheet from dma()")
  assert(type(levels) == "table" and #levels >= 1 and #levels <= 16, "sprites.art needs 1..16 levels")
  local art = {pal = sheet.pal, cells = 0}
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
    art.cells = max(art.cells, #cells)
  end
  ARTS[art] = true
  return art
end

function __ppu_sprites_art(site, levels, sheet)
  local art = sprites.art(levels, sheet)
  __ppu_sprites_cells[site], __ppu_sprites_arts[site] = art.cells, art
  return art
end

local function group(g)
  local s = groups[g]
  if s == nil then
    local l = __ppu_sprites_layout[g]
    s = {first = l.first, size = l.size, sort = l.sort, blocks = {}, owned = 0, unkeyed = l.size,
      touched = {}, keyed = {}, records = {}, rotation = 0, last = {}}
    groups[g] = s
  end
  return s
end

local function touch(s, k, how)
  if not integer(k, 0, 127) then error(how .. " key must be an integer 0..127", 4) end
  local was = s.touched[k]
  if was == "draw" and how == "draw" then error("sprites key " .. k .. " is drawn twice in one frame", 4) end
  if was == "keep" and how == "keep" then error("sprites key " .. k .. " is kept twice in one frame", 4) end
  if was ~= nil then error("sprites key " .. k .. " is both drawn and kept in one frame", 4) end
  s.touched[k] = how
end

-- An entry of the frame: none when the whole art is off screen (cells tile the box, so an entry shows
-- a cell); a group keeps as many entries a frame as it has slots, and later ones are dropped.
local function record(s, r)
  local level, x, y = r.level, r.x, r.y
  if x + level.x1 <= -16 or x + level.x0 >= 256 or y + level.y1 <= -16 or y + level.y0 >= 224 then return end
  if #s.records == s.size then return end
  s.records[#s.records + 1] = r
end

local function keep(g, k)
  local s = group(g)
  touch(s, k, "keep")
  if s.blocks[k] == nil and s.last[k] == nil then
    error("keep(" .. k .. "): the key was never drawn in this group", 3)
  end
  -- A sorted group places a kept entry again, where its depth puts it.
  if s.sort then record(s, s.last[k]) end
end

local function draw(g, art, x, y, options)
  if held then error(CONFLICT, 3) end
  drawn = true
  assert(ARTS[art], "sprites.draw needs art made by sprites.art")
  assert(type(x) == "number" and type(y) == "number" and x == x and y == y, "sprites.draw position must be numbers")
  local depth, size, prio, key = 0, nil, 3, nil
  if options ~= nil then
    depth, size, prio, key = options.depth or 0, options.size, options.prio or 3, options.key
  end
  assert(type(depth) == "number" and depth >= -32768 and depth < 32768, "sprites.draw depth must be a number in -32768..32767")
  assert(size == nil or (type(size) == "number" and size == size), "sprites.draw size must be a number")
  assert(integer(prio, 0, 3), "sprites.draw prio must be an integer 0..3")
  local s = group(g)
  if key ~= nil then
    touch(s, key, "draw")
    if not s.sort then
      local block = s.blocks[key]
      if block == nil then
        local left = s.size - s.owned
        if art.cells > left then
          error("sprites key " .. key .. " needs " .. art.cells .. " slots; " .. left .. " are left in its group", 3)
        end
        block = {first = s.first + s.owned, size = art.cells}
        s.owned = s.owned + art.cells
        s.blocks[key] = block
      elseif art.cells > block.size then
        error("sprites key " .. key .. " has " .. block.size .. " slots, from its first art; this art needs " .. art.cells, 3)
      end
    end
  end
  -- The first level at least as large as size, else the largest.
  local level = art[#art]
  if size ~= nil then
    for _, candidate in ipairs(art) do
      if size <= candidate.size then level = candidate; break end
    end
  end
  x, y, depth = floor(x), floor(y), floor(depth)
  local r = {level = level, x = x, y = y, depth = depth, prio = prio, pal = art.pal}
  if key ~= nil and not s.sort then
    s.keyed[#s.keyed + 1] = {block = s.blocks[key], r = r}
    return
  end
  if key ~= nil then s.last[key] = r end
  record(s, r)
end

-- A group handle; the engine rewrites each sprites.group call to pass the group's number.
function __ppu_sprites_group(g, options)
  return {
    draw = function(_, art, x, y, o) return draw(g, art, x, y, o) end,
    keep = function(_, k) return keep(g, k) end,
  }
end

function sprites.draw(art, x, y, options)
  return draw(#__ppu_sprites_layout, art, x, y, options)
end

function sprites.keep(k)
  return keep(#__ppu_sprites_layout, k)
end

function sprites.hold()
  if drawn then error(CONFLICT, 2) end
  held = true
end

-- The cells of `entries` in order into free positions first .. first + slots - 1 (from 1), item i at
-- (start + i), wrapping (a rotation); the rest of those slots hidden. Returns how many cells showed.
local function place(free, entries, first, slots, rotation)
  local cells = {}
  for _, r in ipairs(entries) do
    for _, cell in ipairs(r.level) do
      local x, y = r.x + cell.dx, r.y + cell.dy
      if #cells < slots and x > -16 and x < 256 and y > -16 and y < 224 then
        cells[#cells + 1] = {x = x, y = y % 256, tile = cell.tile, pal = r.pal, prio = r.prio}
      end
    end
  end
  if slots == 0 then return 0 end
  local start = (slots - rotation) % slots
  for i, c in ipairs(cells) do
    local sprite = obj[free[first + (start + i - 1) % slots]]
    sprite.x, sprite.y, sprite.tile, sprite.pal, sprite.prio = c.x, c.y, c.tile, c.pal, c.prio
    sprite.large, sprite.flip_x, sprite.flip_y, sprite.on = true, false, false, true
  end
  for k = #cells, slots - 1 do
    obj[free[first + (start + k) % slots]].on = false
  end
  return #cells
end

function __ppu_sprites_flush()
  local was = held
  held, drawn = false, false
  local free = __ppu_sprites_free
  for g = 1, #__ppu_sprites_layout do
    local s = group(g)
    local touched = s.touched
    s.touched = {}
    if was then
      -- A held frame keeps every group as it was.
    elseif s.sort then
      -- Stable insertion sort by depth: equal depths keep call order.
      local records = s.records
      for i = 2, #records do
        local r, j = records[i], i - 1
        while j >= 1 and records[j].depth > r.depth do
          records[j + 1] = records[j]
          j = j - 1
        end
        records[j + 1] = r
      end
      place(free, records, s.first + 1, s.size, 0)
    else
      local slots = s.unkeyed
      local count = place(free, s.records, s.first + 1 + s.size - slots, slots, s.rotation)
      if sprites.flicker == true and count > 0 then
        s.rotation = (s.rotation + STRIDE) % count
      else
        s.rotation = 0
      end
      -- Blocks neither drawn nor kept hide; then this frame's keyed draws, each cell in its slot.
      for k, block in pairs(s.blocks) do
        if not touched[k] then
          for i = 1, block.size do obj[free[block.first + i]].on = false end
        end
      end
      for _, d in ipairs(s.keyed) do
        local r = d.r
        for i = 1, d.block.size do
          local sprite, cell = obj[free[d.block.first + i]], r.level[i]
          local x, y = cell and r.x + cell.dx, cell and r.y + cell.dy
          if cell and x > -16 and x < 256 and y > -16 and y < 224 then
            sprite.x, sprite.y, sprite.tile, sprite.pal, sprite.prio = x, y % 256, cell.tile, r.pal, r.prio
            sprite.large, sprite.flip_x, sprite.flip_y, sprite.on = true, false, false, true
          else
            sprite.on = false
          end
        end
      end
      -- Blocks first drawn this frame leave the unkeyed sprites fewer slots from the next frame on.
      if s.size - s.owned < slots then
        s.unkeyed = s.size - s.owned
        if s.unkeyed > 0 then s.rotation = s.rotation % s.unkeyed end
      end
    end
    s.records, s.keyed = {}, {}
  end
end

function sprites.group()
  error("sprites.group must be called at file level or in init(), where the plan numbers it", 2)
end
