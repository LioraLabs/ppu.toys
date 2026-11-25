---
id: fade-a-scene-switch
kind: recipe
title: A scene switch fades with scenes.go(scene, {fade = n}); a cut inside one scene fades by hand with brightness
codes: []
---
[[scenes.go|scenes.go(CAVE, {fade = 15})]] darkens the screen over 15 video frames while the old scene
keeps running, cold-boots, and brightens the new scene over 15 more. The engine owns [[brightness]]
meanwhile, and a second `scenes.go` during the fade changes nothing, so the scene does not have to guard
against a double press.

A cut that stays in one scene (a house door that moves the hero across the same map) fades by hand: a
bounded level and direction, stepped once a frame. Fade out, and once the screen is black make the cut
and fade back in. Hold the controls while it runs.

```luau
local fade = {level = makeBounded(15, 0, 15), dir = makeBounded(1, -1, 1)}
local hero = {x = makeBounded(40, 0, 255)}

local function step_fade(): boolean
  fade.level = math.clamp(fade.level + fade.dir, 0, 15)
  brightness = fade.level
  return fade.dir < 0 and fade.level == 0
end

function frame(t, f)
  if step_fade() then
    hero.x = 200 -- the cut, while the screen is black
    fade.dir = 1
  end
  if pad.pressed.a and fade.level == 15 then
    fade.dir = -1
  end
end
```
