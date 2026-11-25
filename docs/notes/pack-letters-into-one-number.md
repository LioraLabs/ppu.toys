---
id: pack-letters-into-one-number
kind: recipe
title: Three initials pack into one bounded number, five bits each, first * 1024 + second * 32 + third
codes: []
---
A [[saveData]] field is one number, so a name of three letters 0..25 is stored five bits a letter:
`a * 1024 + b * 32 + c`, bounded `0..26425`. Unpack it with `// 1024`, `// 32 % 32` and `% 32`, and index a
list of letters (plus one: Luau lists start at 1). The divisors are powers of two, so a cartridge shifts and
masks where base 26 (`// 676`, `% 26`) would divide. The packed number is also what a
[[save-data-one-flat-record|high-score table]] keeps beside each score.

```luau
local LETTERS = {"A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M",
  "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z"}

local save = saveData({name = makeBounded(68, 0, 26425)}) -- ACE

local function spell(name: number): (string, string, string)
  return LETTERS[name // 1024 + 1], LETTERS[name // 32 % 32 + 1], LETTERS[name % 32 + 1]
end

function frame(t, f)
  if pad.pressed.a then save.name = 19 * 1024 + 22 * 32 + 13 end -- TWN
  local _a, _b, _c = spell(save.name)
end
```
