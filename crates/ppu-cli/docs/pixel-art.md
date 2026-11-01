# Pixel art for the SNES

The PPU decides what art can look like: 8×8 tiles, palettes of 16 colours
(15 plus transparency) in 15-bit colour, a limited budget of unique tiles,
and sprites built from 8×8 pieces. Good 16-bit art comes from designing for
those limits, not from shrinking a painting into them. This chapter covers
the craft; [sources](sources.md) covers conversion and budgets.

## Start from the palette

Pick the palettes before drawing anything.

- **Ramps, not colours.** Each material gets a ramp of 3–5 shades, from dark
  to highlight: fur, wood, water, stone, skin. A 16-colour sprite palette is
  three or four ramps plus an outline colour.
- **Hue-shift the ramp.** Shadows move toward blue or purple and lose a
  little saturation; highlights move toward yellow. A ramp that only changes
  brightness looks flat and muddy.
- **Keep contrast between neighbours.** A sprite has to read against every
  background it crosses. Keep its darkest and lightest values apart from the
  backgrounds' middle values.
- **The hardware has 32 levels per channel.** Colours closer than one step
  (8 in 0–255) merge; spacing ramp steps by 3 or more levels keeps them
  distinct.

## Sprites

- **Silhouette first.** Fill the shape in one flat colour at its real size
  (16×16, 32×32) and check that it reads as the character. Exaggerate what
  identifies it: eyes, ears, hat, weapon. A big head and big eyes read best at
  small sizes.
- **Outlines.** A dark outline separates a sprite from busy backgrounds. Use
  the darkest shade of each part's own ramp rather than pure black ("selective
  outlining"), and drop the outline where light hits the edge.
- **Light from one side,** usually top-left, consistent across every sprite
  and tile in the game.
- **No orphan pixels or jaggies.** Lines step in regular runs (1-1, 2-2,
  1-2-1), not random ones. Single stray pixels look like noise at 1×.
- **Animate the key poses.** A walk cycle needs 2–4 frames with a visible
  squash (body dips on the contact frame). Idle animation (blinking,
  breathing, an ear twitch) brings a still character to life cheaply.
- **Cell sizes are what the hardware has:** a toy picks two sprite sizes with
  `obj.size_sel` (8 and 16, 8 and 32, 16 and 32, …). A character that fits
  one hardware cell costs one sprite; the sprites library draws a 24×24 or
  48×32 cell as several, so each one costs more of the 128 sprites and of a
  line's 34 slivers.

## Backgrounds and tiles

- **Design in tiles.** Plan a set of 8×8 or 16×16 tiles that repeat and
  connect: ground top, ground fill, corners, edges, and one or two variants so
  repetition doesn't show. Unique tiles are the real budget, not pixels.
- **Parallax in layers.** A far layer (sky, gradient, distant mountains) in a
  few dark, desaturated colours; a middle layer (hills, trees) with more
  contrast; the play layer sharpest and brightest. Scroll each at its own
  speed.
- **Sky gradients cost nothing.** A gradient set per scanline with `hdma`
  through `color.fixed` or a palette entry uses no tiles at all; see
  [scanlines](scanlines.md).
- **Dither sparingly.** A checkerboard between two ramp steps smooths a large
  gradient (sky, water). Inside small sprites it turns to noise.
- **Keep gameplay readable.** Things the player can stand on, collect or hit
  must stand out from decoration. Give them more contrast or an outline.

## Mode 7

- One 256-colour palette and at most 256 unique 8×8 tiles for the whole
  plane. A track or map is drawn from a tile set like any background: road,
  edges, grass, water and markings as repeating tiles.
- Detail disappears in the distance: favour big shapes and strong colour
  contrast between road and off-road over fine texture.
- Add depth with the scanlines above the horizon: a sky gradient, a
  background layer of mountains or clouds that scrolls as the camera turns.

## Making art as code

When an agent or a script draws the art, draw at the final pixel size and
place every pixel deliberately:

- generate PNGs at native resolution (a 16×16 sprite is a 16×16 image), from
  explicit palettes and pixel maps or shape code, never by downscaling a
  large drawing;
- keep each sprite on a strict palette, indexed if possible, so conversion
  doesn't invent colours;
- look at the result at 2× or 4× with nearest-neighbour scaling, in the game,
  against its real backgrounds, and redraw what doesn't read; plan on several
  passes;
- check the conversion report for palettes and tiles used, and keep within
  VRAM.
