---
id: compose-by-measuring-what-you-cannot-hear
kind: concept
title: An agent composes by rounds of render, measure, change one thing for a number, and log it
codes: []
---
An agent writes music it never hears, so the song check ([[ppu-song-check|ppu song check]], or the Studio's `song_check`) stands in for
ears: it renders the song
alone with the bank's presets and prints numbers, voices over budget, rows above C6
([[song-pitch-tops-out-at-c6]]), held clashes, loudness per arrangement slot and per sound, and
clipped samples. Work in rounds. Render and measure, pick the one number that is wrong, change
one thing because of it, measure again. Then log the round in a line: what was measured, what
changed, what the re-measure said. [[ppu-song-roll|ppu song roll]] shows where a number lives, and
[[ppu-render-wav|ppu render --wav]] measures the whole toy as it plays. A change no number points at is a guess, and two changes in one
round leave no way to tell which one moved the meter.

When a question outruns the report (does the harmony mask the lead?), measure it directly: copy the
song without that part and compare the levels. What disappears is what the part owned. The check
leaves out the toy's own effects, echo and mixer, and echo adds level, so a song peaking near 0 dB
in the check clips once a toy adds its echo ([[echo-is-the-snes-texture]]).
