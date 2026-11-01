-- Managed sprites in groups (see sprites.rs for the static plan). Groups draw in the order they are
-- made (sprites.group calls in source order), the first on top, then the default group of plain
-- sprites.draw; each owns a run of the free slots (those the program never indexes through obj[...]),
-- as many as its draws can fill in a frame. A lower slot draws on top.
--
-- Sizes: a level's cells are its sheet's (dma() cell_w x cell_h). Once the file-level code has run,
-- __ppu_sprites_finish picks the OBJSEL size pair (obj.size_sel): the program's own constant, else
-- the lowest-numbered pair that holds every hardware-sized cell (8, 16, 32, 64 square, 16x32,
-- 32x64) and builds the other cells from the fewest sprites. Each art level becomes pieces, one
-- hardware sprite each (a cell of another size is several, its blank ones left out), placed
-- around the drawn point; a flipped draw mirrors the pieces' positions and flips each piece.
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
-- The most pieces each art shows, by its sprites.art call (the plan numbers them; the engine rewrites
-- each call to pass its number).
__ppu_sprites_cells = {}
__ppu_sprites_arts = {}
-- OBJSEL size pairs by obj.size_sel: small width, height, large width, height.
local PAIRS = {[0] = {8, 8, 16, 16}, {8, 8, 32, 32}, {8, 8, 64, 64}, {16, 16, 32, 32}, {16, 16, 64, 64},
  {32, 32, 64, 64}, {16, 32, 32, 64}, {16, 32, 32, 32}}
-- The pair the art was built for, once __ppu_sprites_finish has run.
local pair = nil
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

local function sheet_ok(sheet)
  assert(type(sheet) == "table" and integer(sheet.tile, 0, 511) and integer(sheet.pal, 0, 7)
    and type(sheet.cells) == "table", "sprites.art needs a sprite sheet from dma()")
end

-- A cell w x h as pieces of the pair (small sw x sh, large lw x lh): {x, y, large} from the cell's corner,
-- the large ones in a grid from the corner, then small ones beside and below it, leaving out pieces whose
-- tiles are all blank (opaque: per tile row, a bit per drawn tile). nil when the pair cannot build it.
local function pieces(w, h, opaque, P)
  local sw, sh, lw, lh = P[1], P[2], P[3], P[4]
  if w == lw and h == lh then return {{0, 0, true}} end
  if w == sw and h == sh then return {{0, 0, false}} end
  if w % sw ~= 0 or h % sh ~= 0 then return nil end
  local out = {}
  local function add(x, y, large, pw, ph)
    local shows = opaque == nil
    for ty = y // 8, opaque and (y + ph) // 8 - 1 or -1 do
      for tx = x // 8, (x + pw) // 8 - 1 do
        if (opaque[ty + 1] or 0) // 2 ^ tx % 2 == 1 then shows = true end
      end
    end
    if shows then out[#out + 1] = {x, y, large} end
  end
  local nx, ny = w // lw, h // lh
  for j = 0, ny - 1 do
    for i = 0, nx - 1 do add(i * lw, j * lh, true, lw, lh) end
  end
  for y = 0, ny * lh - 1, sh do
    for x = nx * lw, w - 1, sw do add(x, y, false, sw, sh) end
  end
  for y = ny * lh, h - 1, sh do
    for x = 0, w - 1, sw do add(x, y, false, sw, sh) end
  end
  return out
end

-- Where a piece w x h shows: x in (-w, 256), y in (ylo, yhi). A 64-pixel-tall sprite wraps from the
-- bottom of the 256-line space to its top, so it shows only where it cannot wrap onto the screen.
local function window(h)
  if h > 32 then return -33, 257 - h end
  return -h, 224
end

-- A level's pieces flipped (fx, fy), each mirrored around the drawn point, with the level's boxes: an
-- entry at (x, y) shows a piece only when xlo < x < xhi and ylo < y < yhi.
local function variant(level, fx, fy)
  local out = {size = level.size, xlo = 32767, xhi = -32768, ylo = 32767, yhi = -32768}
  for i, p in ipairs(level.pieces) do
    local q = {dx = p.dx, dy = p.dy, w = p.w, h = p.h, tile = p.tile, pal = p.pal, large = p.large,
      flip_x = p.flip_x ~= fx, flip_y = p.flip_y ~= fy}
    if fx then q.dx = -p.dx - p.w end
    if fy then q.dy = -p.dy - p.h end
    q.ylo, q.yhi = window(q.h)
    out[i] = q
    out.xlo, out.xhi = min(out.xlo, -q.w - q.dx), max(out.xhi, 256 - q.dx)
    out.ylo, out.yhi = min(out.ylo, q.ylo - q.dy), max(out.yhi, q.yhi - q.dy)
  end
  return out
end

-- Build an art's levels for the pair P: each level's pieces and its four flips.
local function build(art, P)
  art.cells = 0
  for i, level in ipairs(art.levels) do
    local sheet, w, h = level.sheet, level.w, level.h
    local columns = level.columns
    local x0, y0 = -columns * w // 2, -(#level.cells // columns) * h // 2
    local out = {}
    for index, cell in ipairs(level.cells) do
      local entry = sheet.cells[cell + 1]
      local cx, cy = x0 + (index - 1) % columns * w, y0 + (index - 1) // columns * h
      for _, p in ipairs(pieces(w, h, entry.opaque, P)) do
        local pw, ph = P[p[3] and 3 or 1], P[p[3] and 4 or 2]
        local tile = sheet.tile + entry.tile + p[2] // 8 * 16 + p[1] // 8
        local pal = sheet.pal + (entry.pal or 0)
        assert(integer(tile, 0, 511), "sprites.art cell tile must be 0..511")
        -- A sprite's 8x8 tiles run right within a 16-tile row of the name table, wrapping at its end.
        assert(tile % 16 + pw // 8 <= 16, "sprites.art cell " .. cell .. " runs past the end of a 16-tile row of OBJ tiles: "
          .. "place its sheet at a char address that starts a row (a multiple of 0x100 words from obj.char_base)")
        assert(integer(pal, 0, 7), "sprites.art cell palette must be 0..7")
        out[#out + 1] = {dx = cx + p[1], dy = cy + p[2], w = pw, h = ph, tile = tile, pal = pal, large = p[3],
          flip_x = entry.flip_x == true, flip_y = entry.flip_y == true}
      end
    end
    assert(#out >= 1, "sprites.art level " .. i .. " draws nothing: every tile of its cells is blank")
    level.pieces = out
    -- By flips: [0] none, [1] x, [2] y, [3] both.
    art[i] = {[0] = variant(level, false, false), variant(level, true, false), variant(level, false, true),
      variant(level, true, true), size = level.size}
    art.cells = max(art.cells, #out)
  end
end

function sprites.art(levels, sheet)
  sheet_ok(sheet)
  assert(type(levels) == "table" and #levels >= 1 and #levels <= 16, "sprites.art needs 1..16 levels")
  local art = {levels = {}, cells = 0}
  for i, level in ipairs(levels) do
    local columns, cells, own = level.columns, level.cells, level.sheet or sheet
    if level.sheet ~= nil then sheet_ok(level.sheet) end
    assert(integer(level.size, 0, 1024), "sprites.art level size must be an integer 0..1024")
    assert(type(cells) == "table" and #cells >= 1 and #cells <= 64, "sprites.art level needs 1..64 cells")
    assert(integer(columns, 1, 16) and #cells % columns == 0, "sprites.art level cells must fill its columns")
    for _, cell in ipairs(cells) do
      assert(integer(cell, 0, 1023) and own.cells[cell + 1], "sprites.art cell " .. tostring(cell) .. " is not in the sheet")
    end
    local w = own.cell_w or own.cell_size or 16
    art.levels[i] = {size = level.size, columns = columns, cells = cells, sheet = own, w = w, h = own.cell_h or w}
  end
  ARTS[art] = true
  if pair ~= nil then build(art, PAIRS[pair]) end
  return art
end

function __ppu_sprites_art(site, levels, sheet)
  local art = sprites.art(levels, sheet)
  __ppu_sprites_arts[site] = art
  return art
end

local function hardware(w, h)
  for p = 0, 7 do
    local P = PAIRS[p]
    if (w == P[1] and h == P[2]) or (w == P[3] and h == P[4]) then return true end
  end
  return false
end

local function holds(P, w, h)
  return (w == P[1] and h == P[2]) or (w == P[3] and h == P[4])
end

-- After the file-level code: the size pair (`pin`, the program's own obj.size_sel, or chosen), every
-- art built for it and its piece count in __ppu_sprites_cells. When no pair fits, the art number and
-- why in __ppu_sprites_fail_site and __ppu_sprites_fail.
function __ppu_sprites_finish(pin, arts)
  local sizes, order, at, names = {}, {}, {}, ""
  for site = 1, arts do
    local art = __ppu_sprites_arts[site]
    for _, level in ipairs(art and art.levels or {}) do
      local k = level.w .. "x" .. level.h
      if hardware(level.w, level.h) and not sizes[k] then
        sizes[k], order[#order + 1], at[#order + 1] = {level.w, level.h}, k, site
        names = names == "" and k or names .. ", " .. k
      end
    end
  end
  local candidates = {}
  for p = 0, 7 do
    local fits = pin == nil or p == pin
    for _, k in ipairs(order) do
      if pin == nil and not holds(PAIRS[p], sizes[k][1], sizes[k][2]) then fits = false end
    end
    if fits then candidates[#candidates + 1] = p end
  end
  if #candidates == 0 then
    __ppu_sprites_fail_site, __ppu_sprites_fail = at[min(#order, 3)], "sprites art uses " .. names .. " cells, and an obj.size_sel pair has two sizes"
    return
  end
  local best, cost, fail = nil, nil, nil
  for _, p in ipairs(candidates) do
    local total = 0
    for site = 1, arts do
      local art = __ppu_sprites_arts[site]
      for _, level in ipairs(art and art.levels or {}) do
        for _, cell in ipairs(level.cells) do
          local n = pieces(level.w, level.h, level.sheet.cells[cell + 1].opaque, PAIRS[p])
          if n == nil then
            total = nil
            fail = fail or {site, level.w .. "x" .. level.h}
            break
          end
          total = total + #n
        end
        if total == nil then break end
      end
      if total == nil then break end
    end
    if total ~= nil and (cost == nil or total < cost) then best, cost = p, total end
  end
  if best == nil then
    if fail == nil then return end
    __ppu_sprites_fail_site, __ppu_sprites_fail = fail[1], "sprites art cells of " .. fail[2] .. " can't be built from obj.size_sel " .. candidates[1] .. "'s sprites"
    return
  end
  pair = best
  for site = 1, arts do
    local art = __ppu_sprites_arts[site]
    if art then
      build(art, PAIRS[best])
      __ppu_sprites_cells[site] = art.cells
    end
  end
  __ppu_sprites_size_sel = best
  if pin == nil then obj.size_sel = best end
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

-- An entry of the frame: none when the whole art is off screen (outside its level's boxes); a group
-- keeps as many entries a frame as it has slots, and later ones are dropped.
local function record(s, r)
  local level, x, y = r.level, r.x, r.y
  if x <= level.xlo or x >= level.xhi or y <= level.ylo or y >= level.yhi then return end
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
  local depth, size, prio, key, fx, fy, pal = 0, nil, 3, nil, false, false, nil
  if options ~= nil then
    depth, size, prio, key = options.depth or 0, options.size, options.prio or 3, options.key
    fx, fy, pal = options.flip_x or false, options.flip_y or false, options.pal
  end
  assert(type(depth) == "number" and depth >= -32768 and depth < 32768, "sprites.draw depth must be a number in -32768..32767")
  assert(size == nil or (type(size) == "number" and size == size), "sprites.draw size must be a number")
  assert(integer(prio, 0, 3), "sprites.draw prio must be an integer 0..3")
  assert(type(fx) == "boolean" and type(fy) == "boolean", "sprites.draw flip_x and flip_y must be booleans")
  assert(pal == nil or integer(pal, 0, 7), "sprites.draw pal must be an integer 0..7")
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
  level = level[(fx and 1 or 0) + (fy and 2 or 0)]
  local r = {level = level, x = x, y = y, depth = depth, prio = prio, pal = pal}
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

-- Whether piece p of an entry at (x, y) shows: then the sprite takes it.
local function shows(p, x, y)
  return x > -p.w and x < 256 and y > p.ylo and y < p.yhi
end

local function put(sprite, p, r, x, y)
  sprite.x, sprite.y, sprite.tile, sprite.pal, sprite.prio = x, y % 256, p.tile, r.pal or p.pal, r.prio
  sprite.large, sprite.flip_x, sprite.flip_y, sprite.on = p.large, p.flip_x, p.flip_y, true
end

-- The pieces of `entries` in order into free positions first .. first + slots - 1 (from 1), item i at
-- (start + i), wrapping (a rotation); the rest of those slots hidden. Returns how many pieces showed.
local function place(free, entries, first, slots, rotation)
  local cells = {}
  for _, r in ipairs(entries) do
    for _, p in ipairs(r.level) do
      local x, y = r.x + p.dx, r.y + p.dy
      if #cells < slots and shows(p, x, y) then
        cells[#cells + 1] = {p = p, r = r, x = x, y = y}
      end
    end
  end
  if slots == 0 then return 0 end
  local start = (slots - rotation) % slots
  for i, c in ipairs(cells) do
    put(obj[free[first + (start + i - 1) % slots]], c.p, c.r, c.x, c.y)
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
          local sprite, p = obj[free[d.block.first + i]], r.level[i]
          local x, y = p and r.x + p.dx, p and r.y + p.dy
          if p and shows(p, x, y) then
            put(sprite, p, r, x, y)
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
  -- No plan: the program never draws, so the group shows nothing (as on the cartridge).
  if __ppu_sprites_free == nil then return {} end
  error("sprites.group must be called at file level or in init(), where the plan numbers it", 2)
end
