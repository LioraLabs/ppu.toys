---
id: ppu-login
kind: recipe
title: ppu login signs a machine in once with a code, and the token cannot publish
codes: []
---
`ppu login` prints a link and a short code. Open the link, sign in with Discord if asked, check the page
shows the same code and choose Allow; the terminal picks it up. The token is saved readable only by you
(`ppu/credentials.json` in the user config directory, or `PPU_CONFIG_DIR`). It can read, save, delete and
build your projects into cartridges ([[ppu-build]], [[ppu-connect]]); it cannot publish them and reaches
nothing else on the account. `ppu whoami` says who the machine is signed in as and on which site; `ppu
logout` revokes the token and deletes the file. Logging in again revokes the previous token. For another
site (a staging server) set `PPU_SITE` for `login` and for `connect`: `connect` and `build` refuse a machine
signed in to a different one. The Studio's Connect panel can sign every machine out.
