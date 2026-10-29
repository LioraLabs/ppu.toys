-- Printable ASCII tiles, starting with space at tile zero.
local HUD_GLYPHS = {
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
function __ppu_hud_glyph(character)
  local tile = HUD_GLYPHS[character]
  assert(tile ~= nil, "HUD character is absent from the printable ASCII font")
  return tile
end

function __ppu_hud_bounds(map, x, y, columns, rows)
  assert(map.width == floor(map.width) and map.width >= 1 and map.width <= 32, "HUD width must be 1..32 cells")
  assert(map.height == floor(map.height) and map.height >= 1 and map.height <= 32, "HUD height must be 1..32 cells")
  assert(map.map == floor(map.map) and map.map >= 0 and map.map + (map.height - 1) * 32 + map.width <= 32768, "HUD map is outside VRAM")
  assert(x == floor(x) and y == floor(y) and columns == floor(columns) and rows == floor(rows), "HUD cell coordinates must be integers")
  assert(x >= 0 and y >= 0 and columns >= 0 and rows >= 0 and x + columns <= map.width and y + rows <= map.height, "HUD field is outside its map")
end

function hud_text(map, x, y, message, palette)
  local color = palette or map.palette
  __ppu_hud_bounds(map, x, y, #message, 1)
  assert(color == floor(color) and color >= 0 and color <= 7, "HUD palette must be 0..7")
  -- Validation rejects overflow first; bounded columns retain proofs across different callers.
  for column = x, min(x + #message, map.width) - 1 do
    local i = column - x + 1
    vram[map.map + y * 32 + column] = __ppu_hud_glyph(string.sub(message, i, i)) + color * 1024
  end
end

function hud_number(map, x, y, value, width, palette)
  local color = palette or map.palette
  __ppu_hud_bounds(map, x, y, width, 1)
  assert(width >= 1 and width <= 10, "HUD number width must be 1..10 cells")
  assert(value == floor(value) and value >= 0 and value <= 2147483647, "HUD number must be an integer in 0..2147483647")
  assert(color == floor(color) and color >= 0 and color <= 7, "HUD palette must be 0..7")
  local padding = __ppu_hud_glyph(map.padding or " ")
  local remaining = value
  for column = min(x + width, map.width) - 1, x, -1 do
    local tile = padding
    if remaining ~= 0 or column == x + width - 1 then
      local quotient = remaining // 10
      tile = 16 + remaining - quotient * 10
      remaining = quotient
    end
    vram[map.map + y * 32 + column] = tile + color * 1024
  end
  if remaining ~= 0 and map.wrap ~= true then
    for column = x, min(x + width, map.width) - 1 do
      vram[map.map + y * 32 + column] = 3 + color * 1024
    end
  end
end

function hud_clear(map, x, y, width, height)
  local left, top = x or 0, y or 0
  local columns, rows = width or map.width, height or map.height
  __ppu_hud_bounds(map, left, top, columns, rows)
  for row = top, min(top + rows, map.height) - 1 do
    for column = left, min(left + columns, map.width) - 1 do
      vram[map.map + row * 32 + column] = 0
    end
  end
end
