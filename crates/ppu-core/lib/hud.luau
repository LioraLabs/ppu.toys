--!strict
-- @ppu/hud: a retained background HUD. Static text is written once; a field or line is a record
-- whose set costs one compare when the value is unchanged, and nothing runs at frame end.
-- local hud = require("@ppu/hud"); local score = hud.field(HUD, 7, 1, 6); score:set(points)
-- HUD is {map = 0x7000, width = 32, height = 4, palette = 7, padding = "0"}.

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

-- Checks the cells x..x+width-1 of row y against the map and returns the palette to use.
local function check_cells(map: any, x: number, y: number, width: number, palette: number?): number
  if map.width ~= map.width // 1 or map.width < 1 or map.width > 32 then error("HUD width must be 1..32 cells") end
  if map.height ~= map.height // 1 or map.height < 1 or map.height > 32 then error("HUD height must be 1..32 cells") end
  if map.map ~= map.map // 1 or map.map < 0 or map.map + (map.height - 1) * 32 + map.width > 32768 then
    error("HUD map is outside VRAM")
  end
  if x ~= x // 1 or y ~= y // 1 or width ~= width // 1 then error("HUD cell coordinates must be integers") end
  if x < 0 or y < 0 or y >= map.height or x + width > map.width then error("HUD field is outside its map") end
  local color = palette or map.palette or 0
  if color ~= color // 1 or color < 0 or color > 7 then error("HUD palette must be 0..7") end
  return color
end

local function text(map: any, x: number, y: number, s: string, palette: number?)
  local color = check_cells(map, x, y, #s, palette)
  -- Every character is checked before the first cell is written, so a bad one leaves the map alone.
  for i = 1, #s do
    local g = GLYPH[string.sub(s, i, i)]
    if g == nil then error("HUD character is absent from the printable ASCII font") end
  end
  local at = map.map + y * 32 + x
  local tint = color * 1024
  for i = 1, #s do
    local g = GLYPH[string.sub(s, i, i)]
    vram[at + i - 1] = g + tint
  end
end

-- Rewrites the cells from the units cell leftwards and stops once the digits still to come are
-- equal in the new and the shown value. A cell rewritten with its own tile costs no upload.
local function field_set(f: any, v: number)
  if v == f.shown then return end
  local no = f.shown
  if v ~= v // 1 or v < 0 or v >= f.limit then
    error("HUD field value must be an integer that fits the field's width")
  end
  f.shown = v
  local at, zero, blank = f.at, f.zero, f.blank
  for i = 0, f.width - 1 do
    local d = v % 10
    v = v // 10
    no = no // 10
    if i > 0 and d == 0 and v == 0 then vram[at - i] = blank else vram[at - i] = zero + d end
    if v == no then break end
  end
end

local function field(map: any, x: number, y: number, width: number, palette: number?): any
  if width ~= width // 1 or width < 1 or width > 9 then error("HUD field width must be 1..9 cells") end
  local color = check_cells(map, x, y, width, palette)
  local blank = color * 1024
  if map.padding == "0" then
    blank = 16 + color * 1024
  elseif map.padding ~= nil and map.padding ~= " " then
    error("HUD padding must be \" \" or \"0\"")
  end
  local at = map.map + y * 32 + x + width - 1
  local zero = 16 + color * 1024
  for i = 1, width - 1 do
    vram[at - i] = blank
  end
  vram[at] = zero
  return {at = at, width = width, limit = LIMIT[width], zero = zero, blank = blank, shown = 0, set = field_set}
end

-- Writes the new glyphs, then blanks the cells the previous text covered beyond them.
local function line_set(l: any, s: string)
  if s == l.shown then return end
  if #s > l.width then error("HUD line text is wider than its cells") end
  local old = l.shown
  local base, tint = l.at - 1, l.tint
  for i = 1, #s do
    local g = GLYPH[string.sub(s, i, i)]
    if g == nil then error("HUD character is absent from the printable ASCII font") end
    vram[base + i] = g + tint
  end
  for i = #s + 1, #old do
    vram[base + i] = tint
  end
  l.shown = s
end

-- A line starts blank without drawing: its cells hold whatever the map already has until the first set.
local function line(map: any, x: number, y: number, width: number, palette: number?): any
  local color = check_cells(map, x, y, width, palette)
  if width < 1 then error("HUD line width must be at least 1 cell") end
  return {at = map.map + y * 32 + x, width = width, tint = color * 1024, shown = "", set = line_set}
end

return {text = text, field = field, line = line}
