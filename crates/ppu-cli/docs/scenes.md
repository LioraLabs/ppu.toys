# Scenes

A game has parts that never run together: a title, the game itself, a boss
room. Scenes give each part the whole console. A scene is a module that
returns its `init` and `frame`; the program registers each one with
`scenes.add` and switches with `scenes.go`. Entering a scene is a cold boot,
like a SNES game starting a new stage: video memory and registers return
to power-on, the sound chip restarts, then the scene sets itself up.

A program that never calls `scenes` is one scene, the whole program, and
runs exactly as before.

## Write a scene

A scene module places its graphics and sound at file level, as any program
does, and returns its own `init` and `frame`. Keep them `local`: the engine
calls them through the table, not as globals.

```luau
-- title.luau
local sky = dma("sky", {char = 0x1000, map = 0x0000, pal = 0})

local function init()
  bg[1].char_base = sky.char
  bg[1].map_base = sky.map
  screen.main.bg1 = true
end

local function frame(t, f)
  bg[1].scroll.x = f
  if pad.a then
    scenes.go(GAME)
  end
end

return {init = init, frame = frame}
```

Another scene may use the same addresses: the two never share the console.

```luau
-- game.luau
local tiles = dma("hills", {char = 0x1000, map = 0x0000, pal = 0})
local kick = dma("kick")

local function init()
  bg[1].char_base = tiles.char
  bg[1].map_base = tiles.map
  screen.main.bg1 = true
  voice[0].sample, voice[0].pitch = kick.id, 0x1000
  voice[0].vol.l, voice[0].vol.r = 100, 100
  dsp.mvol = {l = 127, r = 127}
end

local function frame(t, f)
  if f == 0 then
    kon(0)
  end
end

return {init = init, frame = frame}
```

## Register scenes

Register each scene once, at file level, with its `require` written inside
the call. `scenes.add` returns the scene's handle. The program starts in the
first scene added.

```luau
-- main.luau
scenes.add(require("./title"))
GAME = scenes.add(require("./game"))
```

The handle is an ordinary value: keep it in a global, as above, so the
other scenes can reach it. A program of scenes has no global `frame()` or
`init()` of its own; each scene brings its own.

## Switch

`scenes.go(scene)` asks for a switch. The update that calls it finishes as
it is; the next update enters the scene:

1. The PPU returns to power-on: every register, OAM, VRAM and CGRAM, with
   `vram[]`, `cgram[]` and `bg[n].map` cells cleared.
2. The sound chip powers off and on: every voice stops, sound RAM holds the
   program's samples again, and the scene's `timer()` hooks and songs start
   over.
3. The scene's placements load: the `dma()` calls its module (and the
   modules it requires) made at file level.
4. The scene's `init()` runs. It may place more with `dma()` and start
   timers or songs, as setup can.
5. The scene's `frame(t, f)` runs with `t` and `f` counted from that
   update, so its first call sees `t = 0` and `f = 0`. Timer hooks see the
   same clock.

Lua globals and `sram` stay as they were: they are how one scene tells the
next what happened. Nothing else crosses a switch, so set registers in the
scene's `init()`, not at file level, where a cold boot would undo them.

```luau
best = math.max(best or 0, 120) -- in a scene's frame(): still set in the next scene
```

File-level `voice[]` and `dsp` writes are undone the same way; a `score{}`
at file level still plays after a switch. A module two scenes require
belongs to the scene that loads it first: require shared graphics from
`main.luau` instead, so they are the program's.

## What a scene owns

- **Placements, timers and songs** made while a scene's module loads belong
  to that scene, and so do those of the modules it requires first. Those
  made anywhere else, such as `main.luau`'s file level, belong to the whole
  program and load in every scene.
- **Placements conflict only within a scene.** Two scenes may place at the
  same VRAM and CGRAM addresses; two placements of one scene still may not
  overlap.
- **Sprite groups** start empty in each scene: a sprite drawn in the title
  never shows in the game.
- **Samples** are the program's: sound RAM holds every scene's samples, each
  at its own address.

## In the Studio

The Studio plays one timeline for the whole program. A scene's own `t` and
`f` start at its entry, while the scrubber keeps counting. Scrubbing back
to before the current scene began starts the program's first scene again at
that point, and its `scenes.go` enters the next scene as it did before. A
paused frame redrawn in place never switches, and an edit to the code
starts the program again in its first scene.
