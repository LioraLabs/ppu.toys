---
id: snes.unproven_index
kind: diagnostic
title: A table key the compiler cannot prove in range keeps its check on the cartridge, which stops if the key leaves
codes: [snes.unproven_index]
---
`Give this key of T a range` means the compiler cannot show that a key stays inside the table's
rows, so the release ROM keeps the check on that line and stops there if the key ever leaves (see
[[snes.halt]] for how a stop is reported). The program still builds; the warning names the first such
access of each table.

Bound the key where it comes from: clamp it (`math.min(n, math.max(1, k))`), keep it in
[[makeBounded|`makeBounded(v, 1, n)`]], or guard the access with `if k >= 1 and k <= n then`. A loop
counter is bounded by its loop's limits, so clamp a limit that comes from a parameter or a table entry.
For a growing list (`table.insert`, `#t + 1`, reported as `Count T's entries yourself`), keep the count
in bounded state and write by index; the table is then sized from those keys and the check goes away. The frame counter `f` and the time `t` are known only to be non-negative, so `T[f]` keeps its check and `T[f % 64 + 1]` does not ([[wrap-with-modulo-or-mask-to-prove-a-range]]).

```luau
local xs = {}
local count = makeBounded(0, 0, 8)

function frame(t, f)
  if pad.a and count < 8 then
    count += 1
    xs[count] = f % 256
  end
  if count > 0 then
    bg[1].scroll.x = xs[count]
  end
end
```
