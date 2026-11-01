# Save data

A Luau toy that should also save on a cartridge declares its save with
`saveData({...})`, a record kept in battery-backed RAM: see "Saving" on the
Luau and Lua page. This page is about `sram`, the free-form table the preview
keeps for any toy; a cartridge cannot hold it.

`sram` is a table that survives reloads, like a cartridge's battery-backed
RAM. Whatever the program leaves in it after a frame is saved automatically
and handed back the next time the toy runs, so `init()` can read it.

## Keep a high score

```luau
function init()
  sram.best = sram.best or 0
end

local score = 0

function frame(t, f)
  if pad.a then score += 1 end
  if score > sram.best then sram.best = score end
end
```

## What fits

`sram` holds numbers, strings, booleans, and nested tables whose keys are
strings or a dense `1..n` array. Anything else, such as a function or a table
with mixed keys, is a runtime error that names the offending path. The whole
table is capped at 32 KiB, a real cartridge's SRAM size.

## Where it lives

Saves are stored in the player's browser, one slot per published toy. In the
Studio a sketch has its own slot, so you can iterate on a game without losing
its progress; a fork starts with an empty cartridge. Reloading the program
keeps `sram`, so reset it by assigning fresh values in `init()` when you want
a clean slate.

See also: [Controller input](pad.md) for reading the buttons that earn a score.
