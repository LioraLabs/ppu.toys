---
id: ppu-art-json-protocol
kind: concept
title: Every ppu art --json answer is one JSON object with a protocol number, so an editor plugin reads reason codes, not messages
codes: []
---
An editor plugin calls `ppu art ... --json` and reads one object on stdout: `"ok"`, `"protocol": 1` and
the command's fields. A refusal is `{"ok":false,"protocol":1,"reason":"<code>","error":"<message>"}` with
`reason` one of `not_project`, `not_listed`, `listed_twice`, `unmatched`, `refused`, `usage` or `error`,
and the exit code the command has for it (1 an importer refusal or unmatched palette, 2 the rest). A
plugin tests `protocol` (absent or lower than it was written for: tell the author to update `ppu`) and
`reason`, never the message text, which may be reworded; new fields do not raise the protocol, a change an
older plugin would misread does. `art check --json` carries the findings (`mixed-rows`, `palette-budget`,
`tile-budget`, `map-budget`, `plte`, `not-snes-art`, `import`) with tile positions, the budgets, the palette
colours and which indices are transparent.

```sh
ppu art check assets/sprites.png --json
```
