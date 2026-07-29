# Agent UI loop

React Cosmos is the component workshop. Its tree follows the fixture paths under
`src`, so a component or composition has a stable source-shaped address:

```text
studio
studio/editor
studio/editor#CompileError
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
# or: pnpm --filter web run cosmos
```

PPU Toys installs the scoped LioraLabs Cosmos packages from npm. It never links
npm directly to a Cosmos source checkout. The release workflow lives on the
fork's `lioralabs-npm` branch; build its publishable packages with
`npm run lioralabs:pack`, publish all three together, then update their exact
versions here together.

Cosmos builds its collapsible tree from colocated `*.fixture.tsx` files. A
fixture's default export is an object whose keys are the states/compositions
shown beneath that component:

```tsx
const Default = () => <ToyCard card={makeWallCard()} signedIn />;
const SignedOut = () => <ToyCard card={makeWallCard()} signedIn={false} />;

export default { Default, SignedOut };
```

A composition boundary is represented by a fixture beside a directory with the
same name. The forked Cosmos navigator makes that row both selectable and
expandable: click the label to render the composition, or the chevron to reveal
its children.

```text
studio.fixture.tsx          # clicking studio renders the production Studio
studio/
  editor.fixture.tsx        # clicking editor renders the production EditorPane
  editor/
    CodeEditor.fixture.tsx  # expanding editor exposes its children
    FileTabs.fixture.tsx
```

Every directory containing fixtures must have this same-name sibling fixture,
and that fixture must import the real production composition—never duplicate
its markup or maintain a fixture-only copy. `npm test` discovers fixture
directories recursively and enforces the convention.

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
pnpm --filter web run shoot 'studio/editor' --build
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
STORY='studio' cook shoot
```

If Chromium is missing, install it once:

```bash
cd web
npx playwright install chromium
```

## Verification

After changing a component or fixture:

```bash
pnpm --filter web run typecheck
pnpm --filter web test
pnpm --filter web run catalog
pnpm --filter web run shoot 'path/Component#Variant'
```

Open the resulting PNG and inspect it. A successful process exit alone does not
prove the intended composition looks correct.
