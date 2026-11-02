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

## Types for the editor

Add the library's type to the Libraries section at the end of `src/ppu.d.luau`, under a
`-- @ppu/<name>` header: a `type ppu_<name> = {` record with one field per line, closed by
`}` at the start of a line. The Studio completes a library local's `name.` members from it.
