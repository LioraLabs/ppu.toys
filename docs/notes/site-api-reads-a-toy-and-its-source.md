---
id: site-api-reads-a-toy-and-its-source
kind: concept
title: GET /api/toys/{id} returns a published toy with its code and assets, and /stats and /comments its numbers
codes: []
---
`GET /api/toys/{id}` returns one toy in full: `title`, `description`, `tags`, `state`, `revision`, `files`
(each `{name, source}`: the code), `sources` (the assets, each with a base64 `payload`), `heartCount`,
`forkedFrom`, `targetHz`, `romSize` (KiB, left out when automatic), `archived`, the latest `rom` (or null) and
`author`. A closed toy (no open source) comes with `files` and `sources` empty for anyone but its owner. An archived toy still loads here but is left out of lists. This is how a game's published code
is read as an example, with the caveat that a toy someone published shows how they wrote it, not a pattern
to copy; the templates are the exemplars. `GET /api/toys/{id}/stats` is `{"views", "downloads",
"comments"}`; `GET /api/toys/{id}/comments` lists newest first, 30 a page, and `?before={next}` gets the
next (`next` null at the end). For example, `curl 'https://ppu.toys/api/toys/abc123' | jq '{title, author: .author.handle, files: [.files[].name]}'` lists a toy's title, author and file names.
