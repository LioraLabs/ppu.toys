import type { CSSProperties } from "react";
import { OverlayStage } from "../../cosmos/FixtureStage";
import { LibraryPanel } from "./LibraryPanel";
import { LibraryDataProvider } from "./useLibrary";
import {
  sketchMetaList,
  libraryOpenState,
  makeOpenSketchState,
} from "../../fixtures";
import "./sketches.css";

// LibraryPanel reads the sketch store through the useLibraryData seam. A story
// supplies fixture data via LibraryDataProvider so the panel renders the demo
// list + saved-sketch rows with no IndexedDB and no wasm core. The action
// handlers (open/rename/dup/delete) only fire on click, so a render is inert.
const noop = () => undefined;

// The .library aside is position:fixed against the shell's rail/toolbar vars.
// OverlayStage contains the fixed panel to the story pane (so it doesn't cover
// Cosmos preview); zeroing the shell vars pins the panel to the
// stage's top-left since there's no rail/toolbar in the story.
function Stage({ children }: { children: React.ReactNode }) {
  return (
    <OverlayStage style={{ "--rail-w": "0px", "--toolbar-h": "0px" } as CSSProperties}>
      {children}
    </OverlayStage>
  );
}

// A sketch is open (id matches the first row) → that row is highlighted and its
// Delete is disabled, exactly as in the app.
const WithOpenSketch = () => (
  <Stage>
    <LibraryDataProvider data={{ sketches: sketchMetaList, open: libraryOpenState }}>
      <LibraryPanel onClose={noop} />
    </LibraryDataProvider>
  </Stage>
);

// A demo is open → no saved row is highlighted.
const DemoOpen = () => (
  <Stage>
    <LibraryDataProvider
      data={{ sketches: sketchMetaList, open: makeOpenSketchState({ kind: "demo", demoId: "dusk-parallax" }) }}
    >
      <LibraryPanel onClose={noop} />
    </LibraryDataProvider>
  </Stage>
);

// Empty state: no saved sketches yet.
const Empty = () => (
  <Stage>
    <LibraryDataProvider
      data={{ sketches: [], open: makeOpenSketchState({ kind: "demo", demoId: "dusk-parallax" }) }}
    >
      <LibraryPanel onClose={noop} />
    </LibraryDataProvider>
  </Stage>
);

export default {
  WithOpenSketch,
  DemoOpen,
  Empty,
};
