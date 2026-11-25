---
id: graphics-come-from-sources
kind: concept
title: Every picture and sound a toy shows comes from a source ppu.json lists, and a source has a kind
codes: []
fixture: twinstar
---
A source is a file the project's `ppu.json` lists under `sources`, with a `name`, a `kind`, its
`file` and its `options`. Image sources are indexed PNGs in `assets/`: `bg` is a whole background
(tiles, a tilemap and palettes), `sheet` is 8x8 tiles in reading order with no tilemap (code or a
Tiled map decides where each goes), `obj` is a sprite sheet cut into equal cells, and `m7` is one
256-colour Mode 7 plane. A `palette` source is colours only, a `.gpl` in `palettes/`
([[palette-sources|palette sources]]). Samples and songs are sources too.

A source has no address until code places it: [[place-art-with-dma|dma() places it]] and returns
where it went. The kind decides what a placement takes and returns, and the
[[source-budgets|limits each kind keeps]]. Names share one namespace, so a palette is named after
its art (`near_pal` for `near`).

```luau
-- A bg source: its tiles, tilemap and colours in one call, shown on BG2.
local near = dma("near", {palette = "near_pal", char = 0x0000, map = 0x7400, pal = 32})

function init()
  mode = 1
  bg[2].char_base, bg[2].map_base = near.char, near.map
  screen.main.bg2 = true
end
```
