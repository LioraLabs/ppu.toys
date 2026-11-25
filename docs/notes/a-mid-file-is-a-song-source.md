---
id: a-mid-file-is-a-song-source
kind: recipe
title: A .mid file in the folder is a song source, converted the way the Studio imports one
codes: []
---
A song source's `file` may be a `.mid`. It becomes a song each time the toy loads, by the same
conversion as the Studio's MIDI import: each General MIDI program family maps to one built-in sound,
drum-channel keys to the drum presets, and note times round to the 1/48-beat grid. The .mid stays the
person's file: an edit made in the Studio lands beside it as `<stem>.song.json` and ppu.json points at
that from then on ([[a-song-json-file-is-a-song-source]]), unless the edit uses what that JSON cannot
hold (nudges, voice pins, a key, a tempo per pattern), when it moves to a payload under `.ppu/sources/`. [[ppu-song-check|ppu song check]] and
[[ppu-song-roll|ppu song roll]] read a .mid directly, when its name ends in lowercase `.mid`.

```json
{"name": "theme", "kind": "song", "file": "music/theme.mid"}
```
