---
id: ppu-connect-clashes
kind: concept
title: ppu connect merges each entry on its own, asks only when both sides changed the same one, and backs up what it replaces
codes: []
---
The folder's `.ppu/link` records the last revision both sides agreed on, with a hash per file, source and
manifest field. Against it each entry is judged alone: changed on one side only, that side's copy wins;
changed on both sides in different entries, both are kept and saved as one revision; the same entry changed
on both sides (a file, a source, the title, the file order), you choose in the Studio's Connect panel:
Keep folder's or Take site's. Taking the site's moves the folder's copy to `.ppu-backup/<revision>/` first;
keeping the folder's overwrites the site's copy. A file added or deleted counts as a change; line endings alone do not. `--keep folder` or `--keep
site` settles the clashes found at start without asking; one found later, while it runs, is still asked
about. See [[ppu-connect]].
