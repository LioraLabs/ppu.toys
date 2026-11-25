---
id: ppu-connect
kind: concept
title: ppu connect keeps a folder and a site project in sync, so the open Studio shows each save
codes: []
---
`ppu connect <project link> <dir>` pulls one of your ppu.toys projects into a new folder and keeps
running: each save in the folder becomes a project save the open Studio picks up, and each save in
the Studio is written into the folder. Inside a linked folder (one with `.ppu/link`) it needs no
arguments. It syncs the files and sources the folder's ppu.json names; a file ppu.json does not list
stays local ([[connected-folder|what a connected folder syncs]]).

A file changed in both places is asked about in the Studio's Connect panel; `--keep folder` or
`--keep site` settles the ones found at start without asking. Sign in first with `ppu login`. Edit,
save, then [[ppu-check|ppu check .]] locally; the Studio shows the same toy. Run it as `ppu connect` inside a linked folder.
