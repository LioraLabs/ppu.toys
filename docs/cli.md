# Standalone demo authoring

The `ppu` executable includes the Lua engine, PPU renderer, PNG importer,
PNG output, and these API docs. No application checkout, Node, Python, WASM
build, or running server is required. Audio and video encoding are not included.

Install with Cargo, then verify with `ppu --version`:

```sh
cargo install --git https://github.com/LioraLabs/ppu.toys.git --locked ppu-cli
```

For Claude Code, install the authoring skill:

```sh
claude plugin marketplace add LioraLabs/claude-plugins
claude plugin install ppu-toys@lioralabs
```

## Create, inspect, check, pack

```sh
ppu new my-demo
ppu render my-demo --at 0,2,4,6 -o previews
ppu check my-demo --duration 8 --loop 8
ppu pack my-demo -o my-demo.ppu.json
ppu check my-demo.ppu.json --duration 8 --loop 8
```

`new` creates an animated Mode 7 floor with HDMA perspective and an editable
`assets/floor.png`. It refuses existing directories. Edit the Lua and PNG;
each subsequent load reconverts the image through Studio's importer.

Open `my-demo.ppu.json` in ppu.toys Studio. The packed file contains all Lua
and imported graphics; it needs no external asset files. Publishing remains
a Studio action. The directory's `ppu.json` is the editable manifest, not the
upload file.

`render` accepts a project directory or a packed file. It runs from time zero
through the last sample, preserving stateful animation, and writes native
256×224 RGBA PNGs named `frame-00000120.png` (frame 120 = 2 seconds). Existing
matching output PNGs are replaced. Inspect the images; successful execution
alone cannot tell whether a composition looks right.

`check` compiles and renders at 60 fps, including the endpoint. The default
duration is 30 seconds; set it to cover the entire performance. Lua errors
include file/line and runtime frame/time. Source metadata reports palette,
tile, and VRAM budgets. Quantization/cropping reports are informational and
must be reviewed; sprite range or sliver overflow fails the check unless
`--allow-overflow` is given for a deliberate effect.

For seekable performances:

```sh
ppu check my-demo --duration 96 --seek 0,15.9833,16,16.0167,48,80 --loop 96
```

`--seek` compares sample pixels from continuous playback with fresh direct
seeks and backward seeks. `--loop` adds samples at zero, quarter intervals,
and the last frame before the loop, then compares samples one period later.
It does not infer a period from Lua. Keep the period and duration consistent
with the Studio timeline. Times round to the nearest 60 Hz frame; a seek
sample must fall within the checked duration. These comparisons are optional
for stateful games. Sprite overflow is checked on every continuous frame.

`pack` preserves file order and publication origin metadata. It validates
the manifest and converts PNGs, but does not execute Lua. Check the final
packed artifact as well as the source directory.

## Project manifest

```json
{
  "title": "My demo",
  "description": "A PPU graphics performance.",
  "files": ["ppuglobals.lua", "main.lua"],
  "sources": [
    {
      "name": "floor",
      "kind": "m7",
      "file": "assets/floor.png",
      "options": {}
    }
  ]
}
```

Lua file names are flat and execute in the listed order in a shared global
scope. Include `main.lua`. Use an empty `sources` array for Lua-generated
VRAM/CGRAM artwork. PNG paths are relative to the project directory and may
not contain `..` or be absolute.

Source kinds: `bg` (tiled background), `sheet` (ordered tiles), `obj` (sprite
cells), and `m7` (Mode 7 plane). Options use Studio's names: `bit_depth`,
`tile_size`, `cell_size`, `extbg`, `dither`, `dither_strength`, and
`alpha_threshold`. Defaults come from the shared importer. For Mode 7 EXTBG,
add `priority_file` pointing to a same-sized black-and-white PNG and set
`options.extbg` to true. Read `ppu docs sources` and `ppu docs dma` before
choosing format and memory placement.

Sources exported from Studio can instead have `payload` (a relative binary
file path), `meta`, and `options`. Specify exactly one of `file` or `payload`.
PNG bytes are never a binary payload. Unpacking a toy writes converted
payloads under `.ppu/sources/`; the original PNG cannot be recovered from
the lossy converted data, so retain the editable source project.

```sh
ppu unpack downloaded.ppu.json editable-copy
ppu pack editable-copy -o revised.ppu.json
```

Unpack overwrites matching files: choose a fresh destination.

## Lua surface and effects

`frame(t, f)` writes registers and memory; `init()` and top-level statements
perform setup. `hdma(first, last, callback)` changes supported state per
scanline. Use `ppu docs` for offline topics. Modes 0–4 and 7, tile layers,
sprites, windows, mosaic, palette animation, color math, and main/sub screens
can be combined. Register controls use friendly Lua fields, with raw aliases
for screen/window/color-math registers. Direct `vram[]` and `cgram[]` writes
allow generated graphics; `obj[]` controls OAM. `voice[]`, `dsp`, `timer()`,
and the music kit drive the S-DSP (see `ppu docs audio`). This is a PPU
playground, not a stock SNES CPU/DMA timing simulator. Modes 5/6, interlace,
overscan, and read-only counter registers are not currently modelled.
