--!strict
-- @ppu/hud: a retained background HUD. Static text is written once; a field or line is a record
-- whose set costs one compare when the value is unchanged, and nothing runs at frame end.
-- local hud = require("@ppu/hud"); local score = hud.field(HUD, 7, 1, 6); score:set(points)
-- HUD is {map = 0x7000, width = 32, height = 4, palette = 7, padding = "0", priority = 1}.
-- The descriptor also says how text sits and how big its glyphs are; a second descriptor for the same map
-- (a centred one, a big font) is how one map shows both:
-- - align = "center" or "right": hud.text aligns its text in the cells from x to the map's right edge
--   (x = 0 centres it on the map), a line aligns each text it shows in its width. Fields are right-aligned
--   numbers already and refuse align.
-- - glyph = "8x16" or "16x16", font = the font's first tile (0 when absent): each character is 1x2 or 2x2
--   map cells, laid out as a 128-pixel-wide sheet of 8x16 or 16x16 cells in ASCII order from space, which
--   is how dma() places a font image drawn that way. x and y stay map cells; a field's or line's width
--   counts characters.
-- The cartridge compiles a plain copy of this file for a program that never names glyph, font or align
-- (the cartridge compiler's own copy of this file): lines ending in --@full are left out and each `--@plain ` line's code is used, so
-- those features cost the program nothing. The preview runs this file as written; the two agree on plain descriptors.

-- Printable ASCII tiles, starting with space at tile zero.
local GLYPH = {
  [" "]=0, ["!"]=1, ["\""]=2, ["#"]=3, ["$"]=4, ["%"]=5, ["&"]=6, ["'"]=7,
  ["("]=8, [")"]=9, ["*"]=10, ["+"]=11, [","]=12, ["-"]=13, ["."]=14, ["/"]=15,
  ["0"]=16, ["1"]=17, ["2"]=18, ["3"]=19, ["4"]=20, ["5"]=21, ["6"]=22, ["7"]=23,
  ["8"]=24, ["9"]=25, [":"]=26, [";"]=27, ["<"]=28, ["="]=29, [">"]=30, ["?"]=31,
  ["@"]=32, ["A"]=33, ["B"]=34, ["C"]=35, ["D"]=36, ["E"]=37, ["F"]=38, ["G"]=39,
  ["H"]=40, ["I"]=41, ["J"]=42, ["K"]=43, ["L"]=44, ["M"]=45, ["N"]=46, ["O"]=47,
  ["P"]=48, ["Q"]=49, ["R"]=50, ["S"]=51, ["T"]=52, ["U"]=53, ["V"]=54, ["W"]=55,
  ["X"]=56, ["Y"]=57, ["Z"]=58, ["["]=59, ["\\"]=60, ["]"]=61, ["^"]=62, ["_"]=63,
  ["`"]=64, ["a"]=65, ["b"]=66, ["c"]=67, ["d"]=68, ["e"]=69, ["f"]=70, ["g"]=71,
  ["h"]=72, ["i"]=73, ["j"]=74, ["k"]=75, ["l"]=76, ["m"]=77, ["n"]=78, ["o"]=79,
  ["p"]=80, ["q"]=81, ["r"]=82, ["s"]=83, ["t"]=84, ["u"]=85, ["v"]=86, ["w"]=87,
  ["x"]=88, ["y"]=89, ["z"]=90, ["{"]=91, ["|"]=92, ["}"]=93, ["~"]=94,
}

-- The smallest value too wide for a field of each width.
local LIMIT = {10, 100, 1000, 10000, 100000, 1000000, 10000000, 100000000, 1000000000}

-- A character's width and height in map cells. --@full
local function glyph_w(map: any): number --@full
  if map.glyph == "16x16" then return 2 end --@full
  return 1 --@full
end --@full

local function glyph_h(map: any): number --@full
  if map.glyph == "8x16" or map.glyph == "16x16" then return 2 end --@full
  return 1 --@full
end --@full

-- The top-left tile of glyph g: the row-major order of a 16-tile-wide sheet of w x h tile cells. --@full
local function corner(font: number, w: number, h: number, g: number): number --@full
  if w == 2 then return font + g % 8 * 2 + g // 8 * 32 end --@full
  if h == 2 then return font + g % 16 + g // 16 * 32 end --@full
  return font + g --@full
end --@full

-- Checks the cells x..x+width-1 of rows y..y+rows-1 against the map and returns the tile attributes to use: the
-- palette (bits 10-12), the map's tile priority (bit 13) and the font's first tile.
local function check_cells(map: any, x: number, y: number, width: number, rows: number, palette: number?): number --@full
--@plain local function check_cells(map: any, x: number, y: number, width: number, palette: number?): number
  if map.width ~= map.width // 1 or map.width < 1 or map.width > 32 then error("error[snes.hud]: HUD width must be 1..32 cells") end
  if map.height ~= map.height // 1 or map.height < 1 or map.height > 32 then error("error[snes.hud]: HUD height must be 1..32 cells") end
  if map.map ~= map.map // 1 or map.map < 0 or map.map + (map.height - 1) * 32 + map.width > 32768 then
    error("error[snes.hud]: HUD map is outside VRAM")
  end
  if x ~= x // 1 or y ~= y // 1 or width ~= width // 1 then error("error[snes.hud]: HUD cell coordinates must be integers") end
  if x < 0 or y < 0 or y >= map.height or y + rows > map.height or x + width > map.width then error("error[snes.hud]: HUD field is outside its map") end --@full
--@plain   if x < 0 or y < 0 or y >= map.height or x + width > map.width then error("error[snes.hud]: HUD field is outside its map") end
  local color = palette or map.palette or 0
  if color ~= color // 1 or color < 0 or color > 7 then error("error[snes.hud]: HUD palette must be 0..7") end
  local priority = map.priority or 0
  if priority ~= 0 and priority ~= 1 then error("error[snes.hud]: HUD priority is the map's tile priority, 0 or 1") end
  local g, a = map.glyph, map.align --@full
  if g ~= nil and g ~= "8x8" and g ~= "8x16" and g ~= "16x16" then error("error[snes.hud]: HUD glyph must be \"8x8\", \"8x16\" or \"16x16\"") end --@full
  if a ~= nil and a ~= "left" and a ~= "center" and a ~= "right" then error("error[snes.hud]: HUD align must be \"left\", \"center\" or \"right\"") end --@full
  local font = map.font or 0 --@full
  -- The last glyph's last tile: "~" (94) at its sheet cell. --@full
  local reach = if map.glyph == "16x16" then 381 elseif map.glyph == "8x16" then 190 else 94 --@full
  if font ~= font // 1 or font < 0 or font + reach > 1023 then error("error[snes.hud]: HUD font must be a tile from 0 that leaves room for every glyph") end --@full
  return color * 1024 + priority * 8192 + font --@full
--@plain   return color * 1024 + priority * 8192
end

-- The cells before a text of `cells` cells in a span of `span`, as the map's align says. --@full
local function offset(map: any, span: number, cells: number): number --@full
  if cells > span then return 0 end --@full
  if map.align == "center" then return (span - cells) // 2 end --@full
  if map.align == "right" then return span - cells end --@full
  return 0 --@full
end --@full

-- Writes static text at cell (x, y), aligned and sized as the descriptor says: a label written once.
local function text(map: any, x: number, y: number, s: string, palette: number?)
  local w, h = glyph_w(map), glyph_h(map) --@full
  local left = x + offset(map, map.width - x, #s * w) --@full
  local tint = check_cells(map, left, y, #s * w, h, palette) --@full
--@plain   local tint = check_cells(map, x, y, #s, palette)
  -- Every character is checked before the first cell is written, so a bad one leaves the map alone.
  for i = 1, #s do
    local g = GLYPH[string.sub(s, i, i)]
    if g == nil then error("error[snes.hud]: HUD character is absent from the printable ASCII font") end
  end
  local at = map.map + y * 32 + left --@full
--@plain   local at = map.map + y * 32 + x
  for i = 1, #s do
    local g = GLYPH[string.sub(s, i, i)]
    local t = corner(tint, w, h, g) --@full
    if w == 2 then --@full
      vram[at + i * 2 - 2] = t --@full
      vram[at + i * 2 - 1] = t + 1 --@full
      vram[at + i * 2 + 30] = t + 16 --@full
      vram[at + i * 2 + 31] = t + 17 --@full
    else --@full
      vram[at + i - 1] = t --@full
      if h == 2 then vram[at + i + 31] = t + 16 end --@full
    end --@full
--@plain     vram[at + i - 1] = g + tint
  end
end

-- Rewrites the cells from the units cell leftwards and stops once the digits still to come are
-- equal in the new and the shown value. A cell rewritten with its own tile costs no upload.
local function field_set(f: any, v: number)
  if v == f.shown then return end
  local no = f.shown
  if v ~= v // 1 or v < 0 or v >= f.limit then
    error("error[snes.hud]: HUD field value must be an integer that fits the field's width")
  end
  f.shown = v
  local at, zero, blank, w, h = f.at, f.zero, f.blank, f.w, f.h --@full
--@plain   local at, zero, blank = f.at, f.zero, f.blank
  for i = 0, f.width - 1 do
    local d = v % 10
    v = v // 10
    no = no // 10
    -- Digits are glyphs 16..25: one run on a sheet 16 glyphs wide, two rows on a 16x16 one. --@full
    local t = if i > 0 and d == 0 and v == 0 then blank elseif w == 2 then zero + d % 8 * 2 + d // 8 * 32 else zero + d --@full
    if w == 2 then --@full
      vram[at - i * 2] = t --@full
      vram[at - i * 2 + 1] = t + 1 --@full
      vram[at - i * 2 + 32] = t + 16 --@full
      vram[at - i * 2 + 33] = t + 17 --@full
    else --@full
      vram[at - i] = t --@full
      if h == 2 then vram[at - i + 32] = t + 16 end --@full
    end --@full
--@plain     if i > 0 and d == 0 and v == 0 then vram[at - i] = blank else vram[at - i] = zero + d end
    if v == no then break end
  end
end

-- A number field `width` cells wide (1 to 9), right-aligned; its set redraws only the digits that changed.
local function field(map: any, x: number, y: number, width: number, palette: number?): any
  if width ~= width // 1 or width < 1 or width > 9 then error("error[snes.hud]: HUD field width must be 1..9 cells") end
  if map.align ~= nil and map.align ~= "left" then error("error[snes.hud]: HUD field digits are right-aligned in its width: give the field a descriptor without align") end --@full
  local w, h = glyph_w(map), glyph_h(map) --@full
  local tint = check_cells(map, x, y, width * w, h, palette) --@full
  local zero = corner(tint, w, h, 16) --@full
  local blank = corner(tint, w, h, 0) --@full
--@plain   local tint = check_cells(map, x, y, width, palette)
--@plain   local zero = 16 + tint
--@plain   local blank = tint
  if map.padding == "0" then
    blank = zero
  elseif map.padding ~= nil and map.padding ~= " " then
    error("error[snes.hud]: HUD padding must be \" \" or \"0\"")
  end
  local f = {at = map.map + y * 32 + x + (width - 1) * w, width = width, limit = LIMIT[width], zero = zero, blank = blank, w = w, h = h, shown = -1, set = field_set} --@full
  -- Drawn as 0 with its padding: every cell written once.
  field_set(f, 0) --@full
  return f --@full
--@plain   local at = map.map + y * 32 + x + width - 1
--@plain   for i = 1, width - 1 do
--@plain     vram[at - i] = blank
--@plain   end
--@plain   vram[at] = zero
--@plain   return {at = at, width = width, limit = LIMIT[width], zero = zero, blank = blank, shown = 0, set = field_set}
end

-- Shows the first `count` characters of `s` (all of them when count is nil), placed in the line's width as its
-- map's align says: writes their glyphs, then blanks the cells the previous text covered beyond them. The same
-- text shown further (typing) keeps its place and writes only the new cells.
local function line_set(l: any, s: string, count: number?)
  local n = count or #s
  if s == l.shown and n == l.count then return end
  if #s > l.width then error("error[snes.hud]: HUD line text is wider than its cells") end
  if n ~= n // 1 or n < 0 or n > #s then error("error[snes.hud]: HUD line count must be an integer from 0 to the text's length") end
  local w, h, blank, tint = l.w, l.h, l.blank, l.tint --@full
  local was, before = l.count * w, l.off --@full
  local base = l.at --@full
  local from = 1 --@full
  local off = before --@full
  if s == l.shown and n > l.count then --@full
    from = l.count + 1 --@full
  elseif l.align == 1 then --@full
    off = (l.width - #s) * w // 2 --@full
  elseif l.align == 2 then --@full
    off = (l.width - #s) * w --@full
  else --@full
    off = 0 --@full
  end --@full
  -- Never true (check_cells kept the line in a row of 32 cells; an empty right-aligned text sits at its end, 32): it --@full
  -- bounds the offset for the cartridge's VBlank plan. --@full
  if off < 0 or off > 32 then error("error[snes.hud]: HUD line text is wider than its cells") end --@full
  for i = from, n do --@full
    local g = GLYPH[string.sub(s, i, i)] --@full
    if g == nil then error("error[snes.hud]: HUD character is absent from the printable ASCII font") end --@full
    local t = corner(tint, w, h, g) --@full
    if w == 2 then --@full
      vram[base + off + i * 2 - 2] = t --@full
      vram[base + off + i * 2 - 1] = t + 1 --@full
      vram[base + off + i * 2 + 30] = t + 16 --@full
      vram[base + off + i * 2 + 31] = t + 17 --@full
    else --@full
      vram[base + off + i - 1] = t --@full
      if h == 2 then vram[base + off + i + 31] = t + 16 end --@full
    end --@full
  end --@full
  -- The previous text's cells the new one does not cover. --@full
  local now = n * w --@full
  for c = before, before + was - 1 do --@full
    if c < off or c >= off + now then --@full
      vram[base + c] = blank --@full
      if h == 2 then vram[base + c + 32] = blank end --@full
    end --@full
  end --@full
  l.shown, l.count, l.off = s, n, off --@full
--@plain   local was = l.count
--@plain   local base, tint = l.at - 1, l.tint
--@plain   local from = if s == l.shown and n > was then was + 1 else 1
--@plain   for i = from, n do
--@plain     local g = GLYPH[string.sub(s, i, i)]
--@plain     if g == nil then error("error[snes.hud]: HUD character is absent from the printable ASCII font") end
--@plain     vram[base + i] = g + tint
--@plain   end
--@plain   for i = n + 1, was do
--@plain     vram[base + i] = tint
--@plain   end
--@plain   l.shown, l.count = s, n
end

-- A line starts blank without drawing: its cells hold whatever the map already has until the first set.
local function line(map: any, x: number, y: number, width: number, palette: number?): any
  if width ~= width // 1 or width < 1 then error("error[snes.hud]: HUD line width must be at least 1 cell") end
  local w, h = glyph_w(map), glyph_h(map) --@full
  local tint = check_cells(map, x, y, width * w, h, palette) --@full
  local align = 0 --@full
  if map.align == "center" then align = 1 elseif map.align == "right" then align = 2 end --@full
  return {at = map.map + y * 32 + x, width = width, tint = tint, blank = corner(tint, w, h, 0), w = w, h = h, align = align, off = 0, shown = "", count = 0, set = line_set} --@full
--@plain   local tint = check_cells(map, x, y, width, palette)
--@plain   return {at = map.map + y * 32 + x, width = width, tint = tint, shown = "", count = 0, set = line_set}
end

return {text = text, field = field, line = line}
