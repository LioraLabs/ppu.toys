---
id: ppu-docs-offline
kind: recipe
title: ppu docs answers from the binary, offline, by note id, diagnostic code, error text or words
codes: []
---
`ppu docs <query>` prints the one best note for a note id (`one-way-ledges`), a diagnostic code
(`snes.range`), the text of an error a build printed, or a few plain words; `ppu docs` alone lists the
maps, and `ppu docs <map>` lists a map's notes. `--links` adds the notes a
note links to. The notes are bundled in the binary of this version, so they describe exactly the engine that
runs the toy, with no network. Look an API name up in `ppu.d.luau` or `ppu docs <name>` rather than
guessing it; paste an error's text into `ppu docs` for its fix.

```sh
ppu docs snes.range
ppu docs one-way ledges --links
```
