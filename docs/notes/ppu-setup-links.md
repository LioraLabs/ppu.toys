---
id: ppu-setup-links
kind: recipe
title: ppu setup once lets the Studio's Connect button start ppu connect with no terminal
codes: []
---
`ppu setup` registers `ppu://` links on Linux, macOS and Windows. After it, the Connect panel's Connect
button starts [[ppu-connect|ppu connect]] itself: in a new folder under `~/ppu/` (`%USERPROFILE%\ppu\` on Windows), or in a folder already
connected to that project anywhere on disk. Any other folder needs one `ppu connect` typed in a terminal
first; after that the button reconnects it. A machine not yet signed in gets the sign-in page first, and a
second click while `connect` starts or runs does nothing. `PPU_SITE=<url> ppu setup` makes links from that
site's Studio connect to it. On Windows the link runs `ppu-link.exe`, beside `ppu.exe` in the zip, so no
console window opens; on macOS `setup` makes a small `ppu-link.app` in `~/Applications` and the browser
asks once whether to open it. The install script runs `ppu setup` for you.

```sh
ppu setup
```
