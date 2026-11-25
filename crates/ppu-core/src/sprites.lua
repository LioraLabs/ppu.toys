-- Managed sprites in groups (see sprites.rs for the static plan). Groups draw in the order they are
-- made (sprites.group calls in source order), the first on top, then the default group of plain
-- sprites.draw; each owns a run of the free slots (those the program never indexes through obj[...]),
-- as many as its draws can fill in a frame. A lower slot draws on top.
--
-- Sizes: a level's cells are its sheet's (dma() cell_w x cell_h). Once the file-level code has run,
-- __ppu_sprites_finish picks the OBJSEL size pair (obj.size_sel): the program's own constant, else
-- of the pairs that hold every hardware-sized cell (8, 16, 32, 64 square, 16x32, 32x64) the one that
-- builds the other cells from the fewest sprites, the lowest-numbered on a tie. Each art level becomes pieces, one
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
--
-- An animation (sprites.animation(arts, frames, {once = true})) is drawn like art: each draw shows one of its arts,
-- each for its `frames` video frames (one number for all, or a list, as Tiled's tile animations), looping (once: holding
-- the last). Its clock counts video frames: the draw's `at` (the game's own count, `f` or an actor's age), else its
-- key's, from 0 on the first update that draws the key with that animation, advancing by the video frames of each update
-- that does again (30 Hz: 2, so an art shorter than an update can be skipped). Drawing the key with anything else, or an
-- update that does not draw it, restarts it. Unlike a world's animated tiles, which all play on the scene's `f`, a keyed
-- animation starts where the key starts it (walk, then jump, plays the jump from its first art). Every art of an
-- animation counts as its largest, so a key's block holds whichever one it shows.
sprites = {flicker = false}

local ARTS = {}
-- The most pieces each art shows, by its sprites.art call (the plan numbers them; the engine rewrites
-- each call to pass its number).
__ppu_sprites_cells = {}
__ppu_sprites_arts = {}
-- Animations: by table, and in the order made.
local ANIMS, anims = {}, {}
-- Flushes so far, wrapping at 32768 (the cartridge's counter); per group and key (g * 128 + key) the animation it last
-- drew, its clock and the flush count then. A scene's entry clears them, as the cartridge's dispatcher does. A paused
-- repeat of an update (`__ppu_sprites_again`, which the engine sets before frame()) draws as the update did: no clock
-- advances and the count stands.
local now, clocks = 0, {}
-- The video frames an update spans (30 Hz: 2), from the last flush: a key's clock advances by them.
local step = 1
-- OBJSEL size pairs by obj.size_sel: small width, height, large width, height.
local PAIRS = {[0] = {8, 8, 16, 16}, {8, 8, 32, 32}, {8, 8, 64, 64}, {16, 16, 32, 32}, {16, 16, 64, 64},
  {32, 32, 64, 64}, {16, 32, 32, 64}, {16, 32, 32, 32}}
-- The pair the art was built for, once __ppu_sprites_finish has run.
local pair = nil
local held, drawn = false, false
-- sprites.rows: the rows this frame's draws from now on must lie wholly inside (top, bottom past the last), or none.
local rows_top, rows_bottom = nil, nil
local SIZE_HELP = ": re-import a sheet at another cell_size, or set obj.size_sel to the pair to draw with (cells of other sizes are drawn as several sprites)"
local CONFLICT = "error[snes.sprites]: sprites.draw and sprites.hold cannot both run in one frame"
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
    and type(sheet.cells) == "table", "error[snes.sprites]: sprites.art needs a sprite sheet from dma()")
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
        local pal = level.palette and level.pal + (entry.pal or 0) or level.pal or sheet.pal + (entry.pal or 0)
        assert(integer(tile, 0, 511), "error[snes.sprites]: sprites.art cell tile must be 0..511")
        -- A sprite's 8x8 tiles run right within a 16-tile row of the name table, wrapping at its end.
        assert(tile % 16 + pw // 8 <= 16, "error[snes.sprites]: sprites.art cell " .. cell .. " runs past the end of a 16-tile row of OBJ tiles: "
          .. "place its sheet at a char address that starts a row (a multiple of 0x100 words from obj.char_base)")
        assert(integer(pal, 0, 7), "error[snes.sprites]: sprites.art cell palette must be 0..7")
        out[#out + 1] = {dx = cx + p[1], dy = cy + p[2], w = pw, h = ph, tile = tile, pal = pal, large = p[3],
          flip_x = entry.flip_x == true, flip_y = entry.flip_y == true}
      end
    end
    assert(#out >= 1, "error[snes.sprites]: sprites.art level " .. i .. " draws nothing: every tile of its cells is blank")
    level.pieces = out
    -- By flips: [0] none, [1] x, [2] y, [3] both.
    art[i] = {[0] = variant(level, false, false), variant(level, true, false), variant(level, false, true),
      variant(level, true, true), size = level.size}
    art.cells = max(art.cells, #out)
  end
end

function sprites.art(levels, sheet)
  sheet_ok(sheet)
  assert(type(levels) == "table" and #levels >= 1 and #levels <= 16, "error[snes.sprites]: sprites.art needs 1..16 levels")
  local art = {levels = {}, cells = 0}
  for i, level in ipairs(levels) do
    local columns, cells, own = level.columns, level.cells, level.sheet or sheet
    if level.sheet ~= nil then sheet_ok(level.sheet) end
    assert(level.pal == nil or integer(level.pal, 0, 7), "error[snes.sprites]: sprites.art level pal must be an integer 0..7")
    assert(level.palette == nil or type(level.palette) == "string", "error[snes.sprites]: sprites.art level palette must be a palette source name")
    assert(level.palette == nil or level.pal ~= nil, "error[snes.sprites]: sprites.art level palette needs pal: the OBJ palette (0..7) its colours go to")
    assert(integer(level.size, 0, 1024), "error[snes.sprites]: sprites.art level size must be an integer 0..1024")
    assert(type(cells) == "table" and #cells >= 1 and #cells <= 64, "error[snes.sprites]: sprites.art level needs 1..64 cells")
    assert(integer(columns, 1, 16) and #cells % columns == 0, "error[snes.sprites]: sprites.art level cells must fill its columns")
    for _, cell in ipairs(cells) do
      assert(integer(cell, 0, 1023) and own.cells[cell + 1], "error[snes.sprites]: sprites.art cell " .. tostring(cell) .. " is not in the sheet")
    end
    local w = own.cell_w or own.cell_size or 16
    art.levels[i] = {size = level.size, columns = columns, cells = cells, sheet = own, w = w, h = own.cell_h or w, pal = level.pal, palette = level.palette}
  end
  -- Every check comes before anything is placed. Whether a palette fits is dma's own rule; what is
  -- left here is only that the whole palette, placed from level.pal, stays within OBJ palette 7.
  for _, level in ipairs(levels) do
    if level.palette ~= nil then
      local own = level.sheet or sheet
      assert(type(own.name) == "string", "error[snes.sprites]: sprites.art level palette needs a sprite sheet from dma()")
      local ok, rows = pcall(__ppu_palette_fits, own.name, level.palette)
      -- dma's refusal, retold under this library's code
      local why = tostring(rows)
      if string.sub(why, 1, 17) == "error[snes.dma]: " then why = string.sub(why, 18) end
      assert(ok, "error[snes.sprites]: sprites.art level palette " .. level.palette .. ": " .. why)
      assert(level.pal + rows <= 8, "error[snes.sprites]: sprites.art level palette " .. level.palette .. " has " .. rows
        .. " rows and goes to OBJ palettes " .. level.pal .. ".." .. level.pal + rows - 1 .. ", past palette 7")
    end
  end
  for _, level in ipairs(levels) do
    if level.palette ~= nil then
      dma(level.palette, {pal = 128 + 16 * level.pal})
    end
  end
  ARTS[art] = true
  if pair ~= nil then build(art, PAIRS[pair]) end
  return art
end

function __ppu_sprites_art(site, levels, sheet)
  local art = sprites.art(levels, sheet)
  art.site = site
  __ppu_sprites_arts[site] = art
  return art
end

function sprites.animation(arts, frames, options)
  assert(type(arts) == "table" and #arts >= 1 and #arts <= 64, "error[snes.sprites]: sprites.animation needs 1..64 arts")
  for _, art in ipairs(arts) do
    assert(ARTS[art], "error[snes.sprites]: sprites.animation needs art made by sprites.art")
  end
  local each = frames
  if type(frames) ~= "table" then
    each = {}
    for i = 1, #arts do each[i] = frames end
  end
  assert(#each == #arts, "error[snes.sprites]: sprites.animation frames must be one number, or a list with one per art")
  local own, ends, length = {}, {}, 0
  for i, art in ipairs(arts) do
    assert(integer(each[i], 1, 255), "error[snes.sprites]: sprites.animation frames (an art's video frames) must be whole numbers 1..255")
    own[i], length = art, length + each[i]
    ends[i] = length
  end
  local once = options ~= nil and options.once
  assert(once == nil or type(once) == "boolean", "error[snes.sprites]: sprites.animation once must be a boolean")
  local anim = {arts = own, ends = ends, length = length, once = once == true}
  ANIMS[anim] = true
  anims[#anims + 1] = anim
  return anim
end

-- The cartridge reads each animation by its site: its arts' sites, the video frame each ends on, and whether it plays once.
__ppu_sprites_anims = {}
function __ppu_sprites_animation(site, arts, frames, options)
  local anim = sprites.animation(arts, frames, options)
  __ppu_sprites_anims[site] = anim
  return anim
end

-- The art a draw of `art` shows: itself, or its animation's art at the draw's `at`, else at its key's clock.
local function play(g, art, key, at)
  if not ANIMS[art] then return art end
  local length, clock = art.length, 0
  if at ~= nil then
    assert(type(at) == "number" and at == at and at >= 0, "error[snes.sprites]: sprites.draw at must be a number of video frames, 0 or more")
    clock = floor(at)
    if art.once then clock = min(clock, length - 1) else clock = clock % length end
  elseif key == nil then
    error("error[snes.sprites]: sprites.draw of an animation needs a key, whose clock it plays on, or {at = frames}", 4)
  else
    local c, cur = clocks[g * 128 + key], now
    if __ppu_sprites_again then cur = (now + 32767) % 32768 end
    if c ~= nil and c.anim == art and c.seen == cur and __ppu_sprites_again then
      clock = c.clock
    elseif c ~= nil and c.anim == art and c.seen == (cur + 32767) % 32768 then
      clock = c.clock + step
      if art.once then clock = min(clock, length - 1) else clock = clock % length end
    end
    clocks[g * 128 + key] = {anim = art, clock = clock, seen = cur}
  end
  for i, e in ipairs(art.ends) do
    if clock < e then return art.arts[i] end
  end
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

local function hex(n)
  local out = ""
  for i = 3, 0, -1 do
    local d = n // 16 ^ i % 16
    out = out .. string.sub("0123456789abcdef", d + 1, d + 1)
  end
  return "0x" .. out
end

-- The sprite layer's setup a program leaves to the library (set once, then at each scene's cold boot):
-- OBSEL's char base at the one its art's sheets were placed from, and the sprite layer on the main screen.
local function setup()
  if __ppu_sprites_char_base ~= nil then obj.char_base = __ppu_sprites_char_base end
  if __ppu_sprites_layer then screen.main.obj = true end
end

-- After the file-level code: the size pair (`pin`, the program's own obj.size_sel, or chosen), every
-- art built for it and its piece count in __ppu_sprites_cells. When no pair fits, the art number and
-- why in __ppu_sprites_fail_site and __ppu_sprites_fail. Then the setup the program does not write
-- itself (`sets_char_base`, `sets_layer`).
function __ppu_sprites_finish(pin, arts, sets_char_base, sets_layer)
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
    __ppu_sprites_fail_site, __ppu_sprites_fail = at[min(#order, 3)], "sprites art uses " .. names .. " cells, and an obj.size_sel pair has two sizes" .. SIZE_HELP
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
    __ppu_sprites_fail_site, __ppu_sprites_fail = fail[1], "sprites art cells of " .. fail[2] .. " can't be built from obj.size_sel " .. candidates[1] .. "'s sprites" .. SIZE_HELP
    return
  end
  pair = best
  for site = 1, arts do
    local art = __ppu_sprites_arts[site]
    if art then build(art, PAIRS[best]) end
  end
  -- An animation's arts count as its largest, to a fixed point (animations sharing an art share one size), so a key's
  -- block holds any art it shows (the cartridge compiler's `block_cells`).
  local changed = true
  while changed do
    changed = false
    for _, anim in ipairs(anims) do
      local m = 0
      for _, art in ipairs(anim.arts) do m = max(m, art.cells) end
      for _, art in ipairs(anim.arts) do
        if art.cells < m then art.cells, changed = m, true end
      end
    end
  end
  for site = 1, arts do
    local art = __ppu_sprites_arts[site]
    if art then __ppu_sprites_cells[site] = art.cells end
  end
  __ppu_sprites_size_sel = best
  if pin == nil then obj.size_sel = best end
  if not sets_char_base then
    -- The base dma() measured each sheet's tiles from: every art's must be one, which OBSEL holds.
    local base = nil
    for site = 1, arts do
      local art = __ppu_sprites_arts[site]
      for _, level in ipairs(art and art.levels or {}) do
        local sheet = level.sheet
        if type(sheet.char) == "number" then
          local b = sheet.char - sheet.tile * 16
          if base ~= nil and b ~= base then
            __ppu_sprites_fail_site, __ppu_sprites_fail = site, "sprites art draws from sheets placed from char " .. hex(base) .. " and " .. hex(b)
              .. ", and obj.char_base holds one: place every sprite sheet in one 8K-word region (from char 0x0000, 0x2000, 0x4000 or 0x6000), "
              .. "or set obj.char_base yourself"
            return
          end
          base = b
        end
      end
    end
    __ppu_sprites_char_base = base
  end
  __ppu_sprites_layer = not sets_layer
  setup()
end

local function group(g)
  local s = groups[g]
  if s == nil then
    local l = __ppu_sprites_layout[g]
    s = {first = l.first, size = l.size, sort = l.sort, keep = l.keep, blocks = {}, owned = 0, unkeyed = l.size,
      touched = {}, keyed = {}, records = {}, rotation = 0, last = {}}
    groups[g] = s
  end
  return s
end

local function touch(s, k, how)
  if not integer(k, 0, 127) then error("error[snes.sprites]: " .. how .. " key must be an integer 0..127", 4) end
  local was = s.touched[k]
  if was == "draw" and how == "draw" then error("error[snes.sprites]: sprites key " .. k .. " is drawn twice in one frame", 4) end
  if was == "keep" and how == "keep" then error("error[snes.sprites]: sprites key " .. k .. " is kept twice in one frame", 4) end
  if was ~= nil then error("error[snes.sprites]: sprites key " .. k .. " is both drawn and kept in one frame", 4) end
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
    error("error[snes.sprites]: keep(" .. k .. "): the key was never drawn in this group", 3)
  end
  -- A sorted group places a kept entry again, where its depth puts it.
  if s.sort then record(s, s.last[k]) end
end

local function draw(g, art, x, y, options)
  if held then error(CONFLICT, 3) end
  drawn = true
  assert(ARTS[art] or ANIMS[art], "error[snes.sprites]: sprites.draw needs art made by sprites.art or sprites.animation")
  assert(type(x) == "number" and type(y) == "number" and x == x and y == y, "error[snes.sprites]: sprites.draw position must be numbers")
  local depth, size, prio, key, fx, fy, pal, at = 0, nil, 3, nil, false, false, nil, nil
  if options ~= nil then
    depth, size, prio, key = options.depth or 0, options.size, options.prio or 3, options.key
    fx, fy, pal, at = options.flip_x or false, options.flip_y or false, options.pal, options.at
  end
  assert(type(depth) == "number" and depth >= -32768 and depth < 32768, "error[snes.sprites]: sprites.draw depth must be a number in -32768..32767")
  assert(size == nil or (type(size) == "number" and size == size), "error[snes.sprites]: sprites.draw size must be a number")
  assert(integer(prio, 0, 3), "error[snes.sprites]: sprites.draw prio must be an integer 0..3")
  assert(type(fx) == "boolean" and type(fy) == "boolean", "error[snes.sprites]: sprites.draw flip_x and flip_y must be booleans")
  assert(pal == nil or integer(pal, 0, 7), "error[snes.sprites]: sprites.draw pal must be an integer 0..7")
  local s = group(g)
  if key ~= nil then touch(s, key, "draw") end
  art = play(g, art, key, at)
  if key ~= nil then
    if not s.sort then
      local block = s.blocks[key]
      if block == nil then
        local left = s.size - s.owned
        if art.cells > left then
          error("error[snes.sprites]: sprites key " .. key .. " needs " .. art.cells .. " slots; " .. left .. " are left in its group", 3)
        end
        block = {first = s.first + s.owned, size = art.cells}
        s.owned = s.owned + art.cells
        s.blocks[key] = block
      elseif art.cells > block.size then
        error("error[snes.sprites]: sprites key " .. key .. " has " .. block.size .. " slots, from its first art; this art needs " .. art.cells, 3)
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
  local r = {level = level, x = x, y = y, depth = depth, prio = prio, pal = pal, top = rows_top, bottom = rows_bottom}
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

function sprites.rows(top, bottom)
  assert(integer(top, 0, 255) and integer(bottom, top + 1, 256), "error[snes.sprites]: sprites.rows takes whole rows: a top 0..255 and a bottom below it, up to 256")
  rows_top, rows_bottom = top, bottom
end

function sprites.hold()
  if drawn then error(CONFLICT, 2) end
  held = true
end

-- Whether piece p of entry r at (x, y) shows: then the sprite takes it.
local function shows(p, r, x, y)
  return x > -p.w and x < 256 and y > p.ylo and y < p.yhi and (r.top == nil or (y >= r.top and y + p.h <= r.bottom))
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
      if #cells < slots and shows(p, r, x, y) then
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

function __ppu_sprites_flush(frames)
  if not __ppu_sprites_again then now = (now + 1) % 32768 end
  step = frames or 1
  local was = held
  held, drawn = false, false
  rows_top, rows_bottom = nil, nil
  local free = __ppu_sprites_free
  for g = 1, #__ppu_sprites_layout do
    local s = group(g)
    local touched = s.touched
    s.touched = {}
    local scene = __ppu_sprites_layout[g].scene
    if was or (scene ~= nil and not scene[__ppu_sprites_scene]) then
      -- A held frame keeps every group as it was; another scene's group shows nothing.
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
        if not touched[k] and not s.keep then
          for i = 1, block.size do obj[free[block.first + i]].on = false end
        end
      end
      for _, d in ipairs(s.keyed) do
        local r = d.r
        for i = 1, d.block.size do
          local sprite, p = obj[free[d.block.first + i]], r.level[i]
          local x, y = p and r.x + p.dx, p and r.y + p.dy
          if p and shows(p, r, x, y) then
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

-- A scene's cold boot: no group holds a sprite, and OBJSEL is the pair the art was built for.
function __ppu_sprites_reset()
  groups, held, drawn, clocks = {}, false, false, {}
  if pair ~= nil then obj.size_sel = pair end
  setup()
end

function sprites.group()
  -- No plan: the program never draws, so the group shows nothing (as on the cartridge).
  if __ppu_sprites_free == nil then return {} end
  error("error[snes.sprites]: sprites.group must be called at file level or in init(), where the plan numbers it", 2)
end
