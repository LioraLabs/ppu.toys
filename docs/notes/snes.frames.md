---
id: snes.frames
kind: diagnostic
title: A toy's update rate is the targetHz build setting, not a `---@snes frames` comment
codes: [snes.frames]
---
`error[snes.frames]` means a `---@snes frames` comment does not give a whole number from 1 to 4. The comment is an
old way to slow a cartridge's updates; the rate is now a build setting, `targetHz` in ppu.json (60 or 30 on NTSC) or the
Studio's Cartridge settings. Delete the comment and set the rate there, if you want one other than 60.

Write motion in steps per update, not per second: at 30 Hz `f` still counts video frames, advancing by 2 each update,
so motion keyed to `f` or `t` keeps its speed. See [[snes.frames_setting]] when both are set.

```luau
-- ppu.json: "targetHz": 30
local x = makeBounded(0, 0, 255)

function frame(t, f)
  x = math.floor(t * 60) % 256
  bg[1].scroll.x = x
end
```
