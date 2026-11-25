---
id: ppu-sfx
kind: recipe
title: ppu sfx makes a sound-effect WAV from a preset, with knobs in real units
codes: []
---
`ppu sfx <preset> -o sfx/name.wav` writes a short mono 32 kHz WAV for a sound the built-in bank lacks
([[sound-effects-are-short-one-shots]]). Presets: `coin`, `jump`, `hurt`, `explosion`, `powerup`,
`blip`, `laser`. Knobs override the preset: `--wave square|saw|sine|noise`, `--pitch` Hz, `--length`,
`--attack` and `--decay` in ms, `--sweep` in semitones over the sound, `--seed` for noise. The same
arguments always write the same file, and it prints the result as JSON (length, peak dBFS, the values
used). Without `-o` it writes `<preset>.wav`; it does not make the folder `-o` names. List the file in ppu.json as a sample
([[a-wav-file-is-a-sample-source]]) and play it through [[bank]].

```sh
mkdir -p sfx
ppu sfx laser --pitch 900 --length 200 --sweep -12 -o sfx/zap.wav
```
