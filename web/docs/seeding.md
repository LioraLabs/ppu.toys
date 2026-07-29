# Seeding the official demo toys

The bundled demos ship to the wall as real published toys owned by a system
account (ShaderToy model: examples are just toys you fork). The studio no
longer lists them — this runbook is how they get onto a deployment.

## One-time per deployment

1. **Mint the official account + a session** (on the box):

   ```bash
   ssh <box>
   sudo -u ppu env $(sudo grep PPU_DB_PATH /etc/ppu/ppu-server.env) /opt/ppu/ppu-server mint-session ppu
   ```

   Prints a session id. The account is a system user (`sys:ppu`, handle `ppu`)
   with no Discord identity; the session expires after 7 days.

2. **Start the dev server locally, proxying to the deployment as that account:**

   ```bash
   PPU_API_TARGET=https://ppu.toys \
   PPU_API_COOKIE="ppu_sess=<session-id>" \
   pnpm --filter web run dev
   ```

   The cookie is attached by the Vite proxy — it never enters the browser.

3. **Open <http://localhost:5173/seed> and click "Seed all demos".**

   The page drives the real Studio underneath: opens each demo, waits for the
   core to render, serializes files + converted asset payloads, records the
   5-second loop clip, and publishes. Progress logs in the overlay.

   Re-running is safe: demos whose title the account has already published are
   skipped.

## Notes

- `/seed` is `import.meta.env.DEV`-gated — it does not exist in the prod bundle.
- Publishing needs a real browser tab (MediaRecorder) — keep the tab focused
  until "done" so the clips record at full rate.
- To reseed one demo after changing it: delete the published toy (admin), then
  rerun — only the missing title republishes.
