# Undersea · colour math showcase

A sunlit sunken temple, 12-second seamless loop, silent. Everything bright is
*added* by the PPU's colour math rather than painted into the art.

- **Sub-screen light layer.** BG3 is on the sub screen only (`light.lua`, a
  Lua-generated 2bpp layer). `color.addend = "sub"`, `op = "add"` on BG1, BG2,
  OBJ and backdrop, so shafts and caustics brighten whatever they cross:
  ruins, kelp, sand and fish.
- **Sun shafts.** One row of vertical ray tiles, slanted by a per-line BG3
  scroll skew (HDMA) that slowly changes, so the shafts sway. Six dimmer
  copies of the row are picked per line to fade them with depth, and a pulse
  travels down them. Four ray groups use four BG3 palettes that breathe
  independently.
- **Caustics.** A two-layer Voronoi net under the sand, scrolled with BG1,
  rippled per line, compressed toward the horizon.
- **Depth haze.** Where BG3 is empty the addend falls back to COLDATA, an HDMA
  ramp of blue-green that's strongest mid-water. That's why the ruins fade.
- **Translucent surface.** The top 22 lines switch to half-add (`color.half`),
  blending a rippling surface with the backdrop and kelp tips.
- **Sprites.** The fish and bubbles use OBJ palettes 4 and 5. Only palettes
  4–7 take colour math, so the light catches them too.
- Per-line backdrop gradient (`cgram[0]`) and kelp sway (BG1 scroll) ride the
  same HDMA hooks.

Best 6-s window: **2.5–8.5 s** (the school crosses the shafts in front of the
arch and temple).
