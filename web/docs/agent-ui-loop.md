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

## Studio panels: presentational vs wired

The `src/studio/inspector/` tabs follow a two-tier split so that panels can
be storied and screenshotted without booting the wasm PPU core. This is the
durable convention for any new studio panel, not a one-off for the inspector.

**Presentational panel** — a pure function of a `FrameResult`-shaped prop. It
imports no `transport`, `ppuCore`, or `useInspectorFrame`; everything it
renders comes from its props. `RegistersTab.tsx` and `SpritesTab.tsx` are the
real examples: both are `({ frame }: { frame: FrameResult | null }) => ...`.
Because a presentational panel takes its data as a prop, it gets prop-driven
stories with no core involved.

**Wired container** — a thin component that reads the live singleton/hook and
passes the result down as props. `Inspector.tsx` is the real example: it
calls `const frame = useInspectorFrame()` once and passes `frame` to both
`<RegistersTab frame={frame} />` and `<SpritesTab frame={frame} />`. The
container itself stays too thin to be worth storying on its own — its job is
wiring, not rendering.

### Storying a presentational panel wasm-free

Pass the fixture straight in as a prop:

```tsx
<RegistersTab frame={frameResult} />
```

Import `frameResult` (a ready-made `FrameResult`) or `makeFrameResult`
(overridable factory) from `src/fixtures`. Use `makeFrameResult({ ... })` to
hit variant states without touching the component:

- `makeFrameResult({ objOverflow: { rangeOver: true, timeOver: true, maxSprites: 32, maxTiles: 34 } })`
  — triggers SpritesTab's RANGE OVER / TIME OVER badges.
- `makeFrameResult({ oam: frameResult.oam.map((s) => ({ ...s, on: false })) })`
  — every sprite off, SpritesTab's empty state.
- `frame={null}` — the panel's own "waiting for frame…" state.

No `initCore()`, no `ppuCore`, no MSW — the story never touches the wasm
core or the network, because the panel itself never does.

### The `useInspectorFrame` seam

A presentational panel is easy to story because it only takes props. A wired
container is harder, because it reads a singleton — but the singleton has a
story/test seam built in. `useInspectorFrame` (`src/studio/inspector/useInspectorFrame.tsx`)
checks a React context first and only falls back to subscribing to the live
transport (which requires the wasm core) if nothing was injected:

```tsx
export function useInspectorFrame(): FrameResult {
  const injected = useContext(InspectorFrameContext);
  if (injected) return injected;
  return useTransport().frame;
}
```

`InspectorFrameProvider` sets that context. Wrap a wired consumer in it with
a fixture frame and the hook returns the fixture instead of subscribing to
the transport — no wasm core loads, `initCore()` is never called. In the real
app no provider is mounted, so the app path is unchanged.

Worked example — `ViaInspectorFrameSeam` in `RegistersTab.stories.tsx`:

```tsx
function WiredRegisters() {
  const frame = useInspectorFrame();
  return <RegistersTab frame={frame} />;
}
// story:
<InspectorFrameProvider frame={frameResult}>
  <WiredRegisters />
</InspectorFrameProvider>
```

This renders identically to the direct-prop `Default` story — it exists to
prove the seam works, not because you'd normally story a container this way.
Prefer storying the presentational panel directly; reach for
`InspectorFrameProvider` when you specifically need to exercise a wired
consumer (e.g. a container you can't easily pull the prop out of, or a test
asserting the hook's fallback behavior).

### Recipe for a new studio panel

1. If the panel reads frame data, give it a `FrameResult`-shaped prop (e.g.
   `{ frame: FrameResult | null }`) and keep the component pure. Move any
   `useInspectorFrame()` call or other singleton read up into the container
   that renders it (e.g. `Inspector.tsx`), and pass the result down as a
   prop instead.
2. Colocate `Foo.stories.tsx` beside `Foo.tsx`. Keep stories props-only,
   drawing fixture data from `src/fixtures` (`frameResult`, `makeFrameResult`,
   or a new fixture factory added there if the panel needs different shape).
3. Shoot it: `npm run shoot -- <story-id> --build`, then open the PNG and
   eyeball it — see "The loop" above for the full cycle.

### Scope caveat: this is a vertical slice, not a studio-wide rewrite

Only `RegistersTab` and `SpritesTab` have been decoupled so far. Some other
inspector tabs also read `ppuCore` directly and are NOT yet storyable
wasm-free: `VramTab.tsx` calls `ppuCore.vram()` and `ppuCore.importReports()`
in its own body, and `MemoryTab.tsx` calls `ppuCore.vram()` (and reads
`useInspectorFrame()` itself, rather than taking `frame` as a prop). Neither
can render without a live core today.

Decouple those the same way — lift the `ppuCore` reads (and, for
`MemoryTab`, the `useInspectorFrame()` call) up into `Inspector.tsx` and pass
the results down as props — but only when you're next touching that panel.
Don't rewrite the whole studio in one pass just to make everything storyable.

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
