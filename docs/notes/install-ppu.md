---
id: install-ppu
kind: recipe
title: One line installs ppu on macOS, Linux and Windows, and the same line upgrades it
codes: []
---
The `ppu` executable holds the engine, the PNG importer and the API docs, so checking and rendering a toy
needs no browser tab, Node or running server. One line installs the latest release, puts `ppu` on the PATH
and runs `ppu setup`; run it again to upgrade. The line is `curl -fsSL https://ppu.toys/install.sh | sh`. Or use a package manager, then run `ppu setup` yourself.
A shell started before the install may not see the new PATH (the Windows line
updates its own window): open a new shell or call `ppu` by its full path.

```sh
ppu --version
```
