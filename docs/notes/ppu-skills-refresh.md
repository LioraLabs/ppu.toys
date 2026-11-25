---
id: ppu-skills-refresh
kind: concept
title: ppu keeps AGENTS.md and the making-toys skill in the folder, refreshing only the copies you did not edit
codes: []
---
`ppu new`, `ppu connect`, `ppu types` and `ppu skills` write the guidance for coding agents into the toy
folder: `AGENTS.md`, and the `making-toys` skill as two plain copies, `.agents/skills/` (Codex, OpenCode
and others) and `.claude/skills/` (Claude Code). They match the engine version. `.ppu/skills.json` records
the version that wrote them and a hash per file. After an upgrade the next of those commands rewrites each
file that is missing or still as `ppu` wrote it, keeps one you edited and prints a line naming it; a run
by the same version writes nothing. `ppu skills <dir> --reset` overwrites every file, edited or not.

```sh
ppu skills .
ppu skills . --reset
```
