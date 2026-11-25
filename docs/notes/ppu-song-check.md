---
id: ppu-song-check
kind: recipe
title: ppu song check measures a song, the voice budget, rows above C6, clashes and loudness, without hearing it
codes: []
fixture: twinstar
---
`ppu song check <file | name>` prints the report the Studio's `song_check` tool gives: how many voices
sound at once against the budget and the notes it cuts, rows above C6
([[song-pitch-tops-out-at-c6]]), held clashes, and loudness in dBFS (RMS and peak) per arrangement slot
and per sound, with clipped samples. It takes a `.song.json`, a `.mid`, or the name of a song source in
the folder's ppu.json (`ppu song check [dir] <name>`). Piped, it prints JSON; it exits 1 when the song is
over the voice budget or above C6.

The song renders alone with the bank's presets: the toy's own `BANK` edits, echo and mixer are not in
it. Run it after every change and change one thing because of a number
([[compose-by-measuring-what-you-cannot-hear]]); [[ppu-song-roll|ppu song roll]] shows where, and
[[ppu-render-wav|ppu render --wav]] measures the toy as it plays.

```sh
ppu song check twinstar-title
ppu song check twinstar-battle > battle.json
```
