---
id: ppu-song-roll
kind: recipe
title: ppu song roll draws a song as a piano roll PNG, to look at what was written
codes: []
fixture: twinstar
---
`ppu song roll <file | name> -o roll.png` draws the song: a lane per pitch, highest at the top, C lanes
shaded and drums below, one colour per sound, a line where each arrangement slot starts, and tails the
voice budget cuts dimmed. `--slots 2..4` draws only those slots. It reads what
[[ppu-song-check|ppu song check]] reads. Look at it to find the place a number pointed at: a lead
buried in the harmony's register, a bar with nothing on the beat, a repeat with no difference.

```sh
mkdir -p build
ppu song roll twinstar-battle -o build/roll.png
ppu song roll twinstar-battle -o build/start.png --slots 1..2
```
