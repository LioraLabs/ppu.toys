# SNES art for Aseprite

Opens a PNG that your project's `ppu.json` lists as SNES art, and flags bad tiles while you
draw. The verdicts come from `ppu art check`; the plugin never judges art itself.

The artist's notes: `ppu docs art` lists them.

## What it does

Open a PNG that a `ppu.json` above it names (a `bg`, `sheet`, `obj` or `m7` source):

- it stays **indexed**, and the source's `.gpl` (`options.palette`) is loaded as its palette;
- the **grid** is set to the tile size (the cell size for `obj`);
- the entries `ppu art check` lists as `transparent` (entry 0 of every palette row: 0, 16, 32 … at
  4 bpp, 0, 4, 8 … at 2 bpp, only 0 at 8 bpp and Mode 7; also 128 for Mode 7 EXTBG) are
  **transparent** when the sprite is opened. Nothing puts them back afterwards: ppu makes them
  transparent again when the palette is saved, and a wrong one is a finding like any other;
- a **SNES art** panel (a normal window, not modal) shows the palette in its real rows
  (Aseprite's own palette bar ignores the `.gpl`'s `Columns:`), the tile and palette budgets,
  a mini-map of the sprite with every offending tile or cell boxed in red, and the findings.

A PNG that no `ppu.json` lists is left alone. A listed PNG that cannot be opened as SNES art
(full colour, unknown palette name, missing `.gpl`, a source kind that is not art) shows a
message naming the problem.

After a stroke, and 120 ms with no further change, the plugin runs
`ppu art check <png> --stdin` **in the background**, feeding it the sprite's raw indices, and
shows the result when it lands. Drawing never waits for it. Saving is never blocked.

The check from stroke to highlight takes under 200 ms on a 1024x1024 Mode 7 PNG with a release
build of `ppu` (about 150 ms; a debug build is about 200 ms).

Opening runs `ppu art check <png> --json` once, synchronously (stopped after 20 s where the
system has `timeout`): that is where the kind, grid, palette row, transparent entries and `.gpl`
come from. If opening changed nothing (the PNG already had the `.gpl`'s palette and its
transparent entries), that check's findings fill the panel at once; otherwise opening started
the live check, so the panel never shows a finding that opening itself cured.

Reopen the panel with **Sprite > SNES art panel**.

## New SNES art

**File > New SNES art…** works with no sprite open. Pick the PNG's path, the kind (`bg`, `sheet`,
`obj`, `m7`), the size, the bit depth ("kind's default" unless you need 2 or 8), an obj cell size
(0 for the default), and a palette: a new one by name, or an existing `.gpl`. **Create** runs
`ppu art new`, which writes the PNG (every pixel index 0), the new `.gpl` if any, and the
`ppu.json` entry, then the PNG opens as SNES art with a passing check. The plugin writes none of
these files and knows none of the rules: if ppu refuses (the PNG exists, a size off the 8-pixel
grid, a bit depth the kind lacks, a PNG outside any project), its message is shown and nothing is
written. The PNG must be inside a project (a folder with a `ppu.json` above it).

## Convert to SNES art

Draw a PNG freely, in full colour or indexed, and have it converted from inside Aseprite:
**Sprite > Convert to SNES art…** on any PNG you have open, listed in a `ppu.json` or not (it
must still sit inside a project: outside one, ppu's refusal is shown before the dialog). The dialog asks for the kind, the bit depth (`bg`, `sheet`), the
cell (`obj`), the new palette's name, and the converter's own arguments: dither (none, bayer,
diffusion; not Mode 7), dither strength, alpha threshold and the palette cap (`bg`, `sheet`). A PNG the
project lists starts with its entry's values (ppu reports them in `ppu art check`); any other
starts at the converter's defaults.

**Convert** saves the sprite if it has unsaved changes, runs `ppu art convert` on the file, and
reopens the result as SNES art with a passing check. The plugin converts nothing and knows no
rule: it passes only the flags the kind reads, and only the values you changed, so the entry
keeps its own options and ppu its defaults (a new obj cell is square). You see the colours kept, the tiles and palettes
used out of what the art may hold, ppu's notes, and where the original went: `.ppu-backup/` in
the project (ppu keeps it). If ppu refuses (a palette name already taken, too many tiles), its
message is shown and the conversion changed no file (the save before it stands). The sprite
must have one frame and be inside a project (a `ppu.json` above it).

The conversion is a file operation, not an edit: there is no preview and no undo across it
(undo history of the old sprite is gone; copy the original back from `.ppu-backup/` to return).
Opening a full-colour project PNG offers this command.

## Editing palettes and rows

The plugin only moves bytes: `ppu` writes the `.gpl` and `ppu.json` and decides every remap
(`ppu art palette write`, `ppu art rows`, `ppu art palette set`; see the ppu CLI docs).

- **Change a colour** in Aseprite's palette, then **save the sprite**. Nothing touches the disk
  before that, the same as for pixels: an edit, an undo or a redo writes no file. On save the
  plugin hands the sprite's colours to `ppu`, which rewrites the `.gpl` (its `Name:`, `Columns:`
  and colour names kept; never Aseprite's own palette save), updates every PNG that names it, and
  the status says `<name>.gpl saved, N PNGs synced`. Colours are snapped to the SNES's 5 bits a
  channel; if that changed yours, the sprite takes the snapped colour as its own undo step,
  **Snap to SNES colours** (which leaves the sprite marked modified: Aseprite offers no way to
  say otherwise). A **Save As** to another path writes no `.gpl`; the panel says so.
  Art that names no palette keeps its colours in the PNG alone.
- **Set tile row** (panel button, or **Sprite > SNES art: set tile row**): select the tiles or
  cells with a rectangle, pick a colour of the row you want as the foreground colour, and every
  tile or cell the selection touches moves to that row. One undo step.
- **Fix tile rows**: every tile or cell takes the row most of its pixels use, and a pixel of another
  row moves to the same colour in that row. A pixel whose colour is not in the row stays, and the
  panel lists those tiles. One undo step; nothing to undo when nothing moved.
- **Select bad tiles** (panel button, or **Sprite > SNES art: select bad tiles**): the selection
  becomes the tiles or cells the last check flagged, so they are marked on the canvas itself.
  Findings that name no tile (a budget) are skipped; with nothing flagged it says so.
- **Switch palette**: not in the plugin. Run `ppu art palette set <png> <gpl>`, then reopen the PNG.

The row commands work on a sprite of one layer and one frame, and not on 8 bpp or Mode 7 art
(one 256-colour block has no rows). On art that names no palette they read the colours from the
saved PNG, so after a colour edit they refuse until you save the sprite.

## Install

Aseprite 1.3 or newer (needs scripting API 25: `json`, `Timer`, canvas dialogs, sprite events;
tested on Aseprite 1.3.18.6, Linux, the Arch Linux build, API 41). Older versions get the
message "SNES art needs Aseprite 1.3 or newer" and the plugin registers nothing.

- **`ppu art install aseprite`** puts the extension in Aseprite's user folder, over an older
  install: `extensions/ppu-art/` under `--dir`, else `ASEPRITE_USER_FOLDER`, else Aseprite's
  own folder (`~/.config/aseprite` on Linux). It prints the folder. Restart Aseprite.
- Or **Edit > Preferences > Extensions > Add Extension**, and pick a zip of this folder
  renamed `ppu-art.aseprite-extension` (`zip` the folder's `package.json` and
  `snes-art.lua`), or
- copy this folder into Aseprite's `extensions/` folder (**Help > Open Configuration Folder**).

Then put `ppu` on your `PATH`, or set `PPU_BIN` to its full path before starting Aseprite.

## The security prompt

Aseprite asks before a script reads or writes a file or runs a program. Installing an extension
does not trust it. The first time you open a project PNG you will see a **Security** dialog
naming `snes-art.lua`; tick **Give full trust to this script** and press Allow. You are asked
once. Everything the plugin touches (the project's `ppu.json` and `.gpl`, a temp folder
`ppu-art` for the check, the `ppu` program) is in that one file.

Without trust the plugin does not work; it has no fallback checker.

## Limits

- Windows is untested: the plugin has not been run there at all. It starts `ppu art job` with one
  quoted command line (an extra pair of quotes, for `cmd /c`), with no shell script, so the
  parts nobody has tried are that quoting with a path holding spaces, whether a console window
  flashes for each check, and the temp path (`app.fs.tempPath`). Linux is tested, macOS is not.
- One SNES-art sprite is watched at a time; opening another PNG moves the panel to it.
- Without `ppu` (not on `PATH`, no `PPU_BIN`), opening any PNG alerts once per file: whether a PNG
  belongs to a project is decided by `ppu`.
- Two Aseprite windows at once share the `ppu-art` temp folder.
- Opening a full-colour PNG as SNES art is refused; **Sprite > Convert to SNES art…** converts it
  (the original is kept in `.ppu-backup/`).
- A PNG you opened before it was added to `ppu.json` is not picked up until you choose
  **Sprite > SNES art panel**.
- Opening is one undo step ("Open as SNES art") that restores the file's own palette and grid.
  Coming back to a sprite that is already set up adds no step, unless another sprite saved new
  colours to its `.gpl` meanwhile: then it takes them in one "Open as SNES art" step.
- Two sprites on one `.gpl` each write the entries they edited: the other sprite takes the saved
  colours when you come back to it, keeping its own unsaved edits, so one save never undoes another's.

## Test

`sh test/run.sh` builds `ppu` from this checkout and runs `test/snes-art-test.lua` under
`aseprite -b`. It makes its own project in a temp folder.
