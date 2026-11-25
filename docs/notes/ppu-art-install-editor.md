---
id: ppu-art-install-editor
kind: recipe
title: ppu art install puts the SNES-art plugin into Aseprite or Pixelorama from inside the ppu binary
codes: []
---
`ppu art install aseprite` writes the plugin (`package.json`, `snes-art.lua`) into
`extensions/ppu-art/` of Aseprite's user folder, over an older install: `--dir`, else
`ASEPRITE_USER_FOLDER`, else Aseprite's own (`~/.config/aseprite` on Linux, `~/Library/Application
Support/Aseprite` on macOS, `%APPDATA%\Aseprite` on Windows). Restart Aseprite; the first time it opens a
project PNG, tick "Give full trust to this script" and press Allow. `ppu art install pixelorama` writes
`SnesArt.zip` into `<user folder>/extensions/`; restart Pixelorama and enable SNES art once in Preferences >
Extensions. An unknown editor is refused, naming the known ones. The plugins call the `ppu art` commands,
so upgrade `ppu` and install again together.

```sh
ppu art install aseprite
```
