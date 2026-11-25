---
id: snes.frames_setting
kind: diagnostic
title: The project's update-rate setting wins over a `---@snes frames` comment, which is then ignored
codes: [snes.frames_setting]
---
The warning `---@snes frames N is ignored: the project's build setting is ... Hz` means the toy sets its pace twice and
they disagree. Region and update rate are build settings: `targetHz` in ppu.json, or the Studio's Cartridge settings.
The cartridge follows the setting. Delete the comment, and change the setting if the comment was the rate you meant
(see [[snes.frames]]).

```luau
-- Was: ---@snes frames 2 at the top of main.luau, with "targetHz": 60 in ppu.json.
-- Now: only ppu.json says the rate.
function frame(t, f)
  bg[1].scroll.x = f % 256
end
```
