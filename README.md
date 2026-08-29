# ppu.toys

[ppu.toys](https://ppu.toys) is a ShaderToy-style playground for an emulated SNES Picture Processing Unit. Write Lua, see the frame update live, and explore how tile backgrounds, sprites, palettes, scanline state, and compositing fit together.

## What it is

The project pairs a headless Rust PPU engine with a browser-based studio. The engine currently supports tile modes 0–4 and affine Mode 7, including the familiar Mode 1 and Mode 7 workflows, per-scanline effects, sprites, priority and color compositing, and PNG import for backgrounds and sprite sheets.

See [Source placement and `dma()`](docs/dma.md) for the Lua source-loading contract.

The studio provides a multi-file Lua editor, live output, register and memory inspectors, compositing controls, local sketches, and publishing. It is useful both as a creative toy and as an approachable way to learn the machinery behind SNES graphics.

## Quick start

You need Rust, Node.js/pnpm, `wasm-pack`, and [Cook](https://github.com/alexandru/cook) available on your path. The web app is a one-package pnpm workspace rooted at the repo top level; Cook drives it through the `cook_pnpm` module (`cook modules install` realises the pin in `cook.toml`).

```sh
pnpm install
cook wasm
cook dev
```

The final command starts Vite with the real WASM core. For a production build, run `cook build`.

Without Cook, the equivalent web workflow is:

```sh
pnpm install
pnpm --filter web run build:wasm
pnpm --filter web run dev
```

The Rust crates can be built and tested directly with `cargo build --workspace` and `cargo test --workspace`.

## Lua authoring

A toy is a small set of Lua files built around a `frame()` function. Lua writes PPU memory and register state; `scanline`/`hdma` hooks can vary that state across the frame for raster effects. The editor keeps the last valid program running while you type.

Drop a PNG onto the output to quantize and import it into authentic VRAM/CGRAM data. The inspector can then trace the rendered layers, sprites, palettes, and per-pixel compositing decisions.

## Architecture

- `crates/ppu-core/` is the pure Rust emulation, Lua, import, tracing, and rendering core. It also builds to WebAssembly.
- `web/` is the React and Vite studio consuming that WASM module.
- `crates/ppu-server/` is the Axum service for the built web app, sketches, publishing, authentication, and storage.
- `deploy/` contains production operations and infrastructure support.

The browser talks to the core through a small TypeScript seam, while the server remains separate from rendering. See `web/README.md` for a detailed contributor tour of the studio.

## Development

Run the repository-wide checks before sending a change:

```sh
cook check
```

This runs TypeScript typechecking, Vitest, and both Rust test suites. Useful direct fallbacks are:

```sh
cargo test --workspace
pnpm --filter web run typecheck
pnpm --filter web test
pnpm --filter web run build
```

`cook build` produces the WASM package, `web/dist`, and the server binary. `cook dev` is the normal live-development loop, and `cook dev-offline` boots the same Vite server with MSW answering every API call so the app runs with no backend at all.

## Configuration

Copy the safe template for local server configuration:

```sh
cp .env.example .env
```

The server reads environment variables rather than parsing the file itself. Load them into your shell before starting it:

```sh
set -a; source .env; set +a
cook server.run
```

The defaults use SQLite with database-backed blobs. Discord credentials are optional; fill them in when testing authentication or publishing flows that require them.

## Running the site locally

```sh
cook stack
```

This runs the backend against the disposable, gitignored `build/dev.db` and the
Studio at `http://localhost:5173/`. Ctrl-C stops both. Use `cook stack-fresh` to
delete the database first, or `cook db-nuke` to delete it without starting the
site. SQLite creates and migrates the next database automatically.

No `.env` or Discord account is needed: **Sign in locally** uses the disposable
`ppu` development account. A production build still uses Discord.

## Edit a toy on your machine

Install the small sync client:

```sh
cargo install --git https://github.com/LioraLabs/ppu.toys.git --bin ppu
```

When working from this checkout, `cook cli-install` installs the same binary.

On your profile, create a CLI token under **Local editing**, then run the command
shown there once.

Start a new toy entirely locally:

```sh
ppu new my-tutorial
cd my-tutorial
$EDITOR main.lua
ppu status
ppu push
```

The first push creates a private draft and prints its URL. Open that URL in the
Studio when you are ready to render its preview and publish it.

To work on an existing toy:

```sh
ppu pull https://ppu.toys/t/abc123
cd abc123
$EDITOR main.lua
ppu push
```

`ppu sync` handles the normal two-way loop: it pushes when only local files
changed, pulls when only the server changed, and stops when both changed. In a
conflict it leaves local files untouched and writes the remote versions under
`.ppu/remote/`. `ppu pull --force` discards local edits; `ppu push --force`
explicitly overwrites the latest remote code.

`ppu.json` keeps the toy id, server revision, title, description, file order,
and last-synced hashes. Pushes create normal server revisions and preserve the
toy's existing image sources. Studio uses the same revision check, so a browser
save and CLI push cannot silently overwrite one another.

The intended toy development cycle is: `ppu new`, edit and run `ppu sync`
as needed while the remote copy remains a private draft, open it in Studio to
check the real renderer, then publish. Keep official tutorials/examples in a
separate private repository as ordinary `ppu` toy directories; ppu.toys—not
this application repository—is their public distribution. Update them with the
same pull/edit/sync loop.
Set `PPU_CONFIG` to override the default token file at
`~/.config/ppu/config.json`.

## Deployment

Production-related files live in `deploy/`. The deployment pipeline is defined in `.github/workflows/deploy.yml`; review both before adapting the service to your own host.

## Contributing

Issues and small pull requests are welcome. A focused change with a clear explanation and relevant tests is the easiest kind to review. Please run `cook check` and keep public documentation free of local credentials or private operational details.

## License

ppu.toys is available under the [MIT License](LICENSE).
