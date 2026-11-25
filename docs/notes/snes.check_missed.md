---
id: snes.check_missed
kind: diagnostic
title: A build refused after a passing check is a bug in the check, to report
codes: [snes.check_missed.export, snes.check_missed.build]
---
`error[snes.check_missed.build]: the check passed, then a later stage refused it: ...` means the
cartridge check (`ppu check`, the Studio's snes_check) accepted the toy, and a later stage of the
ROM build refused it anyway: `snes.check_missed.export` when making the cartridge's code refused it,
`snes.check_missed.build` when fitting it into the ROM did. The message carries what that stage
said. This should never happen: it is a bug in the check, not in your game.
Report it with the toy. Meanwhile, the original finding after the colon says what the build needs,
as it would for that code (for a ROM that does not fit, see [[snes.rom|the ROM size note]]): change
the toy as that finding asks, then build again.

```luau
-- A toy the check passes and the build takes: nothing here reaches a later refusal.
function frame(t, f)
  brightness = 15
end
```
