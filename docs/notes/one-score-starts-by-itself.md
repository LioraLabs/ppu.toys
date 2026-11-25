---
id: one-score-starts-by-itself
kind: rule
title: Only one score may start by itself; the others are set up with at = false and played later
codes: []
fixture: twinstar
---
A [[score]] with no `at = false` starts playing at power-on, and a cartridge allows only one of those.
Every other score is made with `at = false`, silent until its handle's `play()`. One score plays at a
time: `play()` stops whichever score is playing, whether a bare handle, a scene's `sound.songs` or the
program's started it, in the preview and on a cartridge alike, so no `stop()` comes first. Make every
handle at file level (a score is setup work) and keep it. A cartridge holds at most 256 scores.

```luau
local title = score{song = "twinstar-title", loop = true}
local battle = score{song = "twinstar-battle", at = false, loop = true}

function frame(t, f)
  if pad.pressed.start then battle.play() end
end
```
