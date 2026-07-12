# ppu.toys

[ppu.toys](https://ppu.toys) is a ShaderToy-style playground for an emulated SNES Picture Processing Unit. Write Lua, see the frame update live, and explore how tile backgrounds, sprites, palettes, scanline state, and compositing fit together.

## What it is

The project pairs a headless Rust PPU engine with a browser-based studio. The engine currently supports tile modes 0–4 and affine Mode 7, including the familiar Mode 1 and Mode 7 workflows, per-scanline effects, sprites, priority and color compositing, and PNG import for backgrounds and sprite sheets.

The studio provides a multi-file Lua editor, live output, register and memory inspectors, compositing controls, local sketches, bundled demos, and publishing. It is useful both as a creative toy and as an approachable way to learn the machinery behind SNES graphics.

## Quick start

You need Rust, Node.js/npm, `wasm-pack`, and [Cook](https://github.com/alexandru/cook) available on your path.

```sh
npm --prefix web install
cook wasm
cook dev-wasm
```

The final command starts Vite with the real WASM core. For a production build, run `cook build`.

Without Cook, the equivalent web workflow is:

```sh
npm --prefix web install
npm --prefix web run build:wasm
npm --prefix web run dev
```

The Rust crates can be built and tested directly with `cargo build --workspace` and `cargo test --workspace`.

## Lua authoring

A sketch is a small set of Lua files built around a `frame()` function. Lua writes PPU memory and register state; `scanline`/`hdma` hooks can vary that state across the frame for raster effects. The editor keeps the last valid program running while you type, and edits to bundled demos automatically become local sketches.

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
npm --prefix web run typecheck
npm --prefix web test
npm --prefix web run build
```

`cook build` produces the WASM package, `web/dist`, and the server binary. `cook dev-wasm` is the normal live-development loop.

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

## Deployment

Production-related files live in `deploy/`. The deployment pipeline is defined in `.github/workflows/deploy.yml`; review both before adapting the service to your own host.

## Contributing

Issues and small pull requests are welcome. A focused change with a clear explanation and relevant tests is the easiest kind to review. Please run `cook check` and keep public documentation free of local credentials or private operational details.

## License

ppu.toys is available under the [MIT License](LICENSE).
