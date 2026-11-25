---
id: snes.precision_cost
kind: diagnostic
title: A finer makeFixed step makes every product that reads the value wider and slower
codes: [snes.precision_cost]
---
`x is stored at 1/256: its products are 32-bit (about N ticks each ...); at 1/16 the operand is 1 byte
narrower` charges the multiplications in an update to the value whose step makes them wide. The
65816 multiplies 8 bits at a time, so each byte a value needs makes every product that reads it
costlier; the step in [[makeFixed]] (or the precision a [[lookup]] returns) decides how many bytes
that is. Over [[snes.budget|budget]] each costly value gets its own warning, costliest first;
under budget one note names the costliest.

Pick the coarsest step the game still looks right with: positions rarely need more than 1/16 of a
pixel, speeds 1/16 or 1/8. Round a lookup's results to whole numbers or a coarse step when that is all
the screen shows. Compare a screenshot of the same input before and after the change. A product is cheapest when both sides are whole numbers proven 0..255; a signed side or a wider one makes it costlier, so bound a weight 0..255 rather than -1..1 when the look allows.

```luau
-- Was: makeFixed(0, -4, 4, 256); 1/16 of a pixel per frame is as smooth on screen.
local vx = makeFixed(0, -4, 4, 16)
local x = makeFixed(128, 0, 255, 16)

function frame(t, f)
  vx = math.clamp(vx + (pad.right and 0.25 or 0) - (pad.left and 0.25 or 0), -4, 4)
  x = math.clamp(x + vx * 0.5, 0, 255)
  bg[1].scroll.x = x // 1
end
```
