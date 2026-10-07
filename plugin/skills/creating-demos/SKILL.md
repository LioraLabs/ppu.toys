---
name: creating-demos
description: Create and edit uploadable ppu.toys demos and games with the standalone ppu CLI. Use Lua controller input, register and memory control, Mode 7, HDMA, sprites, windows, color math, and S-DSP music, then render, check, and pack a self-contained toy.
---

# Creating ppu.toys demos

Use the installed `ppu` CLI to create an editable project and a self-contained
`.ppu.json` the user can open in ppu.toys. No application checkout, private
examples repository, Node, WASM build, or server is needed.

## Start with the installed API

Run `ppu --version`, `ppu --help`, and `ppu docs registers`. This skill
requires the authoring commands introduced in ppu 0.1.0 (`new`, `render`,
`check`, `docs`). Install or update it with Cargo:

```sh
cargo install --git https://github.com/LioraLabs/ppu.toys.git --locked ppu-cli
```

Read `ppu docs cli` for the project format and commands. Read only the API
topics needed for the effect: `backgrounds`, `sprites`, `mode7`, `scanlines`,
`windows`, `color-math`, `sources`, `dma`, `display`, or `pad`. These docs
ship inside the executable and match its engine version. Do not invent APIs
or ask the user to clone a development repository to find documentation.

## Exploit the PPU

ppu.toys emulates the SNES PPU graphics pipeline and exposes its implemented
register controls through a Lua DSL. Think in registers, tile memory,
palettes, sprites, compositing, and changes along the scanline. This is much
more than a pixel canvas: take as much advantage of SNES effects as the demo
allows, especially **Mode 7 and HDMA**, and combine them with windows, color
math, palette animation, mosaic, sprite priorities, and parallax. Prefer
visible use of these capabilities over a static image with a few particles.

The present engine supports background modes 0–4 and Mode 7. It does not yet
model modes 5/6, interlace, overscan, or read-only counter registers; do not
claim every physical SNES register or hardware feature is emulated. The Lua
surface gives register-level control over the supported pipeline, with raw
aliases for screen/window/color-math registers and friendly names elsewhere.

| Surface | What it controls |
| --- | --- |
| `init()`, `frame(t, f)` | Setup, then animation from seconds and frame number |
| `pad.up/down/left/right`, `pad.a/b/x/y/l/r/start/select` | First SNES controller; booleans refreshed before each frame |
| `mode`, `brightness`, `force_blank`, `mosaic` | Display and background configuration |
| `bg[1]` through `bg[4]` | Character/map bases, tilemaps, scrolling, layer settings |
| `m7.a/b/c/d`, `m7.cx/cy`, `m7.wrap`, `m7.extbg` | Affine sampling, perspective via scanlines, and Mode 7 priority |
| `obj[0]` through `obj[127]`, `obj.char_base`, `obj.size_sel` | OAM sprites, metasprites, priority, flips, size, and tiles |
| `screen.main`, `screen.sub`, `win`, `color` | Layer designation, masking, add/subtract/half color compositing |
| `hdma(first, last, function(y) ... end)` | Inclusive per-line register and palette changes; `scanline` is an alias |
| `vram[address]`, `cgram[index]` | Direct tile/map words and palette colors |
| `dma("source", options)` | Place imported graphics during setup; returns addresses/cells |

Read the topic docs for the exact field names and limits. `hdma` cannot run
DMA or rewrite frame-global OAM. VRAM addresses are words, not bytes.
Unassigned scanline fields keep their frame-wide values.

Useful combinations include per-line Mode 7 matrices for a perspective
floor or vortex; window bounds shaped per line into an iris; scanline
scrolling for reflections/refraction; and selective color math with palette
cycling for lighting. Follow the user's subject and style; there is no
required story, scene count, palette, or duration.

[recipes.md](recipes.md) has worked recipes for the effects that make a
demo look impossible (per-line mode split, scroll-mirror reflections,
window-shaped shadows and light, additive sprites, line-budgeted sprite
formations) and the Lua and hardware gotchas behind them. Read the recipes
for the effects you plan to use before writing them.

## Author locally

```sh
ppu new my-demo
ppu render my-demo --at 0,2,4,6 -o previews
```

The starter already contains a PNG source and working Mode 7 + HDMA Lua.
Edit or replace them. Create artwork with the available drawing tools, or
encode indexed tile/palette data in Lua. `ppu` imports PNGs; it is not an
image-generation tool. Preserve asset provenance when reusing supplied art.

For original art, write a deterministic generator in the project (for
example `build_art.py` with a fixed seed, using whatever image library is
installed) that writes every PNG under `assets/`, plus any generated Lua
tables (landing spots, palettes, score patterns). Rerunning it rebuilds the
project; the art can be revised as code. Draw at native scale with a
deliberate, limited palette per layer: the importer quantizes to 16-colour
sub-palettes, so plan colours rather than let it guess. Layers that will be
recoloured from Lua can be drawn as palette indices and coloured in
`main.lua`.

The directory's `ppu.json` lists ordered Lua files (including `main.lua`) and
sources. Lua files share globals; use locals or clearly named shared tables.
A PNG source entry looks like:

```json
{"name":"floor","kind":"m7","file":"assets/floor.png","options":{}}
```

The CLI converts PNGs on every load, so editing the PNG is enough. Source
kinds are `bg`, `sheet`, `obj`, and `m7`; use `ppu docs sources` and
`ppu docs dma` for format and placement. For generated Lua art use
`"sources": []`. Imported binary `payload` entries remain supported; never
put raw PNG bytes in a payload field. Keep original PNGs: unpacking yields
converted PPU data, not the original art.

Animate native tiles, sprites, registers, and palettes. Load static scene
art when needed and stream bounded sprite poses; do not software-render and
upload a whole new screen each frame. Budget VRAM ranges and protect font
and sprite banks from overlapping writes. There are 128 OAM entries but
only 32 sprites / 34 eight-pixel sprite slivers per scanline. Leave room for
readable text when used.

For timeline performances, derive motion from time and stable hashes, clear
and rebuild active OAM, and restore registers used across scene changes.
Avoid accumulated motion that breaks seeking. Scene-upload caching is fine
when direct and backward seeks produce the same picture. Keep motion
controls and reduced-motion behavior where useful.

Studio markers use a declarative `timeline.lua`, for example:

```lua
-- timeline: end=32 in=0 out=32 loop=true
-- loop markers: in=start out=loop_end
markers = { start = 0, change = 16, loop_end = 32 }
```

Drive animation from those timings. Match the CLI check duration/loop flags
to the timeline; the CLI does not infer them from Lua.

## Interactive games

Games use the same Lua files and packed format as demos. Read `ppu docs pad`
for controller input and keyboard/gamepad mappings. `pad` fields are true
while held; keep the previous value to detect a single press for actions
such as jumping, firing, or starting a game. Keep positions, velocities,
collisions, scores, and game state in Lua between frames. The PPU renders
the result; gameplay logic belongs to the toy.

In Studio, click the output to focus keyboard input. Arrow keys are the
D-pad; Z/X are B/A, A/S are Y/X, Q/W are L/R, Enter is Start, and Shift is
Select. Standard-mapping gamepads are supported too. Include the controls
with the game so players can discover them.

Stateful games need not support arbitrary seeks or a fixed timeline loop.
Use ordinary sequential `ppu check --duration ...`, and test movement,
button edges, collisions, and restart behavior interactively in Studio.
The current CLI supplies released input and has no input-replay command:
headless checks can exercise the idle/attract state but do not prove gameplay.
Give a game that waits for Start a visible title/idle screen for previews.

## Inspect, check, deliver

For a 32-second looping piece, for example:

```sh
ppu render my-demo --at 0,8,15.9833,16,16.0167,24,31.9833 -o previews
ppu check my-demo --duration 32 --seek 8,16,24 --loop 32
ppu pack my-demo -o my-demo.ppu.json
ppu check my-demo.ppu.json --duration 32 --seek 8,16,24 --loop 32
```

Inspect the actual PNGs. Fix clipping, unreadable text, empty scenes, and
incorrect motion direction; a successful check is not visual review.
Review like an art director, not a test runner: render a contact sheet
across the whole piece, name what is weak (banding, sparse or illegible
shapes, cramped composition, jitter, dead air, flashes that run long), fix
it, and render again. Plan on at least three passes; that loop, not the
first draft, is what makes a demo look authored.
`render` plays sequentially from zero and writes native 256×224 images.
Use integer nearest-neighbor scaling if enlarging previews.

Check the entire performance, including transitions and dense sprite moments.
`check` reports Lua errors with file/line/frame/time, source budgets, and
sprite overflow. Review importer reports for quantization or cropped data.
Use `--allow-overflow` only for an intentional effect. Seek/loop checks
compare actual pixels and are optional for stateful games. Times are sampled
at 60 fps. `pack` alone does not execute Lua.

Return the editable directory, packed upload file, rendered samples, and
check results. Open the packed file in Studio when available for a final
compatibility check. Publishing needs user authorization.

## Sound

The S-DSP is emulated too. Read `ppu docs audio` before adding sound.
`bank("piano")` and friends place built-in instruments and drums with no
uploads; `song{ tempo, steps, tracks }` sequences them on the audio timer,
one voice per track; `dsp.echo` adds the chip's echo. Pattern strings are
easy to generate from your art script, so a through-composed score can live
next to the art (see the finale recipe in [recipes.md](recipes.md)). Keep
`dsp.mvol` below full when many voices stack, and set `dsp.echo` in
`init()` so sample placement sees the echo budget.

Read the song handle (`step`, `beat`) in `frame()` to sync visuals to the
music: spawn sprites on notes, land a reveal on a downbeat. A `song{}`
loops; call `stop()` from `frame()` for a one-pass piece. `ppu check` runs
the audio but the CLI does not encode it: listen in Studio, and tell the
user you have not heard it if you cannot.
