# ppu

`ppu` is the Rust engine behind [ppu.toys](https://ppu.toys): an emulated SNES
Picture Processing Unit and S-DSP sound chip driven from Lua. This repository
holds the engine crate, the `ppu` command-line tool for authoring toys offline,
and the authoring guides.

Write Lua, render frames to PNG, check a toy for errors, pack it, then open the
packed file in the ppu.toys Studio to play it with sound, edit it live, and
publish it.

## Install

One line installs the latest release (macOS and Linux, Intel and ARM, and Windows), puts `ppu`
on your PATH and runs `ppu setup`, so the Studio's Connect button can start `ppu connect`. Run it
again to upgrade.

```sh
curl -fsSL https://ppu.toys/install.sh | sh   # macOS and Linux: into ~/.local/bin
```

```powershell
irm https://ppu.toys/install.ps1 | iex       # Windows, in PowerShell: into %LOCALAPPDATA%\Programs\ppu
```

Or with a package manager, then run `ppu setup` once yourself:

```sh
brew install lioralabs/tap/ppu   # macOS and Linux
cargo install ppu-cli            # from crates.io
```

Or build from this repository, or grab a tarball from the
[releases page](https://github.com/LioraLabs/ppu.toys/releases):

```sh
cargo install --git https://github.com/LioraLabs/ppu.toys.git --locked ppu-cli
```

## Quick start

```sh
ppu new my-demo
ppu render my-demo --at 0,2,4,6 -o previews
ppu check my-demo --duration 8 --loop 8
ppu pack my-demo -o my-demo.ppu.json
```

Then open `my-demo.ppu.json` at [ppu.toys](https://ppu.toys). `ppu docs <query>` prints
one of the notes in [docs/notes/](docs/notes/) for a diagnostic code, an API name, an error or
words; `ppu docs` lists the maps in [docs/maps/](docs/maps/). Start with `ppu docs start`.

To run a toy on a real SNES, open it in the [ppu.toys](https://ppu.toys) Studio:
its SNES panel checks it against the console's limits as you type, and
**Export to ROM** builds a cartridge image for an emulator or flash cart.

Every toy folder `ppu new` or `ppu connect` makes carries `AGENTS.md` and the
`making-toys` skill for coding agents (`.agents/skills/`, `.claude/skills/`),
written by `ppu` from [docs/](docs/) and refreshed when it is upgraded
(`ppu docs cli`, Agent skills). For Claude Code outside a toy folder, the
plugin's one skill installs `ppu` and starts a folder:

```sh
claude plugin marketplace add LioraLabs/claude-plugins
claude plugin install ppu-toys@lioralabs
```

## Layout

- `crates/ppu-core` is the emulator: tile modes 0 to 4, Mode 7, sprites,
  windows, color math, HDMA, the S-DSP, PNG import, and the Lua bindings. It
  also builds to WebAssembly for the site.
- `crates/ppu-cli` is the `ppu` binary: new, pack, unpack, check, render, docs, skills.
- `vendor/piccolo` is the Lua VM, crates.io 0.3.3 plus one backported fix and a few local ones. Its
  README says when to drop it.
- `docs/notes/` holds the docs as one note per idea and `docs/maps/` their reading orders, embedded
  into both the CLI and the site (`crates/ppu-docs`).
- `docs/AGENTS.md` and `docs/skills/` are the agent guidance `ppu` embeds and
  writes into toy folders; `docs/agent-instructions.md` is what the ppu.toys
  MCP server tells a connecting agent.
- `plugin/` is the Claude Code plugin: one bootstrap skill that installs `ppu`
  and starts a folder. The LioraLabs marketplace installs it from here.

## Develop

```sh
cargo test --workspace
```

The notes' snippets are run by `crates/ppu-docs`' notes test, so doc edits are covered by
`cargo test`.

## Releasing

Maintainers cut releases with cook: `cook bump`, `cook release`, then `cook tap`,
`cook aur`, and `cook publish` once the Release workflow is green. The Cookfile
has the details.

## License

MIT. See [LICENSE](LICENSE).
