---
id: a-song-json-file-is-a-song-source
kind: recipe
title: A .song.json file is a song source, the song as JSON an agent writes by hand
codes: []
---
A song is a source of kind `song` whose `file` is a `.song.json` (`{"name": "title", "kind": "song",
"file": "music/title.song.json"}` in ppu.json's `sources`), played by name with
[[snes.score|score{}]]. The JSON is the one the Studio's `write_song` tool takes: `tempo` in BPM,
optional `swing` (0 to 75), `rows` (`sound`, a MIDI `note` except on drums, `vol` 0 to 127, default
110, `pan` -100 to 100, default 0, and `keep`: true for a part no sound effect may take, which reserves
its voices for the whole song: [[voices-for-songs-and-effects]]), `patterns` (`name`, `length` and `notes`, each `at`, `row`, `len`
and `vel` 0 to 127, default 100), `arrangement` (pattern indexes, default every pattern in order) and
`loopStart` (the arrangement slot a loop returns to). Times are 1/48 of a beat: a 16th is 12, a 4/4 bar
192. It is strict: a key it does not know is refused by name, and numbers must be JSON numbers, not
strings.

A song made in the Studio's sequencer lands in a connected folder as `music/<name>.song.json`, and an
edit made in the Studio to a song read from a .mid lands beside it as `<stem>.song.json`, with ppu.json
moved to it ([[a-mid-file-is-a-song-source]]). A Studio edit this JSON cannot hold (nudges, voice
pins, a key, a tempo per pattern) moves the source to a payload under `.ppu/sources/` instead, and
`ppu connect` says the file is no longer used; the file stays on disk. After every change run [[ppu-song-check|ppu song check]].

```json
{"tempo": 120,
 "rows": [{"sound": "pluck", "note": 72, "keep": true}, {"sound": "kick"}],
 "patterns": [{"name": "A", "length": 192, "notes": [
   {"at": 0, "row": 0, "len": 24}, {"at": 0, "row": 1, "len": 12, "vel": 120}]}],
 "arrangement": [0, 0]}
```
