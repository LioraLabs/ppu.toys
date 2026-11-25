---
id: site-api-serves-roms
kind: concept
title: GET /api/toys/{id}/roms lists hosted cartridge versions, and a download is refused when the author turned it off
codes: []
---
`GET /api/toys/{id}/roms` returns `{"hosting", "allowDownload", "versions": [...]}`, newest first; if the
author turned ROM hosting off, `hosting` is false and `versions` is empty. A version holds `version`, `url`,
`play`, `download`, `sha256`, `bytes`, `romSize`, `targetHz`, `sramSize`, `compiler` and `createdAt`.
`download` says whether you may download it: the author can turn "Allow ROM download" off, and then only
the author can, and `url` answers `404`. `play` serves the same image to the in-browser player either way.
`url` (`/api/toys/{id}/rom/{file}.sfc`) downloads a plain `.sfc`; it may answer `302` to a short-lived
signed link, so follow redirects.

Shown as text, not run (the notes test reads only this repository); replace `{id}` with the id of any
published toy, from `GET /api/toys`.

```text
curl -f 'https://ppu.toys/api/toys/{id}/roms'
```
