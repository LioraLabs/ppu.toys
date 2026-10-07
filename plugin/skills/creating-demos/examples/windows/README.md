# Searchlights (showcase: windows)

A city at night in the rain. Two searchlights sweep the sky, find an airship,
hold it for a few seconds, and let it go. 12-second seamless loop, silent.

**The light is a mask.** Nothing about the beams is drawn:

- Each beam is a window. `hdma(0, 145)` recomputes window 1 (lamp A) and
  window 2 (lamp B) on every scanline (WH0–WH3), so the cone is real geometry
  that pivots on its lamp lens.
- BG2 (stratus fog, on the **sub** screen), BG3 (rain, main) and the airship
  (OBJ, main) are all windowed to the union of the beams: both windows
  inverted and ANDed (W12SEL/W34SEL/WOBJSEL, WBGLOG/WOBJLOG, TMW/TSW). Outside
  the light they don't exist.
- Colour math adds the sub screen to the main screen only inside the colour
  window (w1 OR w2, CGWSEL region = inside). Where the fog has holes the sub
  pixel is the backdrop, i.e. COLDATA, so the beam still glows.
- Light falls off with height: COLDATA and the fog palette are rewritten per
  line from tables built at load.
- Supporting: per-line backdrop gradient (night sky), palette-animated
  windows going dark and beacons blinking, BG2/BG3 scroll for drifting
  stratus and falling rain (both loop exactly in 12 s).

Art is procedural (`python3 build_art.py`). main.lua finds palette slots by
rank, since the importer stores each palette in ascending BGR555 order.

**Best 6-second window:** 3.5 s → 9.5 s (the beams converge on the airship,
cross on it, ride it across the sky, then let it go).
