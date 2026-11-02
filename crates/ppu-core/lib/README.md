# Engine libraries

Each `<name>.luau` here is a library a toy loads with `require("@ppu/<name>")`. It ships
built into the engine: one copy, the same for every toy, with no version pinned per toy.
The preview runs it, and the SNES compiler builds it into the cartridge, from this one source.

## How a library loads

- `build.rs` embeds every `lib/*.luau` file as `frontend::LIBRARIES`. There is no list to edit.
- `frontend::with_libraries` adds the libraries a toy requires, directly or through other
  modules, to its files as `@ppu/<name>.luau`. Both the engine (`LuaEngine::set_sources`) and
  the compiler call it before anything else reads the files. From then on, a library is an
  ordinary required module: it runs once, on its first `require`, and its returned table is
  the value of `require`. A library may require another library.
- A library that nothing requires does not run, and costs a cartridge nothing.
- `require("@ppu/<unknown>")` is refused, naming the libraries the engine ships.

## Writing one

- Write it in the Luau the cartridge accepts (the SNES guide), and test it in the preview and
  on the cartridge.
- Return one table of functions and values, like any module.
- Text drawing reads a constant glyph table inline, `GLYPH[string.sub(s, i, i)]`, with the
  table named directly: the cartridge turns each literal string into a ROM table that way.
  The character held in a local, a helper doing the lookup, or a table passed as a parameter
  or reached through a field takes the slow path.
- A library that writes a background map checks the cell against `map.width` and
  `map.height` and stores the address as `{at = map.map + y*32 + x}`. The cartridge then
  proves the VBlank upload fits without a `---@snes vblank` note. A guard such as
  `x + len > map.width` keeps a span inside the map. Write an address that steps back as
  `base - i`, not `at = at - 1`, which is not proven.
- A `makeFixed` anywhere in a library turns on fixed-point arithmetic for every toy that
  requires it. Use whole numbers unless the library needs fractions.

## Adding a library without touching another's lines

Several libraries are written at the same time, so each one owns its own lines:

1. `lib/<name>.luau`: the new file. This is the only file that is new for each library.
2. `src/ppu.d.luau`: the Libraries section at its end already has a `-- @ppu/<name>` header
   line for each planned library. Write your block directly below your own header: a
   `type ppu_<name> = {` record with one field per line, closed by `}` at the start of a line.
   The Studio's completion reads `name.` members from this block. Do not edit another
   library's block. For a library with no header yet, add one in alphabetical order.
3. The SNES guide's "Engine libraries" section has a `<!-- @ppu/<name> -->` line for each
   planned library. Write your section directly below your own marker.
4. Tests go in a new test file of your own, not at the end of a shared one.
