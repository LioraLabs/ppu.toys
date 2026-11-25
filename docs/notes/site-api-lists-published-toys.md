---
id: site-api-lists-published-toys
kind: concept
title: GET /api/toys lists published toys as cards, 24 a page, with no sign-in
codes: []
---
The site's read-only JSON API needs no sign-in and returns published toys only; it is the API the site
itself uses, unversioned, so fields may be added. Fields are camelCase and times are Unix seconds. An error
is a non-2xx status with `{"error": "message"}` (an unknown or unpublished toy is `404 {"error": "no such
toy"}`). `GET /api/toys` returns `{"toys": [...], "nextPage": 1}` newest first, `nextPage` null on the last
page. Query: `q` (title or author handle), `tag` (case-insensitive), `author` (exact handle), `sort=popular`
(most hearts first) and `page` from 0. A toy card holds `id`, `title`, `author` (`id`, `handle`, `avatar`),
`thumbUrl`, `clipUrl`, `heartCount`, `hearted` (always false when anonymous), `createdAt`, `tags`,
`hasRom`, `stage` and `source`. `GET /api/featured` gives `{"id": ...}` (null when none), `GET /api/highlights` the curated row
as cards, `GET /api/users/{handle}` a user with their `toys` and `archived`. Thumbnails and clips are
`/blobs/thumb/{id}` (PNG) and `/blobs/clip/{id}` (WebM), outside `/api`.

The command below is shown as text, not run: the notes test reads only this repository.

```text
curl -f 'https://ppu.toys/api/toys?sort=popular&tag=shooter'
```
