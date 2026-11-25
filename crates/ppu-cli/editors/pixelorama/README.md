# SNES art for Pixelorama

An extension that opens a ppu.toys project PNG as indexed SNES art and checks it while you draw.

- **Open**: File > Open, drag-drop or the command line. One `ppu art check` of the PNG decides: a PNG
  that belongs to a project and is SNES art opens indexed, with the PNG's own PLTE as the palette, one
  swatch row per sub-palette, and the grid at the tile (8x8) or obj cell size. The entries `ppu`
  reports as transparent (entry 0 of every row) are Pixelorama's transparent and cannot be edited. Swatches that share a
  colour are shown one step apart, so flips, rotations, effects, selection moves and image brushes
  keep every index; the PNG's palette is always the true colours. Any other PNG (outside a project,
  not listed in it, or one the import would quantize: a full-colour one) opens the normal way; a listed
  one says so in a toast and offers **Project > Convert to SNES art…**. A ppu that cannot run
  opens it the normal way too, with a toast.
- **256-colour art** (Mode 7, 8bpp): PNG index 0 is Pixelorama's "transparent" and swatch *k* is PNG
  index *k*+1, so every index 0 to 255 survives.
- **Live check**: after a stroke, undo or redo the extension runs `ppu art check` on the current
  pixels off the UI thread. Offending tiles are marked on the canvas. The **SNES art** strip, a row
  of its own between the tabs and the canvas (it never covers the art), shows `Checking…`, then
  `No findings: the art would import.` or `N findings`, and the budget line
  `Tiles: 40 / 512    Palettes: 2 / 8`. The findings, one per line as `<rule> [x,y]: <message>`,
  are its tooltip. `Check failed: <reason>` when `ppu` cannot run (cut to the strip's width, whole in the
  tooltip). No rule is decided in the extension.
- **Save**: Ctrl+S, File > **Save SNES art**, or Export as **SNES indexed PNG** write the indexed PNG
  (PLTE = the palette, tRNS = the transparent entries `ppu` reported). Only edited pixels change.

## Palette, rows

The extension only moves bytes: `ppu` decides every remap (`ppu art rows`; see the ppu CLI docs).

- **The palette is locked whole.** Any change to it in Pixelorama (a colour, undo, redo, order, size, a
  sort, another palette picked, a global palette edit) is put back, with the toast `The palette is locked
  for SNES art: put back.`, and nothing ever writes a `.gpl` or a PNG. Edit colours in the `.gpl`
  (Aseprite, a text editor, `ppu art palette write`); the tab takes them when the PNG is reopened.
  A colour that appears twice in the palette is shown one step off, so Pixelorama keeps the indices
  apart; the saved PNG has the true colours.
- **Art that names no palette** is recoloured by giving it one with `ppu art palette set` (or by editing
  the PNG's colours in Aseprite), then reopening it.
- **Switch palette**: not here. Run `ppu art palette set <png> <gpl>` and reopen the PNG.
- **Set tile row** (**Project > SNES art: set tile row**): select the tiles or cells with a rectangle,
  pick a colour of the row you want as the left colour in the palette, and every tile or cell the
  selection touches moves to that row. One undo step.
- **Fix tile rows** (**Project > SNES art: fix tile rows**): every tile or cell takes the row most of
  its pixels use, and a pixel of another row moves to the same colour in that row. A pixel whose colour
  is not in the row stays, and the toast lists those tiles. One undo step; nothing to undo when nothing
  moved.

The row commands work on art of one layer and one frame, and not on 8 bpp or Mode 7 art (one
256-colour block has no rows). They are refused, with a message and nothing changed, on a project that
is not SNES art. They wait for a running check first. `ppu` runs off the UI thread; if the art is
drawn on while it runs, a row command changes nothing and says so.

## New SNES art

**File > New SNES art…** works with no project open. Pick the PNG's path (inside a project: a folder with a
`ppu.json` above it), the kind (`bg`, `sheet`, `obj`, `m7`), the size, the bit depth ("kind's default" unless
you need 2 or 8), an obj cell size (0 for the default), and a palette: a new one by name, or an existing
`.gpl` (the two palette choices are exclusive: only the chosen one is used; the PNG and `.gpl` paths have
a Browse… button). **Create** runs `ppu art new` once, off the UI thread; it writes the PNG (every pixel index 0), the new
`.gpl` if any and the `ppu.json` entry, and the PNG then opens as SNES art with a passing check. The
extension writes none of these files and knows none of the rules: if ppu refuses (the PNG exists, a size off
the 8-pixel grid, a bit depth the kind lacks, a PNG outside any project), its message is shown as it is and
nothing is written.

## Convert to SNES art

**Project > Convert to SNES art…** works on any tab: a full-colour PNG, a fresh canvas you drew on, an indexed
PNG that is not SNES art. The dialog asks for the PNG's path (inside a project; Browse…), the kind, bit depth, obj
cell size, dither (none, bayer, diffusion), dither strength, alpha threshold, max palettes and the new palette's
name. When `ppu.json` lists the PNG the fields start as that source holds them and the kind is fixed; otherwise
they start at the converter's defaults. Only the fields the kind reads are on, and a field is passed to ppu only
when you changed it. Only the first layer of the current frame is converted. **Convert** saves those pixels to
the PNG (the tab's own file, or a new one; a typed path to another existing file is refused) unless the file
already holds them, runs `ppu art convert` once off the UI thread, closes the plain tab if it has one layer and
one frame (else it stays open), and reopens the PNG as SNES art with a passing check. A report shows the colours kept, tiles and palettes used, ppu's notes and where the
original went in `.ppu-backup/`. The extension converts nothing and knows no rule: if ppu refuses, its message is
shown as it is and no file other than the PNG's pixels changed. There is no undo across the conversion.

## Install

Needs Pixelorama **1.2.4 or newer** (older ones are refused with a message) and a current `ppu` on
`PATH` (`PPU_BIN` overrides it; one whose `--json` answers carry no `protocol` number is refused).

```sh
ppu art install pixelorama [--dir <Pixelorama user folder>]
```

writes `SnesArt.zip` (the files of `SnesArt`, as `src/Extensions/SnesArt/...`) into Pixelorama's
`extensions` folder (`~/.local/share/pixelorama` on Linux, `~/Library/Application Support/pixelorama` on
macOS, `%APPDATA%\pixelorama` on Windows), over an older copy. Restart Pixelorama and enable it once in
Preferences > Extensions. (Pixelorama's Preferences > Extensions > Add also takes that zip.)

Tested on Pixelorama 1.2.4, Linux. Windows is untested: the extension has not been run there at all. It
starts `ppu art job <folder>` directly with `OS.execute`, with no shell, so what nobody has tried is
that start on Windows (`ppu` found through `PATH`, a user data folder with spaces) and the job folder
under Pixelorama's user data folder. macOS is untested too.

## Limits

- Only the first layer's cel of the current frame is the art; other layers and frames are ignored.
- The File menu's own Save (and the quick-access save button) still write a `.pxo`; use Ctrl+S or
  Save SNES art.
- The palette is locked for SNES art (see Palette, rows): Pixelorama's own palette edits are undone, and Ctrl+Z of
  a deleted project palette is put back to the one palette under its own name. Picking another palette also
  rewrites Pixelorama's `last_palette` setting; the put-back sets it to the project's palette again.
- A stroke in a colour that is not on the palette snaps to the nearest swatch (Pixelorama 1.2.4 adds no swatch
  in indexed mode, which the tests assert).
- A *global* palette edit still writes Pixelorama's own copy of that palette to its user folder; only the
  project's palette is put back. The art, its `.gpl` and the PNG are not touched by it.
- The transparent entries are Pixelorama's transparent. When a tool moves transparent pixels, they are
  saved as the entry 0 the opened (or last saved) PNG held at their new position, else index 0.
  That never changes what the import makes of the art.

## Tests

`test/run.sh` runs Pixelorama in its own headless sway on fixture projects (`PIXELORAMA` names the
binary, `PPU_BIN` the `ppu` build) and exits non-zero on any failed assertion. It prints the run
directory with `results-final.json` and screenshots. The test zip is the one `ppu art install pixelorama` writes, plus `test/autotest.gd`, which
the shipped extension never holds; the shipped extension loads it only under `PPU_ART_TEST`.
