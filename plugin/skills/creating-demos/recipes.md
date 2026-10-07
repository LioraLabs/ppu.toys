# Effect recipes

Each recipe is one hardware idea pushed until it carries a scene. Combine
them; most good demos use three or four. Field names are the Lua DSL's; read
the matching `ppu docs` topic for exact semantics.

## Per-line mode split: Mode 7 floor under a tile-layer sky

Mode 7 has one layer, so a flight scene looks bare. `mode` is per-scanline:
set `mode = 1` inside an `hdma()` hook for the lines above the horizon and
`mode = 7` below it. Above, use real tile layers (a cloud ceiling, a far
bank with the sun) that scroll with the heading; below, give each line its
own floor matrix (`mode7_floor()` or your own). Mode 7's fixed layout takes
VRAM `0x0000–0x3FFF`; place the sky layers and sprites above it.

- A per-line `cgram[0]` gradient behind both halves sells the sky.
- Haze: per-line colour math near the horizon. Half-add on the farthest
  rows blends 50/50 toward a horizon-glow COLDATA; a fading add nearer in.
- Smooth motion: M7X/M7Y are whole texels, so a slow camera steps. Holding
  `m7.cx = m7.cy = 0` and moving the camera through per-line scroll and the
  M7B/M7D terms gives sub-texel placement.
- Close a loop exactly: fly a course whose displacement over the loop is a
  multiple of the 1024-texel plane, and repeat the texture across the PNG so
  wrap mode 0 is seamless.

## Reflections from scroll: the mirror lake

Nothing below the horizon is drawn. On each water line `y`, set every
layer's `scroll.y` so it samples the mirrored row:
`r = 2 * horizon - 1 - y`, `bg[n].scroll.y = r - y` (plus the layer's own
scroll). Then:

- Ripples: a per-line `scroll.x` offset whose amplitude grows toward the
  viewer and whose wavelength is perspective-spaced. A travelling gust is a
  phase that moves down the lines.
- Darken the water with fixed-colour subtract on water lines only; pulling
  one channel harder tints the reflection (green → gold reflects rose).
- Per-line palette effects (a sun's stripes, haze bands) must be looked up
  by *source* row so the reflection inherits them.
- Sprites are not mirrored by scroll: place an explicit `flip_y` twin at the
  mirrored height with the same ripple offset.

## Light as colour math: shafts, caustics, glow

Put the light on its own layer on the **sub** screen only
(`screen.sub.bgN = true`, not main), set `color.addend = "sub"`,
`color.op = "add"`, and enable `color.on` for every layer the light should
brighten. The light then adds onto whatever it crosses instead of painting
over it.

- Shafts from one row of tiles: vertical ray tiles, slanted by a per-line
  `scroll.x` skew that drifts over time so they sway. Pick dimmer copies per
  line to fade with depth.
- Where the light layer is empty, the addend is COLDATA (an empty sub-screen
  pixel selects the fixed colour, not CGRAM 0). Drive `color.fixed` per line
  as a ramp and you get depth haze for free.
- `color.half = true` on a band of lines makes a translucent surface.
- Sprites take colour math only on **OBJ palettes 4–7**. Place glowing
  sprites there (`dma(name, { pal = 4, ... })`).
- Additive sprites over a gradient: put the background on the sub screen
  too and set `color.fixed` per line to the backdrop colour, so a spark adds
  onto exactly what is behind it everywhere.

## Windows as geometry: beams, shadows, irises

A window is one horizontal span per line, and the span can change on every
line. Compute `win.w1.lo/hi` (and `w2`) inside `hdma()` and the mask becomes
any shape that is convex per row: a cone pivoting on a lamp, a circle, a
craft's silhouette.

- Things that exist only in the light: window a layer to the *outside* of
  the inverted beams (both windows inverted, combined with `"AND"`), on main
  and sub as needed. Fog, rain, even a sprite (`win.obj`) appear only where
  the beam falls.
- Light that adds: restrict colour math with the colour window
  (`win.color.w1/w2`, `color.region = "inside"`).
- A cast shadow: reshape a window per line to the caster's outline (precompute
  spans per pose in the art script) and colour-math *subtract* inside it.
- Window logic is per layer, and each layer has only two windows: plan which
  effect gets which window before writing the scene.
- Brighter overlap by flicker: two windows can't add twice, but time can.
  Alternate the colour window's combine between `"OR"` and `"AND"` on even
  and odd frames (key it on the frame number so seeks stay exact). A single
  beam is lit every other frame and the crossing every frame, so at 60 Hz the
  eye sees the crossing twice as bright. It only survives 60 fps capture:
  a 30 fps encode keeps one phase and the effect vanishes or strobes.

## Sprite formations within the line budget

128 OAM entries, but only 32 sprites / 34 slivers per scanline. A swarm that
forms a picture will drop sprites where many targets share rows.

- Choose target points in the art script with farthest-point sampling over
  the picture's opaque pixels, rejecting candidates that would put more than
  ~22 sprites on any row (leave headroom for sprites in flight).
- Order targets by angle around the centre so neighbours in the swarm land
  as neighbours, and stagger departures so the formation sweeps in.
- A 3D swarm is just Lua: rotate points on a ring or cloud, project with
  `s = f / (f + z)`, and pick a brighter/larger tile for nearer sprites.
- Hand off to a background copy of the picture on the landing frame (BG
  with `mosaic` resolving 15 → 0) and turn the sprites into an outward burst.
- `ppu check` fails on overflow; fix the cause rather than adding
  `--allow-overflow`.

## Music-synced finale

Generate `song{}` pattern strings in the art script (one track per voice:
pad voicing, arpeggio, bass, drums) for a through-composed piece, for
example 8 bars × 16 steps at 96 BPM = 20 s with the landing on step 96
(15.0 s). In `frame()`, derive the landing time from the step, spawn or
brighten sprites per note, flash on the downbeat with a short fixed-colour
add (≈0.3 s, not full white), and `stop()` the song after the last bar so it
does not loop.

## Gradients without banding

5-bit colour bands badly in dark gradients. Interpolate channels in floats
and dither between neighbouring 5-bit levels by line (an ordered pattern
such as `{0, 0.5, 0.25, 0.75}[y % 4 + 1]` added before `floor`), then pack
`r + g * 32 + b * 1024`.

## Lua and hardware gotchas

- Toy Lua's `string` has only `len`, `sub`, `lower`, `upper` and `reverse`;
  there is no `table.sort` and no `exp` (use a rational falloff such as
  `1 - 1 / (1 + k * x)`). `math.random` exists but breaks seeking and loops;
  for performances hash instead: `sin(n * 12.9898) * 43758.5453`.
- `cgram[i]` reads return only what Lua wrote, never colours placed by
  `dma()`. Importers store each palette in ascending BGR555 order, so a
  generator can draw palette *indices* (index k as a known colour) and
  `main.lua` can set every real colour itself.
- `hdma()` hooks live one frame: register them in `frame()` every frame.
- OAM persists: turn off every sprite slot you stop using.
- A seamless loop needs time-derived state only; `ppu check --loop` proves it.
- Perf: keep per-line hooks small and precompute tables at load. A frame
  with three layers, a sub screen and 224-line hooks is typical; don't
  rewrite thousands of VRAM words per frame.
