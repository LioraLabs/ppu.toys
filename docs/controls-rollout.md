# Controls rollout: the hosted-content conversion, recorded and NOT run

**Status: NOT RUN. Nothing has been deployed and nothing in `~/dev/ppu-demos`
has been written.** This file is the record the conversion exists and what it
would do; running it is a deliberate, separate act.

## Why this record exists

`web/src/components/ReadOnlyPlayer.tsx` hands hosted files straight to the
transport — `transport.setSources(files)`, with no `convertLegacyFiles` call
anywhere on that path (`grep -n 'setSources(files)' web/src/components/ReadOnlyPlayer.tsx`).
So a toy published before this milestone still plays through the OLD
precedence in the read-only player, while the same toy opened in the Studio is
converted on open and renders through the new one. The two agree on
"the same controls document and execution order" only once the hosted corpus
is actually converted. Until then that is the one known parity gap, and it is
closed by running the rollout below — not by any further code change.

## The dry run (read-only, observed 2026-09-10)

Run against `~/dev/ppu-demos` at `2aa2002` ("Move demos to project
manifests"), with its uncommitted authoring work in place. The demos repo was
verified untouched before and after (`git status --short | wc -l` = 104 both
times); dry run is the default and `--write` was never passed.

|                                         | count                                     |
| --------------------------------------- | ----------------------------------------- |
| toys scanned                            | 34                                        |
| toys that gain a `controls.lua`         | 34 (all)                                  |
| legacy `pokes.lua` files converted      | 17                                        |
| legacy `timeline.lua` files converted   | 7                                         |
| explicit `apply_pokes()` calls stripped | 20 (one per toy, 20 `main.lua` rewritten) |
| refusals                                | 0 (exit 0)                                |

`transitions` is the only toy carrying both legacy files, so 24 legacy files
across 23 toys. The remaining 11 toys — `big-metasprite`,
`bovine-intervention`, `cavern-camera`, `dead-signal`, `dusk-parallax`,
`first-light`, `noise-drum`, `pad-piano`, `sprite-parade`, `star-patrol`,
`wavy-text` — carry no legacy file and only gain the minted `controls.lua`.

`pokes.lua` converted (17): `direct-color`, `extbg-direct-color`,
`first-note`, `glow`, `mode3-gradient`, `mode7-extbg`, `mode7-floor`,
`mosaic`, `offset-per-tile`, `parallax-skyline`, `split-screen`, `spotlight`,
`sprite-limits`, `sprite-storm`, `tilesheet-cavern`, `transitions`,
`translucency`.

`timeline.lua` converted (7): `asterion`, `cygnus-book-1`, `hemispheres`,
`mode7-road`, `ppu-logo`, `stage-lights`, `transitions`. ALL SEVEN are untracked in the demos
repo — `git -C ~/dev/ppu-demos ls-files | grep timeline` returns nothing — so
none of them are visible to a tracked-file listing. Only three surface
individually in `git status --short`, because they sit in otherwise-tracked toy
directories: `toys/mode7-road/timeline.lua`, `toys/stage-lights/timeline.lua`,
`toys/transitions/timeline.lua`. The other four live in wholly untracked
directories that status collapses to a single `??` line each. The converter
globs the filesystem, so it sees all seven regardless; it is the `git add -N`
step in the review below that this matters for.

Zero refusals means every legacy file cleared its own converter gate: the 17
`pokes.lua` through the `parsePokes`/`pokesToLua` round trip, the 7
`timeline.lua` through timeline parsing into a valid controls document. No toy
would be left with a kept, byte-untouched legacy file needing a hand fix.

## To actually run it

From this repo's `web/` directory, against a clean demos working tree:

```
node scripts/convert-controls.mjs --write ~/dev/ppu-demos/toys/*/
```

Then review with `git -C ~/dev/ppu-demos diff` (and `git add -N` first, since
several affected files are untracked), re-render the demos, and publish. Drop
`--write` to re-take the dry run above at any time. The script refuses loudly
per toy — exit 1, naming file and line — rather than guessing, so a non-zero
exit means some toy needs a hand fix before the batch is repeated.
