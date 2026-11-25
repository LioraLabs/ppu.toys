--!strict
-- @ppu/world: a Tiled map (a .tmj project file) larger than the screen, streamed into backgrounds.
-- local world = require("@ppu/world"); local w = world.load("level1") at file level;
-- in init(): w:bind("ground", bg[1]) and w:follow(x, y); in frame(): w:follow(x, y), x in 0..w.max_x and y in 0..w.max_y.
-- Each tile layer binds to its own background and one follow streams them all; a layer may scroll
-- slower than the camera, w:bind("far", bg[3], {parallax = 0.5}) (sixteenths from 1/16 to 1).
-- With {wrap = true} a layer repeats its painted part (up to its last column and row with a tile).
-- The option map = 0x0400 places the background's map (else bind takes the map_base already set).
-- With {priority = 1} every cell the layer streams draws at tile priority 1 (Mode 1: over priority 2 sprites).
-- A map drawn from an m7 source binds to the plane instead, w:bind("track", m7), and loads whole.
-- One past 128x128 cells streams through the plane: load it with {radius = r}, and
-- w:follow(x, y) the point the matrix centres on; the window is r cells either side of it.
-- A perspective plane loads with {view = {ahead = A, behind = B}} (and speed = 1..8, default 8) and follows
-- w:follow(x, y, angle): the whole plane chases a window leading the camera along the heading; a move past speed
-- redraws the whole window.
-- The camera moves as far as the author likes: a follow draws every row and column the move uncovers, and
-- w:follow(x, y, {cut = true}) (a view's w:follow(x, y, angle, {cut = true})) redraws the whole window on purpose.
-- w:find("start") is the one object named start among w.objects, the objects of the map w shows.
-- w:enter(name, x, y) shows another loaded map on the same backgrounds, a hard cut under force_blank.
-- w:set(layer, x, y, tile) changes the Tiled tile under map pixel (x, y) to tile id `tile` of the layer's
-- tileset: w:tile reports its class, and a background showing it redraws it now. w:bind and w:enter start
-- the map as authored again.
-- A generated world loads with {period = P, pick = f} beside a radius or a view: its map is a sheet of
-- 16x16-cell templates, and chunk (cx, cy) of ground P cells round shows template f(cx, cy), numbered from 0
-- left to right, top to bottom. math.hash(seed, x, y) is a hash for f to pick with.
-- A generated map drawn from sheets (no radius, no view) streams to backgrounds instead: {chunk = {w, h}} sizes
-- its templates (16 or 32 cells each), every bound layer shows the same picked templates, and the camera wraps.
-- The library only does camera arithmetic: the engine's __ppu_world_strip presents each row or
-- column of cells the camera uncovers, so a cartridge can stream them its own way.

-- Each load's own fields, by its id, so a world that entered another map can come back to it.
local loads: {[number]: any} = {}
-- Every world this run loaded (file-level loads, one per world.load), for the engine's hot swap hooks.
local worlds: {any} = {}

-- Show load `id`'s map: its fields replace the world's (the engine already moved the bindings).
local function show(w: any, id: number)
  local m = loads[id]
  w.id, w.name, w.width, w.height = m.id, m.name, m.width, m.height
  w.kinds, w.objects, w.layers = m.kinds, m.objects, m.layers
  w.max_x, w.max_y = m.max_x, m.max_y
end

-- The cells a camera at (x, y) shows, clipped to the map: columns c0..c1, rows r0..r1 (at most
-- 33 by 29, so a 64x32 VRAM map holds them without a visible cell sharing a slot).
local function window(w: any, x: number, y: number): (number, number, number, number)
  return x // 8, math.min(x // 8 + 32, w.width - 1), y // 8, math.min((y + 223) // 8, w.height - 1)
end

-- A bind's options: the parallax factor in sixteenths, 16 (the camera's rate) when none is given, and
-- whether the layer repeats its painted part (wrap).
local function parallax(options: any): (number, boolean)
  if options == nil then return 16, false end
  if type(options) ~= "table" then error("error[snes.world]: w:bind: the third argument is the options table, {parallax = 0.5}") end
  for k in options do
    if k ~= "parallax" and k ~= "wrap" and k ~= "map" and k ~= "priority" then
      error("error[snes.world]: w:bind: the options are parallax, wrap, map and priority (got " .. tostring(k) .. ")")
    end
  end
  local wrap = options.wrap
  if wrap ~= nil and type(wrap) ~= "boolean" then error("error[snes.world]: w:bind: wrap is true or false (got " .. tostring(wrap) .. ")") end
  local f = options.parallax
  if f == nil then return 16, wrap == true end
  if type(f) ~= "number" or not (f > 0 and f <= 1) or (f * 16) % 1 ~= 0 then
    error("error[snes.world]: w:bind: parallax is the layer's scroll rate as a multiple of 1/16 from 1/16 to 1, such as 0.25 or 0.5 (got " .. tostring(f) .. ")")
  end
  return f * 16, wrap == true
end

-- A bind option's background map address, {map = 0x0400}: words, a multiple of 0x400 (nil when none is given).
local function map_option(options: any): number?
  local m = if type(options) == "table" then options.map else nil
  if m ~= nil and (type(m) ~= "number" or m % 0x400 ~= 0 or m < 0 or m >= 0x8000) then
    error("error[snes.world]: w:bind: map is the background map's VRAM word address, a multiple of 0x400 below 0x8000, such as {map = 0x0400} (got " .. tostring(m) .. ")")
  end
  return m
end

-- A bind option's tile priority, {priority = 1}: every cell the layer streams draws at priority 0 (the default) or 1.
local function priority_option(options: any): boolean
  local v = if type(options) == "table" then options.priority else nil
  if v ~= nil and v ~= 0 and v ~= 1 then
    error("error[snes.world]: w:bind: priority is the layer's tile priority, 0 or 1, such as {priority = 1} (got " .. tostring(v) .. ")")
  end
  return v == 1
end

-- Binds tile layer `name` to a background (with parallax, wrap, map and priority options) or to m7.
local function bind(w: any, name: string, layer: any, options: any)
  local p, wrap = parallax(options)
  local m = map_option(options)
  local high = priority_option(options)
  if w.id ~= w.home then
    -- A bind (init, or a scene entered again) starts the world over on the map it was loaded with.
    show(w, w.home)
    w.bound = {false, false, false, false}
    w.parallax = {16, 16, 16, 16}
  end
  if layer == m7 then
    if options ~= nil then error("error[snes.world]: w:bind: the Mode 7 plane takes no options: its matrix is its camera") end
    if w.radius and (w.bound[1] or w.bound[2] or w.bound[3] or w.bound[4]) then
      error("error[snes.world]: w:bind: world " .. w.name .. " streams backgrounds or the Mode 7 plane, not both")
    end
    -- A Mode 7 map loads whole, now, into the 128x128 plane (one row strip per plane row); the
    -- matrix is its camera, so follow leaves it alone. An EXTBG tileset brings BG2's priority.
    local _, extbg = __ppu_world_bind(w.id, name, 0, 0)
    if w.radius then
      -- A streamed map fills at the next follow; a generated one picks every chunk again.
      w.plane, w.cut = true, true
      if w.period then
        w.tags = {}
        for i = 1, 256 do w.tags[i] = 0 end
      end
    else
      for r = 0, 127 do __ppu_world_strip(w.id, 0, 0, r, 128, false) end
    end
    if extbg then m7.extbg = true end
    return
  end
  if w.plane then error("error[snes.world]: w:bind: world " .. w.name .. " streams backgrounds or the Mode 7 plane, not both") end
  local n = 0
  for i = 1, 4 do
    if bg[i] == layer then n = i end
  end
  if n == 0 then error("error[snes.world]: w:bind: the second argument is a background, bg[1] to bg[4], or m7") end
  if layer.tile_size ~= 8 then error("error[snes.world]: w:bind: a world draws 8x8 background tiles: set bg[" .. n .. "].tile_size = 8") end
  if w.period then
    -- Every layer of generated ground shows the same chunks, so each moves at the camera's rate.
    if p ~= 16 or wrap then
      error("error[snes.world]: w:bind: world " .. w.name .. " is generated, and its layers scroll with the camera: bind a slower layer from a world of its own")
    end
    -- The next follow picks every chunk again.
    w.tags = {}
    for i = 1, 256 do w.tags[i] = 0 end
  end
  if m then layer.map_base = m end
  local char, wide = __ppu_world_bind(w.id, name, n, layer.map_base, wrap, high)
  layer.char_base = char
  layer.screen_size = if wide then 1 else 0
  w.bound[n] = true
  w.parallax[n] = p
  w.shows[n] = name
  -- The next follow draws every bound layer whole: a cold boot or a new binding left nothing on screen.
  w.cut = true
end

-- A generated world's template for chunk (cx, cy): what its pick returns, checked.
local function template(w: any, cx: number, cy: number): number
  local t = w.pick(cx, cy)
  if type(t) ~= "number" or t % 1 ~= 0 or t < 0 or t >= w.templates then
    error("error[snes.world]: world " .. w.name .. ": pick(" .. cx .. ", " .. cy .. ") returned " .. tostring(t)
      .. ": return a template's number, 0 to " .. (w.templates - 1))
  end
  return t
end

-- One strip of a generated world's window, 2 * radius cells from cell (x, y). Each chunk it crosses gets
-- its template first, unless its grid slot (16x16 chunks, wrapped) already holds that chunk: the pick runs
-- once per chunk that comes into the window, never per cell.
local function strip(w: any, x: number, y: number, column: boolean)
  local n, chunks = 2 * w.radius, w.period // 16
  x, y = x % w.period, y % w.period
  local first = if column then y else x
  for i = first // 16, (first + n - 1) // 16 do
    local cx, cy = i % chunks, y // 16
    if column then cx, cy = x // 16, i % chunks end
    local slot, tag = cy % 16 * 16 + cx % 16, cy // 16 * 16 + cx // 16 + 1
    if w.tags[slot + 1] ~= tag then
      local t = template(w, cx, cy)
      __ppu_world_chunk(w.id, slot, t % w.columns * 16, t // w.columns * 16)
      w.tags[slot + 1] = tag
    end
  end
  __ppu_world_strip(w.id, 0, x, y, n, column)
end

-- A generated world's follow, with a radius or a view (heading h): the window is the 2 * radius cells
-- square about a centre cell that steps toward its target at most 2 cells an axis, the short way round
-- the period. The target is the follow point's cell, or a view's, leading it along the heading as followView's
-- does. A radius world whose point moved more than 2 cells, or a view world's camera past its speed, redraws the
-- whole window at the target. Nothing clamps: the ground has no edge.
local function followGenerated(w: any, x: number, y: number, h: number?)
  local period, r = w.period, w.radius
  local size = period * 8
  if x < 0 or x >= size or y < 0 or y >= size then
    error("error[snes.world]: w:follow: the point " .. x .. ", " .. y .. " is off the generated ground, which is " .. size
      .. " px round: wrap it, x % " .. size .. " and y % " .. size)
  end
  local tx, ty = x // 8, y // 8
  if h then tx, ty = (x + w.lead_x[h + 1]) // 8 % period, (y + w.lead_y[h + 1]) // 8 % period end
  if not w.cut then
    local half = period // 2
    if h then
      local mx, my = (x - w.x) % size, (y - w.y) % size
      if math.min(mx, size - mx) > w.speed or math.min(my, size - my) > w.speed then w.cut = true end
    elseif math.abs((tx - w.gx + half) % period - half) > 2 or math.abs((ty - w.gy + half) % period - half) > 2 then
      w.cut = true
    end
  end
  local lx, ly = tx, ty
  if w.cut then
    for i = 0, 2 * r - 1 do strip(w, lx - r + i, ly - r, true) end
  else
    local ox, oy = w.gx, w.gy
    local half = period // 2
    local dx = math.max(-2, math.min((tx - ox + half) % period - half, 2))
    local dy = math.max(-2, math.min((ty - oy + half) % period - half, 2))
    lx, ly = (ox + dx) % period, (oy + dy) % period
    -- New columns over the final rows, then new rows over the final columns, nearest the old window first.
    if dx > 0 then strip(w, ox + r, ly - r, true) end
    if dx > 1 then strip(w, ox + r + 1, ly - r, true) end
    if dx < 0 then strip(w, ox - r - 1, ly - r, true) end
    if dx < -1 then strip(w, ox - r - 2, ly - r, true) end
    if dy > 0 then strip(w, lx - r, oy + r, false) end
    if dy > 1 then strip(w, lx - r, oy + r + 1, false) end
    if dy < 0 then strip(w, lx - r, oy - r - 1, false) end
    if dy < -1 then strip(w, lx - r, oy - r - 2, false) end
  end
  w.x, w.y, w.gx, w.gy = x, y, lx, ly
  w.cut = false
end

-- One strip of generated ground on every bound background, n cells from cell (x, y) (wrapped at the period):
-- each chunk it crosses gets its template first, unless its grid slot already holds that chunk.
local function stripBackgrounds(w: any, x: number, y: number, n: number, column: boolean)
  local period, cw, ch = w.period, w.cw, w.ch
  local first, size = if column then y else x, if column then ch else cw
  for i = first // size, (first + n - 1) // size do
    local cx, cy = i % (period // cw), y // ch
    if column then cx, cy = x // cw, i % (period // ch) end
    local slot, tag = cy % 16 * 16 + cx % 16, cy // 16 * (period // cw // 16) + cx // 16 + 1
    if w.tags[slot + 1] ~= tag then
      local t = template(w, cx, cy)
      __ppu_world_chunk(w.id, slot, t % w.columns * cw, t // w.columns * ch)
      w.tags[slot + 1] = tag
    end
  end
  for b = 1, 4 do
    if w.bound[b] then __ppu_world_strip(w.id, b, x, y, n, column) end
  end
end

-- A generated world on backgrounds: the window is 33 columns by 29 rows from the camera's cell, the camera moves
-- the short way round the period, and a move draws the columns it uncovers over the new rows, then the rows over
-- the new columns, each in increasing order; a move past the window redraws it whole.
local function followGeneratedBackgrounds(w: any, x: number, y: number)
  local period = w.period
  local size = period * 8
  if x < 0 or x >= size or y < 0 or y >= size then
    error("error[snes.world]: w:follow: the point " .. x .. ", " .. y .. " is off the generated ground, which is " .. size
      .. " px round: wrap it, x % " .. size .. " and y % " .. size)
  end
  local c0, r0 = x // 8, y // 8
  local half = period // 2
  local dx = (c0 - w.x // 8 + half) % period - half
  local dy = (r0 - w.y // 8 + half) % period - half
  if math.abs(dx) >= 33 or math.abs(dy) >= 29 then w.cut = true end
  if w.cut then
    for i = 0, 28 do stripBackgrounds(w, c0, (r0 + i) % period, 33, false) end
  else
    -- The new columns over the new rows, then the new rows over the new columns.
    local first = if dx > 0 then c0 + 33 - dx else c0
    for i = 0, math.abs(dx) - 1 do stripBackgrounds(w, (first + i) % period, r0, 29, true) end
    first = if dy > 0 then r0 + 29 - dy else r0
    for i = 0, math.abs(dy) - 1 do stripBackgrounds(w, c0, (first + i) % period, 33, false) end
  end
  for b = 1, 4 do
    if w.bound[b] then
      bg[b].scroll.x = x % 1024
      bg[b].scroll.y = (y - 1) % 1024
    end
  end
  w.x, w.y = x, y
  w.cut = false
end

-- A streamed plane: the window of cells around (x, y): the columns a move uncovers over the new rows, then the rows
-- over the new columns, each in increasing order (a move past the window: all of it).
local function followPlane(w: any, px: number, py: number)
  -- The window's centre is the follow point (a view world's leads it: followView).
  local x, y = px, py
  local r = w.radius
  local left, right, top, bottom = r * 8, (w.width - r) * 8 + 7, r * 8, (w.height - r) * 8 + 7
  if x < left or x > right or y < top or y > bottom then
    error("error[snes.world]: w:follow: the point " .. x .. ", " .. y .. " leaves the map's streaming window: keep x in " .. left
      .. ".." .. right .. " and y in " .. top .. ".." .. bottom .. " (clamp it)")
  end
  local cx, cy = x // 8, y // 8
  local c0, c1, r0, r1 = cx - r, cx + r, cy - r, cy + r
  if w.cut then
    for c = c0, c1 - 1 do __ppu_world_strip(w.id, 0, c, r0, 2 * r, true) end
  else
    if c0 < w.c0 then
      for c = c0, math.min(w.c0, c1) - 1 do __ppu_world_strip(w.id, 0, c, r0, 2 * r, true) end
    elseif c1 > w.c1 then
      for c = math.max(w.c1, c0), c1 - 1 do __ppu_world_strip(w.id, 0, c, r0, 2 * r, true) end
    end
    if r0 < w.r0 then
      for q = r0, math.min(w.r0, r1) - 1 do __ppu_world_strip(w.id, 0, c0, q, 2 * r, false) end
    elseif r1 > w.r1 then
      for q = math.max(w.r1, r0), r1 - 1 do __ppu_world_strip(w.id, 0, c0, q, 2 * r, false) end
    end
  end
  w.x, w.y, w.c0, w.c1, w.r0, w.r1 = x, y, c0, c1, r0, r1
  w.cut = false
end

-- A view world's plane: the 128x128 window chases a target that leads the follow point by
-- (ahead - behind) / 2 cells along heading h; it moves VIEW_STEP = 2 cells an axis a follow, so
-- the near square (w.near cells either side of the camera) is always loaded and far rows catch
-- up within w.catch_up follows. Unrolled twice per axis: a cartridge mirrors this shape. A camera that moved more
-- than w.speed px on an axis outran the chase: the whole window redraws at the target.
local function followView(w: any, x: number, y: number, h: number)
  local n = w.near
  local left, right, top, bottom = n * 8, (w.width - n) * 8 + 7, n * 8, (w.height - n) * 8 + 7
  if x < left or x > right or y < top or y > bottom then
    error("error[snes.world]: w:follow: the point " .. x .. ", " .. y .. " leaves the map's streaming window: keep x in " .. left
      .. ".." .. right .. " and y in " .. top .. ".." .. bottom .. " (clamp it)")
  end
  if not w.cut and (math.abs(x - w.x) > w.speed or math.abs(y - w.y) > w.speed) then w.cut = true end
  -- The lead, floor(+-4 (ahead - behind) * sin/cos(h * pi / 128) + 0.5) px, from the engine's f64 tables.
  local dx, dy = w.lead_x[h + 1], w.lead_y[h + 1]
  local tx = math.max(64, math.min((x + dx) // 8, w.width - 64))
  local ty = math.max(64, math.min((y + dy) // 8, w.height - 64))
  local lx, ly
  if w.cut then
    lx, ly = tx, ty
    for c = lx - 64, lx + 63 do __ppu_world_strip(w.id, 0, c, ly - 64, 128, true) end
  else
    local ox, oy = w.c0 + 64, w.r0 + 64
    lx, ly = ox, oy
    if tx > lx then lx += 1 elseif tx < lx then lx -= 1 end
    if tx > lx then lx += 1 elseif tx < lx then lx -= 1 end
    if ty > ly then ly += 1 elseif ty < ly then ly -= 1 end
    if ty > ly then ly += 1 elseif ty < ly then ly -= 1 end
    -- New columns over the final rows, then new rows over the final columns, nearest the old window first.
    if lx > ox then __ppu_world_strip(w.id, 0, ox + 64, ly - 64, 128, true) end
    if lx > ox + 1 then __ppu_world_strip(w.id, 0, ox + 65, ly - 64, 128, true) end
    if lx < ox then __ppu_world_strip(w.id, 0, ox - 65, ly - 64, 128, true) end
    if lx < ox - 1 then __ppu_world_strip(w.id, 0, ox - 66, ly - 64, 128, true) end
    if ly > oy then __ppu_world_strip(w.id, 0, lx - 64, oy + 64, 128, false) end
    if ly > oy + 1 then __ppu_world_strip(w.id, 0, lx - 64, oy + 65, 128, false) end
    if ly < oy then __ppu_world_strip(w.id, 0, lx - 64, oy - 65, 128, false) end
    if ly < oy - 1 then __ppu_world_strip(w.id, 0, lx - 64, oy - 66, 128, false) end
  end
  w.x, w.y, w.c0, w.c1, w.r0, w.r1 = x, y, lx - 64, lx + 64, ly - 64, ly + 64
  w.cut = false
end

-- A follow's options, {cut = true}: redraw the whole window (a teleport).
local function cutOption(options: any): boolean
  if options == nil then return false end
  if type(options) ~= "table" then error("error[snes.world]: w:follow: the options are a table, {cut = true} (got " .. tostring(options) .. ")") end
  for k in options do
    if k ~= "cut" then error("error[snes.world]: w:follow: the one option is cut, {cut = true} (got " .. tostring(k) .. ")") end
  end
  if options.cut ~= nil and type(options.cut) ~= "boolean" then error("error[snes.world]: w:follow: cut is true or false (got " .. tostring(options.cut) .. ")") end
  return options.cut == true
end

-- Moves the camera to map pixel (x, y) and streams what the move uncovers; a view world takes the heading.
local function follow(w: any, x: number, y: number, angle: any, options: any)
  x, y = x // 1, y // 1
  if not w.ahead and type(angle) == "table" then angle, options = nil, angle end
  if cutOption(options) then w.cut = true end
  if w.ahead then
    if type(angle) ~= "number" then
      error("error[snes.world]: w:follow: world " .. w.name .. " loads with a view: pass the heading, w:follow(x, y, angle) (got "
        .. tostring(angle) .. ")")
    end
    local h = angle // 1 % 256
    if not (h >= 0 and h < 256) then error("error[snes.world]: w:follow: the angle is a finite number of 256ths of a turn (got " .. angle .. ")") end
    if not w.plane then error("error[snes.world]: w:follow: world " .. w.name .. " has a view: bind its Mode 7 layer first, w:bind(name, m7)") end
    if w.period then return followGenerated(w, x, y, h) end
    return followView(w, x, y, h)
  end
  if angle ~= nil then
    error("error[snes.world]: w:follow: world " .. w.name .. " has no view, so it takes no angle: w:follow(x, y), or load it with view")
  end
  if w.period then
    if not w.radius then return followGeneratedBackgrounds(w, x, y) end
    if not w.plane then error("error[snes.world]: w:follow: world " .. w.name .. " is generated: bind its Mode 7 layer first, w:bind(name, m7)") end
    return followGenerated(w, x, y, nil)
  end
  if w.plane then return followPlane(w, x, y) end
  if x < 0 or x > w.max_x or y < 0 or y > w.max_y then
    error("error[snes.world]: w:follow: the camera at " .. x .. ", " .. y .. " leaves the map: keep x in 0..w.max_x (" .. w.max_x
      .. ") and y in 0..w.max_y (" .. w.max_y .. "): clamp it")
  end
  for n = 1, 4 do
    if w.bound[n] then
      -- The layer's own camera, p/16 of the camera's (parallax); its last window is the last camera's.
      local p = w.parallax[n]
      local lx, ly = x * p // 16, y * p // 16
      local c0, c1, r0, r1 = window(w, lx, ly)
      local o0, o1, q0, q1 = window(w, w.x * p // 16, w.y * p // 16)
      if w.cut then
        for r = r0, r1 do __ppu_world_strip(w.id, n, c0, r, c1 - c0 + 1, false) end
      else
        -- The columns that came into view over the new rows, then the rows over the new columns, each in
        -- increasing order (a move past the window: all of it).
        if c0 < o0 then
          for c = c0, math.min(o0 - 1, c1) do __ppu_world_strip(w.id, n, c, r0, r1 - r0 + 1, true) end
        elseif c1 > o1 then
          for c = math.max(o1 + 1, c0), c1 do __ppu_world_strip(w.id, n, c, r0, r1 - r0 + 1, true) end
        end
        if r0 < q0 then
          for r = r0, math.min(q0 - 1, r1) do __ppu_world_strip(w.id, n, c0, r, c1 - c0 + 1, false) end
        elseif r1 > q1 then
          for r = math.max(q1 + 1, r0), r1 do __ppu_world_strip(w.id, n, c0, r, c1 - c0 + 1, false) end
        end
      end
      bg[n].scroll.x = lx % 1024
      bg[n].scroll.y = (ly - 1) % 1024
    end
  end
  local c0, c1, r0, r1 = window(w, x, y)
  w.x, w.y, w.c0, w.c1, w.r0, w.r1 = x, y, c0, c1, r0, r1
  w.cut = false
  w.sets = 0
end

-- A hard cut to map `name` (a same-owner load) with the camera at (x, y): the engine checks
-- everything and moves the bindings, then the world takes the map's fields, each bound background its
-- char base and screen size, and one follow draws every bound layer's whole window.
local function relocate(w: any, name: string, x: number, y: number, ...: any)
  -- Extra arguments go along so the engine refuses them.
  local id, wide, c1, c2, c3, c4 = __ppu_world_enter(w.id, name, x, y, ...)
  show(w, id)
  local chars = {c1, c2, c3, c4}
  for n = 1, 4 do
    if w.bound[n] then
      bg[n].char_base = chars[n]
      bg[n].screen_size = if wide then 1 else 0
    end
  end
end

-- A hard cut to another loaded map on the same backgrounds, the camera at (x, y), drawn whole.
local function enter(w: any, name: string, x: number, y: number, ...: any)
  relocate(w, name, x, y, ...)
  w.cut = true
  follow(w, x, y)
end

-- The kind (w.kinds number) of the tile under map pixel (x, y) on a layer, or 0 off the map.
local function tile(w: any, name: string, x: number, y: number): number
  local layer = w.layers[name]
  if layer == nil then error("error[snes.world]: w:tile: the map has no tile layer named " .. tostring(name)) end
  if w.period then
    -- Generated ground: the pixel's chunk picks its template, and the kind is the template's there.
    local size = w.period * 8
    x, y = x // 1 % size, y // 1 % size
    local pw, ph = w.cw * 8, w.ch * 8
    local t = template(w, x // pw, y // ph)
    return __ppu_world_kind(w.id, layer, t % w.columns * pw + x % pw, t // w.columns * ph + y % ph)
  end
  return __ppu_world_kind(w.id, layer, x, y)
end

-- The cell columns w:set may redraw between follows: a cartridge queues each for the next VBlank beside the
-- follow's strips (the cartridge compiler holds the same count).
local SET_RUNS = 16

-- The Tiled tile under map pixel (x, y) becomes tile id t of the layer's tileset, redrawn now if on screen.
local function set(w: any, name: string, x: number, y: number, t: number)
  local layer = w.layers[name]
  if layer == nil then error("error[snes.world]: w:set: the map has no tile layer named " .. tostring(name)) end
  local cx, cy, k = __ppu_world_set(w.id, layer, x // 1, y // 1, t)
  if w.cut then return end -- the next follow draws every bound layer whole
  for n = 1, 4 do
    if w.bound[n] and w.shows[n] == name then
      -- The cells this background's camera showed at the last follow get the new tile now.
      local p = w.parallax[n]
      local c0, c1, r0, r1 = window(w, w.x * p // 16, w.y * p // 16)
      local top, bottom = math.max(cy, r0), math.min(cy + k - 1, r1)
      for c = math.max(cx, c0), math.min(cx + k - 1, c1) do
        if top <= bottom then
          w.sets += 1
          if w.sets > SET_RUNS then
            error("error[snes.world]: w:set: this update redraws more than " .. SET_RUNS .. " columns of on-screen cells (a 16 px tile is two): "
              .. "a cartridge redraws at most " .. SET_RUNS .. " between follows, so spread the changes over several updates")
          end
          __ppu_world_strip(w.id, n, c, top, bottom - top + 1, true)
        end
      end
    end
  end
end

-- Each of a map's objects by its name, built once per load: false for a name two objects share.
local function named(objects: any): any
  local t = {}
  for _, o in objects do
    if t[o.name] == nil then t[o.name] = o else t[o.name] = false end
  end
  return t
end

-- The one object named `name` on the map w shows.
local function find(w: any, name: string): any
  local o = loads[w.id].named[name]
  if o == nil then error("error[snes.world]: w:find(\"" .. tostring(name) .. "\"): map " .. w.name .. " has no object named " .. tostring(name)) end
  if o == false then error("error[snes.world]: w:find(\"" .. tostring(name) .. "\"): map " .. w.name .. " has two or more objects named " .. tostring(name) .. ": give each its own name in Tiled") end
  return o
end

-- Loads Tiled map `name` with its options (radius, view, speed, period, pick, chunk) and returns its world.
local function load(name: string, options: any): any
  for k in options or {} do
    if k ~= "speed" and k ~= "radius" and k ~= "view" and k ~= "period" and k ~= "pick" and k ~= "chunk" then
      error("error[snes.world]: world.load(\"" .. tostring(name) .. "\"): the options are radius, view (and its speed), period, pick and chunk (got " .. tostring(k) .. ")")
    end
  end
  local period, pick = options and options.period, options and options.pick
  if (period == nil) ~= (pick == nil) or (pick ~= nil and type(pick) ~= "function") then
    error("error[snes.world]: world.load(\"" .. tostring(name) .. "\"): a generated world takes both period, its size in cells, and pick, "
      .. "a function (cx, cy) that returns each chunk's template: {period = 4096, pick = pick}")
  end
  local w = __ppu_world_load(name, options and options.speed, options and options.radius, options and options.view, period,
    options and options.chunk)
  w.pick = pick
  -- The camera's stops on a background world: its top left shows a 256x224 screen of the map.
  w.max_x, w.max_y = math.max(w.width * 8 - 256, 0), math.max(w.height * 8 - 224, 0)
  w.bound = {false, false, false, false}
  w.parallax = {16, 16, 16, 16}
  w.shows = {"", "", "", ""}
  w.sets = 0
  w.x, w.y, w.c0, w.c1, w.r0, w.r1 = 0, 0, 0, 0, 0, 0
  w.cut = true
  w.home = w.id
  w.scene = __ppu_scene_file or ""
  table.insert(worlds, w)
  loads[w.id] = {id = w.id, name = w.name, width = w.width, height = w.height, kinds = w.kinds, objects = w.objects, layers = w.layers, max_x = w.max_x, max_y = w.max_y, named = named(w.objects)}
  -- w's setup mark: a recompile keeps this run's entry, whose id is this run's, not a carried one.
  setmetatable(loads[w.id], getmetatable(w))
  w.bind, w.follow, w.tile, w.enter, w.find, w.set = bind, follow, tile, enter, find, set
  return w
end

-- Before a recompile, in the OLD run: one "scene<TAB>home<TAB>shown" line per world, in load order.
-- `s` cut at each `sep` character (the std string library here has no match or split).
local function split(s: string, sep: string): {string}
  local parts, from = {}, 1
  for i = 1, #s do
    if string.sub(s, i, i) == sep then
      table.insert(parts, string.sub(s, from, i - 1))
      from = i + 1
    end
  end
  table.insert(parts, string.sub(s, from))
  return parts
end

function __ppu_world_shown(): string
  local out = ""
  for _, w in worlds do out ..= w.scene .. "\t" .. loads[w.home].name .. "\t" .. w.name .. "\n" end
  return out
end

-- Called by the engine after a recompile carried the previous run's state, with the old run's
-- __ppu_world_shown: the author's camera came back after init() followed this run's, so the next
-- follow of each world draws its whole window there, and a world that had entered another map is
-- put back on it. Returns the worlds it could not put back, one line each.
function __ppu_world_hotswap(shown: string): string
  local lines = ""
  for _, w in worlds do w.cut = true end
  local taken = {}
  local rows = split(shown, "\n")
  for _, row in rows do
    local f = split(row, "\t")
    local scene, home, at = f[1], f[2], f[3]
    -- Matched by scene and home map, in load order, not list position: a reordered load keeps each
    -- world its own.
    local w, known = nil, false
    for _, o in worlds do
      if at ~= nil and o.scene == scene then
        if w == nil and not taken[o] and loads[o.home].name == home then w = o end
        if loads[o.home].name == at then known = true end
      end
    end
    if w ~= nil then taken[w] = true end
    if at ~= nil and at ~= home then
      if w == nil or not known then
        lines ..= "world " .. home .. " was showing " .. at .. ", which this load doesn't have\n"
      else
        -- A hot swap is not a game frame: the engine skips the screen-off check while this hook runs.
        local ok, err = pcall(relocate, w, at, 0, 0)
        if not ok then
          lines ..= "world " .. home .. " was showing " .. at .. ", which this load can't enter: " .. tostring(err) .. "\n"
        end
      end
    end
  end
  return lines
end

-- world.hash is math.hash under its old name (deprecated: call math.hash).
return {load = load, hash = math.hash}
