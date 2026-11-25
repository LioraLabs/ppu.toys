---
name: start-a-toy
description: Start making a ppu.toys toy or game (an emulated SNES PPU and S-DSP programmed in Luau) with the ppu CLI. Use when someone wants to make or open a ppu.toys toy and the folder has no AGENTS.md from ppu yet.
---

# Start a ppu.toys toy

The guidance for making toys ships inside the `ppu` command, matched to its
engine version: `ppu` writes it into every toy folder as `AGENTS.md` and the
`making-toys` skill (`.agents/skills/` and `.claude/skills/`). This skill only
gets you there.

1. Check for the CLI: `ppu --version`. Without it, offer to install it (ask
   first, never install silently):

   ```sh
   brew install lioralabs/tap/ppu   # macOS and Linux
   cargo install ppu-cli            # anywhere with Rust
   ```

   Windows and other routes: https://github.com/LioraLabs/ppu.toys#install.
   After a Windows install the running shell does not see the new PATH: call
   `ppu.exe` by its full path or open a new shell.
2. Make or open the folder:
   - a new toy: `ppu new my-toy` (`--world` for a level painted in Tiled);
   - a project on ppu.toys: `ppu login`, then
     `ppu connect <project link> my-toy`, which keeps the folder and the
     open Studio tab in step;
   - an existing toy folder from an older `ppu`: `ppu types my-toy`.
3. Read the folder's `AGENTS.md` and its `making-toys` skill, and follow
   them. They supersede this one.
