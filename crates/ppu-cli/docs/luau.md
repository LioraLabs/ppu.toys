# Luau and Lua

Toys are written in [Luau](https://luau.org), a typed language built on Lua 5.1.
Lua is still supported: every Lua toy keeps working, and this page is where that
is documented. The rest of these docs show Luau.

## Why Luau

- **Types catch mistakes before you run them.** Annotate a record or a helper and
  a Luau-aware editor flags a misspelled field or a missing argument. The engine
  removes the types before it runs your code, so they cost nothing.
- **Shorter everyday code.** You can write `score += 1`,
  `brightness = if pad.a then 8 else 15`, and `for _, e in enemies do`.
- **The surface says what you mean.** `makeBounded` and `makeFixed` give numbers
  their range and precision. `routine` spreads work over several frames.
  `lookup` and `bakeHdma` precompute math and scanline effects. The preview
  checks all of these as it runs, and the cartridge compiler uses the same
  hints to build a game that fits the console (see the end of this page).
- **Modules.** `require("./state")` loads another file once and returns its table.

## Write a toy in Luau

Name the files `.luau` (`main.luau`). `frame(t, f)` and `init()` are still
global functions; everything else can be `local`.

```luau
type Ball = {x: number, y: number, dx: number}

local ball: Ball = {x = 120, y = 100, dx = 2}

function init()
  screen.main.obj = true
  obj[0].on = true
end

function frame(t: number, f: number)
  ball.x += ball.dx
  if ball.x < 8 or ball.x > 240 then ball.dx = -ball.dx end
  obj[0].x, obj[0].y = ball.x, ball.y
  brightness = if pad.a then 8 else 15
end
```

Luau's `math.clamp`, `math.round`, `table.insert` and `table.freeze` are
available. The short Lua-era names `sin`, `floor`, `min` and so on still work,
but these docs write `math.sin` and `math.floor`.

## Numbers with a range · makeBounded and makeFixed

`makeBounded(initial, min, max)` is a whole number that must stay within
`min..max`. `makeFixed(initial, min, max)` is a fractional number stored in
steps of 1/1024. An optional fourth argument picks a coarser step: a power of
two from 1 to 1024, such as `16` for steps of 1/16. Both return plain numbers.

Use them where the value is made: a local, or a field in a table constructor.
The field keeps its range for every later assignment, through aliases and
helper calls.

```luau
local ship = {
  x = makeFixed(128, 8, 240),
  vx = makeFixed(0, -3, 3, 16),
  lives = makeBounded(3, 0, 3),
}

function init()
  screen.main.obj = true
  obj[0].on = true
end

function frame(t, f)
  local push = (if pad.right then 0.25 else 0) - (if pad.left then 0.25 else 0)
  ship.vx = math.clamp(ship.vx * 0.9 + push, -3, 3)
  ship.x = math.clamp(ship.x + ship.vx, 8, 240)
  obj[0].x = math.floor(ship.x)
end
```

An assignment outside the range is an error, not a clamp. Clamp or wrap the
value yourself, as above. A fractional value stored into a `makeBounded` field
is also an error.

Once a program uses `makeFixed`, all of its arithmetic follows fixed-point rules,
the way a cartridge computes. Fractional literals round to the nearest 1/1024.
`*` and `/` round toward zero, to 1/1024. Each store rounds down to that
field's step.

## Saving · saveData

`saveData({...})` makes the one record a toy keeps in battery-backed RAM, the
save memory on a cartridge. Its fields are `makeBounded(default, min, max)`
with whole-number literal bounds, or `true`/`false`. Read and assign them like
any field: there is nothing to load, store or check.

```luau
local save = saveData({
  hiscore = makeBounded(0, 0, 999999),
  stage = makeBounded(1, 1, 8),
  seen_intro = false,
})
local score = 0

function frame(t, f)
  if pad.a then score = math.min(999999, score + 10) end
  if score > save.hiscore then save.hiscore = score end
  if pad.start and not save.seen_intro then save.seen_intro = true end
  brightness = save.stage + 7
end
```

- The first time the toy runs, the fields take their defaults. After that, a
  reload keeps what the toy last assigned, and so does a power cycle on a
  cartridge.
- A save the toy cannot use starts from the defaults: a damaged one, or one
  written before the record changed (a field added, renamed or given new
  bounds).
- Fields keep their contracts: assigning outside `min..max` is an error.
- Bind the record once, at file level, and use it by its fields:
  `save.hiscore`, never `save` itself passed or aliased. A module can export
  it, `return {save = save}`.
- The preview keeps the save in this browser, one per toy. The Studio's
  Cartridge panel clears it, exports it as a `.srm` file and imports one. The
  file is the cartridge's own save memory, so it also works in emulators and
  on flash carts.

## Work over several frames · routine

`routine(body)` makes a handle for a function that can pause with `yield()`.
`yield(n)` waits `n` steps, and `yield()` waits one. Locals and loop positions
survive the wait, even inside a helper.

```luau
local flash = routine(function(times: number)
  for i = 1, times do
    brightness = 15
    yield(4)
    brightness = 6
    yield(4)
  end
  brightness = 15
end)

function frame(t, f)
  if pad.a then flash:start(3) end
  flash:step()
end
```

- `handle:start(...)` captures its arguments and returns `true`. If the
  routine is already running, it returns `false` and changes nothing.
- `handle:step()` runs the body to its next `yield` or its end. Call it once
  per update. Nothing runs between steps.
- `handle:running()` is true from a successful start until the body returns or
  is cancelled. `handle:cancel()` drops the invocation; writes already made
  stay.
- `yield` outside a routine is an error. `hdma()` callbacks and `lookup`
  functions cannot yield.

## Precompute math · lookup and bakeHdma

`lookup(grid, fn)` samples a pure function over a grid of inputs and returns a
function you call like the original. The grid's keys are the parameter names.
`{min, max}` samples every whole number from `min` to `max`, and
`{min, max, step}` samples more coarsely. A call uses the nearest sample, and
an input outside the grid is an error.

```luau
local orbit = lookup({angle = {0, 255}}, function(angle)
  local a = angle / 256 * 2 * math.pi
  return 128 + math.cos(a) * 64, 112 + math.sin(a) * 48
end)

function init()
  screen.main.obj = true
  obj[0].on = true
end

function frame(t, f)
  obj[0].x, obj[0].y = orbit(f % 256)
end
```

`bakeHdma(grid, first, last, fn)` does the same for a [scanline effect](scanlines.md).
The callback gets the scanline `y` first and then the grid's inputs. Calling
the result selects which sample runs this frame:

```luau
local ripple = bakeHdma({phase = {0, 31}}, 120, 223, function(y, phase)
  bg[1].scroll.x = math.floor(math.sin((y + phase * 2) / 8) * 4)
end)

function frame(t, f)
  ripple(f % 32)
end
```

Write the function inline. It must not change game state, read the pad or
yield. Each grid takes up to five inputs. Lookup results keep 1/1024 precision.

## Modules · require

A file loaded with `require("./name")` runs once, on its first `require`. It
loads `name.luau`, or else `name.lua`. Every `require` of that file returns
the same table. Export types with `export type`.

```luau
-- state.luau
export type Player = {x: number, lives: number}

local player: Player = {x = makeBounded(120, 0, 240), lives = makeBounded(3, 0, 3)}

return {player = player}
```

```luau
-- main.luau
local State = require("./state")

function init()
  screen.main.obj = true
  obj[0].on = true
end

function frame(t, f)
  local player = State.player
  if pad.left then player.x = math.max(0, player.x - 2) end
  if pad.right then player.x = math.min(240, player.x + 2) end
  obj[0].x = player.x
end
```

Files a toy lists but no file requires still run in the listed order and share
globals, as Lua toys always have.

## What Luau code cannot use yet

The engine runs a subset of Luau. Using one of these features is an error that
names it:

- `continue`, `const`, string interpolation (`` `x = {x}` ``), attributes, and
  condition bindings in `if`.
- `+=` and the other compound assignments on a target that calls a
  function, such as `t[next()] += 1` or `pick().x += 1`. Bind the called part
  to a local first:
  `local i = next(); t[i] += 1`.
- An `if` expression whose `then` value could be `false` or `nil`: `false`,
  `nil`, `not`, a comparison, `and` or `or`. Use an `if` statement instead.
  A name, field, index or call is accepted there
  (`local t = if i == 1 then a[1] else a[2]`) and must not be `false` or
  `nil` when chosen: if it is, the `else` value is used.

Types are not checked when the toy runs. They document your intent and help
an editor check it.

## Lua toys keep working

- Every file is read as Luau first. A file Luau cannot parse runs as Lua, so
  `.lua` files that use Lua 5.3's bitwise operators (`&`, `|`, `~`, `<<`,
  `>>`) still work. Luau has no bitwise operators: in a `.luau` file, use
  arithmetic such as `math.floor(x / 16) % 4`.
- A toy can mix `.lua` and `.luau` files. Lua globals are visible to Luau and
  the other way around.
- Studio-generated `ppuglobals.lua` stays Lua.
- The short math names, the flat-global style and `---@snes` annotations keep
  working in Lua files.

The same toy in Lua, for comparison:

```lua
local ball = {x = 120, y = 100, dx = 2}

function frame(t, f)
  ball.x = ball.x + ball.dx
  if ball.x < 8 or ball.x > 240 then ball.dx = -ball.dx end
  obj[0].x, obj[0].y = ball.x, ball.y
  if pad.a then brightness = 8 else brightness = 15 end
end
```

## How the cartridge compiler uses the hints

The Studio's Cart mode and the SNES cartridge compiler read the same Luau.
Each hint gives the compiler a fact it would otherwise have to guess:

- **`makeBounded` and `makeFixed`** set how much memory a value needs and how
  precise it is. Cart mode checks every assignment, so a value that leaves
  its range stops the program rather than wrapping on the console.
- **`lookup`** and **`bakeHdma`** become tables in the cartridge ROM. A call
  reads a precomputed answer instead of computing sines on a 3.58 MHz CPU.
- **`saveData`** puts the record in the cartridge's battery-backed RAM. Reading
  a field costs what any variable does; assigning one also writes it to the
  battery RAM.
- **`routine`** is how to spread long work over several updates when one
  update cannot fit it.
- **Types** are removed before compiling and do not change the game.

ppu.toys' "Run it on a SNES" chapter covers budgets, the language subset and
export. Lua toys reach the same compiler through `---@snes` annotations.
