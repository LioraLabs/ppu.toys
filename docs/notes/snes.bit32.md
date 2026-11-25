---
id: snes.bit32
kind: diagnostic
title: bit32 is six two-operand calls a cartridge lowers to integer operators, not Luau's whole library
codes: [snes.bit32]
---
A cartridge's `bit32` is `band`, `bor`, `bxor`, `lshift` and `rshift` with two operands each, and
`bnot` with one; each becomes one integer operator. Any other `bit32` function is refused, and so is
a call with more or fewer operands: `error[snes.bit32]: bit32.band takes 2 arguments`. The preview
refuses the wrong count the same way.

The values are signed whole numbers with no 32-bit wrap (`bnot(0)` is -1). A cartridge also needs a
constant shift count from 0 to 30, and a right shift of a value it can prove is not negative; for a
value that may be negative, `x // 4` is the arithmetic shift. Nest calls for more operands.

```luau
function frame(t, f)
  local flags = bit32.band(bit32.band(f, 0xff), 0x0f)
  brightness = bit32.rshift(flags, 1) + 8
end
```
