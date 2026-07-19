# Agent UI loop

React Cosmos is the component workshop. Its tree follows the fixture paths under
`src`, so a component or composition has a stable source-shaped address:

```text
studio/__COMPOSITION
studio/editor/__COMPOSITION
studio/inspector/ComposeTab#Default
components/ToyCard#LongTitle
```

Use that address when scoping agent work: “Change only
`studio/inspector/ComposeTab#Default` and its component; do not touch its parent
composition or siblings.” Fixtures import production components; they do not
duplicate the UI implementation.

## Browse and drill down

```bash
cook cosmos
# or: npm --prefix web run cosmos
```

Cosmos builds its collapsible tree from colocated `*.fixture.tsx` files. A
fixture's default export is an object whose keys are the states/compositions
shown beneath that component:

```tsx
const Default = () => <ToyCard card={makeWallCard()} signedIn />;
const SignedOut = () => <ToyCard card={makeWallCard()} signedIn={false} />;

export default { Default, SignedOut };
```

A composition boundary is an expandable directory with a visible
`__COMPOSITION.fixture.tsx` child. The leading underscores keep the assembled
view visually distinct and sorted before ordinary component fixtures:

```text
studio/
  __COMPOSITION.fixture.tsx # renders the complete production Studio
  editor/
    __COMPOSITION.fixture.tsx # renders the complete production EditorPane
    CodeEditor.fixture.tsx  # expanding editor exposes its children
    FileTabs.fixture.tsx
```

Expand `studio` or `editor`, then select `__COMPOSITION` to view the children
assembled exactly as the site assembles them. The fixture must import the real
production composition; never duplicate its markup or maintain a fixture-only
copy.

Do not create a sibling fixture that collides with its directory, such as
`studio/editor.fixture.tsx` beside `studio/editor/`. Cosmos treats a row as
either a selectable fixture or an expandable directory, so that collision
hides the children. Every directory containing fixtures must include a
`__COMPOSITION.fixture.tsx` that assembles its real production components.
`npm test` discovers fixture directories recursively and enforces both rules.

Shared MSW startup and global styling live in `src/cosmos.decorator.tsx`.
`src/cosmos/FixtureStage.tsx` contains the exceptional wrappers:

- `CoreStage` initializes the real Rust/WASM PPU core.
- `OverlayStage` gives fixed-position dialogs a bounded preview stage.
- `RouterStage` supplies a `MemoryRouter` to components that render links but do
  not configure a route-specific router themselves.

Presentational fixtures should remain core-free. Wrap only rasterizer-bound
compositions in `CoreStage`:

```tsx
const LiveCore = () => (
  <CoreStage>
    <Studio />
  </CoreStage>
);
```

## Screenshot one composition

```bash
npm --prefix web run shoot -- 'studio/editor/__COMPOSITION' --build
```

The screenshot is written under `web/.shots/`. `--build` refreshes the static
Cosmos export first; omit it to reuse `web/build`.

Options:

```text
--out <path>
--build
--width <pixels>
--height <pixels>
--theme light|dark
```

The equivalent cached Cook workflow is:

```bash
STORY='studio/__COMPOSITION' cook shoot
```

If Chromium is missing, install it once:

```bash
cd web
npx playwright install chromium
```

## Verification

After changing a component or fixture:

```bash
npm --prefix web run typecheck
npm --prefix web test
npm --prefix web run cosmos:export
npm --prefix web run shoot -- 'path/Component#Variant'
```

Open the resulting PNG and inspect it. A successful process exit alone does not
prove the intended composition looks correct.
