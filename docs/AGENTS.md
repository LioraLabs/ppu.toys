# A ppu.toys toy

This folder is a ppu.toys project: Luau code (`main.luau` and the modules it
requires) programming an emulated SNES PPU (graphics) and S-DSP (audio), with
its art, palettes and sounds listed in `ppu.json`. The `ppu` command runs,
checks and renders it offline, with the same engine as the ppu.toys Studio.

Before changing anything, read the `making-toys` skill: `.agents/skills/` and
`.claude/skills/` hold the same copy. Then:

- Code first: edit, then `ppu check .`, then read only the note a finding
  points at (`ppu docs <code>`), one finding at a time, and check again.
  For an API, read its declaration in `ppu.d.luau` and write the call: if
  it is wrong, the check names the note. Never two reads in a row: write
  code or check between them. When the check passes, stop reading. `ppu.d.luau`
  declares every global: look names up there, never guess them.
- Iterate in small steps: `ppu check .` for errors, `ppu render . --at 0,2,4
  -o previews` and look at the PNGs. Running without errors does not mean it
  looks right.
- Use the blessed APIs (`sprites.group`, `scenes`, `@ppu/hud`, `makeFixed`
  and `makeBounded` for lasting state): the same code then builds into a
  cartridge.
- In a folder `ppu connect` linked, every save reaches the project and the
  open Studio tab. Publishing is the person's decision.

`ppu` writes this file and the skills, and refreshes them when it is
upgraded. A copy you edit is kept as it is; `ppu skills --reset` puts back
the versions this `ppu` ships.
