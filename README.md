# ppu

`ppu` is the Rust engine behind [ppu.toys](https://ppu.toys): an emulated SNES
Picture Processing Unit and S-DSP sound chip driven from Lua. This repository
holds the engine crate, the `ppu` command-line tool for authoring toys offline,
and the authoring guides.

Write Lua, render frames to PNG, check a toy for errors, pack it, then open the
packed file in the ppu.toys Studio to play it with sound, edit it live, and
publish it.

## Install

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

Then open `my-demo.ppu.json` at [ppu.toys](https://ppu.toys). `ppu docs` prints
the same guides that live in [docs/](docs/). Start with
[the authoring loop](docs/cli.md) and [the PPU pipeline](docs/registers.md).

## Layout

- `crates/ppu-core` is the emulator: tile modes 0 to 4, Mode 7, sprites,
  windows, color math, HDMA, the S-DSP, PNG import, and the Lua bindings. It
  also builds to WebAssembly for the site.
- `crates/ppu-cli` is the `ppu` binary: new, pack, unpack, check, render, docs.
- `vendor/piccolo` is the Lua VM, crates.io 0.3.3 plus one backported fix. Its
  README says when to drop it.
- `docs/` holds the authoring guides, embedded into both the CLI and the site.

## Develop

```sh
cargo test --workspace
```

The Lua fences in `docs/audio.md` are executed by the test suite, so doc edits
are covered by `cargo test`.

## License

MIT. See [LICENSE](LICENSE).
