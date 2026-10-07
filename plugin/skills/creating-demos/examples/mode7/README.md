# Skyward (showcase: Mode 7)

A low, fast run over an archipelago at golden hour, 12 s seamless loop, silent.
All art comes from `build_art.py` (seeded numpy/PIL), which also writes `art.lua`
(the world palette's CGRAM ranks and the craft-shadow spans).

## What's on screen

- **Sea and islands:** one 1024×1024 Mode 7 plane (73 unique tiles, 31 colours).
  It's a tiled terrain: corner-height tiles over a toroidal noise field, so the plane
  wraps seamlessly.
- **Floor:** per-scanline `hdma`. Each line below the horizon (89–223) gets its own
  M7A/M7C (texel step) and a centre point. `cx = cy = 0` with vertical scroll `Q − y`
  turns M7B/M7D into a lever arm, so the centre lands with 1/16-texel precision instead
  of M7X/M7Y's whole texels. That keeps the motion smooth.
- **Sky:** a per-line `mode = 1` switch for lines 0–88.
  - BG1 is a cirrus ceiling with its own per-line perspective (`scroll.y` per line),
    streaming overhead.
  - BG2 is the far cumulus bank with the sun.
  - Both turn with the heading.
  - The backdrop gradient is `cgram[0]` per line.
- **Haze:** colour math per line. The 12 far rows use half-add toward the horizon glow (a
  true 50/50 blend). Nearer rows get a fading additive warm wash.
- **Craft:** four 32×32 OBJ form one 64×64 frame. It has 7 bank poses from a tiny
  software rasteriser, and banks with the turn rate.
- **Shadow:** colour window 1 is reshaped per line to the craft's silhouette, and colour
  math *subtracts* inside it.
- **Water:** crest colours rotate through four CGRAM entries (palette cycling). Sun
  glints are re-coloured per line, hot near the horizon and cool up close.

Course: two S-turns per loop on a diagonal mean heading. The run covers (1024, 2048)
texels per loop, so it closes exactly, and the two halves overfly different islands
(flyovers at ~3 s and ~9 s).

## Best window

**1.0–7.0 s:** banking left, island flyover at ~3 s, level-out and right bank. Any
6 s window works; flyover moments are at 2.5–4 s and 8.5–10 s.
