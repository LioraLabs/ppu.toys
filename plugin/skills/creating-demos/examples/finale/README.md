# showcase-finale

The end card of the ppu.toys intro video, built live by the chip: 22 s, one
pass, then it holds on the logo with drifting embers.

| t (s) | On screen | Sound |
|---|---|---|
| 0–10 | One firefly blooms per bell/arp note until all 128 OAM entries fly: a tilted ring of sprites, turning in 3D (perspective projection in Lua, brightness frame by depth) | strings pad, bells, then pluck arpeggio + bass |
| 10.8–15 | The ring sweeps round and every sprite flies to its own pixel of the logo | kick enters, snare roll |
| 15.0 | Downbeat: the logo lands. White flash (fixed-colour add), BG1 mosaic 15→0, sprites burst outward as sparks | crash + kick + bells, Dadd9 |
| 16–22 | End card. Embers rise; the chord rings out through the echo | |

PPU: 128 sprites at 8×8 (OBSEL size 0), never more than 28 on a line
(`build_art.py` caps landing spots per row); OBJ palettes 4–7 so sprites take
colour math; `color.addend = "sub"` with BG1 on the sub screen and a per-line
COLDATA equal to the backdrop, so sparks ADD onto whatever is behind them;
per-line dithered backdrop gradient; mosaic. Audio: `score{}` playing
`finale.bin` (the committed song source; `song.json` is only a record of the
score, and editing it does not change the music) with built-in
`strings`/`pluck`/`bell`/`bass`/drums, echo delay 6.

Logo: the site's chip mark (`assets/chipmark.svg`) and a 7×10 pixel wordmark at 3×. Leaves the
bottom 60 px clear for a line of text.
