---
id: ppu-render-with-a-save-file
kind: recipe
title: ppu render --srm plays a saveData record on a battery file, which also works in emulators and on flash carts
codes: []
fixture: twinstar
---
`ppu render <dir> --at ... -o build/shots --srm save.srm` loads the file as the battery RAM first, when it exists,
runs the toy on it, and writes the save back after the last frame in the cartridge's own layout. Render a
game once to earn a save, then again to see the title screen with that high score. The same `.srm` is what
the Studio's Cartridge panel exports and imports, so one file moves between the preview, emulators and flash
carts. A save the record cannot use (damaged, or from before a field was added or its bounds changed)
starts from the defaults ([[saveData]]).

```sh
ppu render . --at 5 --hold a@60-300 -o build/shots --srm toy.srm
ppu render . --at 0 -o build/shots --srm toy.srm
```
