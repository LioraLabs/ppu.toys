# Agent UI iteration loop

Verify one presentational component visually — fast, one command, without
booting the main app (`src/main.tsx`) or loading the wasm PPU core.

Presentational stories are pure props + fixtures, so a single Ladle story can
be rendered in isolation and screenshotted headlessly. That's what
`npm run shoot` does: it renders one story from the static `ladle build`
output using Playwright Chromium and writes a PNG.

## Prerequisites (one-time)

```bash
npm install
npx playwright install chromium
```

`playwright install chromium` downloads the Chromium binary to
`~/.cache/ms-playwright`, outside the repo. Skip it and `npm run shoot` will
fail trying to launch a browser that isn't there.

## The loop

1. Pick a component to verify, e.g. `src/components/ToyCard.tsx`.
2. Find its story file, colocated beside the component:
   `src/components/ToyCard.stories.tsx`. Find the fixture it draws props
   from in `src/fixtures/` (e.g. `makeWallCard`).
3. Edit the component and/or the fixture.
4. Rebuild the static Ladle output and shoot the story:
   ```bash
   npm run shoot -- <story-id> --build
   ```
5. Open the PNG at `web/.shots/<story-id>.png` and look at it.
6. Repeat from step 3.

**The #1 gotcha:** `shoot` screenshots the *static build* in `web/build/`,
not your live source. If you edit a component or fixture and re-run `shoot`
without rebuilding, you'll screenshot the stale build and see no change.
Always pass `--build` after an edit, or delete `web/build/` first — `shoot`
runs `ladle build` automatically when `web/build/meta.json` is missing.

## Worked example: ToyCard

Files involved:

- `src/components/ToyCard.tsx` — the component
- `src/components/ToyCard.stories.tsx` — its Ladle stories
- `src/fixtures/index.ts` — `makeWallCard`, the fixture factory the stories use

First, shoot it as-is to see the baseline:

```bash
npm run shoot -- components--toycard--default --build
```

This writes `web/.shots/components--toycard--default.png`. Open it — you'll
see the card frame with title "Dusk", author handle "ada", and a heart
button reading "3".

Now make a one-line edit to the fixture default in `src/fixtures/index.ts`:

```diff
-    title: "Dusk",
+    title: "Dawn",
```

Rebuild and re-shoot with the *same* command:

```bash
npm run shoot -- components--toycard--default --build
```

Open `web/.shots/components--toycard--default.png` again: the title text in
the meta row at the bottom of the card now reads "Dawn" instead of "Dusk".
That's the loop working — the PNG's byte size and pixel content changed
because the edit reached the static build before the shot was taken.

(Revert the fixture edit when you're done experimenting.)

## Story IDs

`web/build/meta.json` (produced by `ladle build`) is the source of truth for
story ids — its top-level `stories` object keys are exactly the ids `shoot`
accepts. An id is derived by kebab-casing the story's `title` plus its named
export: `title: "Components/ToyCard"` + export `Default` →
`components--toycard--default`.

The four ToyCard story ids:

- `components--toycard--default`
- `components--toycard--signed-out`
- `components--toycard--long-title`
- `components--toycard--high-heart-count`

If you pass an id `shoot` doesn't recognize, it prints the full list of
valid ids from `meta.json` and exits.

## Flags reference

```
npm run shoot -- <story-id> [--out <path>] [--build] [--width N] [--height N] [--theme light|dark]
```

- `--out <path>` — write the PNG somewhere other than the default
  `web/.shots/<story-id>.png` (relative paths resolve from `web/`).
- `--build` — force `ladle build` before shooting, even if `web/build/`
  already exists. Use this whenever you've edited a component or fixture.
- `--width N` / `--height N` — viewport size in pixels (default 1280x800).
  The capture is cropped to the rendered story content, not the full
  viewport, so this mostly matters for components whose layout responds to
  viewport size.
- `--theme light|dark` — force Ladle's light or dark theme for the shot.

## Media renders black — this is expected

ToyCard's clip/thumbnail area (`.toy-card-clip`, a `<video>` with a poster)
points at `/blobs/...` URLs. No backend or blob server runs in the
screenshot harness, so those requests 404 and that region renders as a
black rectangle in every shot. This is expected, not a bug — the
presentational chrome you're actually verifying with `shoot` is the card
frame and meta row (title, author handle, heart button/count) below it.

Network-backed data for full page stories is a separate concern, handled at
the network seam by MSW (mock service worker) as a parallel effort. The
screenshot harness itself doesn't know or care about that — it just renders
whatever the static build produces.
