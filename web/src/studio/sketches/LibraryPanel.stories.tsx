import type { Story, StoryDefault } from "@ladle/react";
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
export default {
  title: "Studio/Sketches/LibraryPanel",
} satisfies StoryDefault;

const noop = () => undefined;

// A sketch is open (id matches the first row) → that row is highlighted and its
// Delete is disabled, exactly as in the app.
export const WithOpenSketch: Story = () => (
  <LibraryDataProvider data={{ sketches: sketchMetaList, open: libraryOpenState }}>
    <LibraryPanel onClose={noop} />
  </LibraryDataProvider>
);

// A demo is open → no saved row is highlighted.
export const DemoOpen: Story = () => (
  <LibraryDataProvider
    data={{ sketches: sketchMetaList, open: makeOpenSketchState({ kind: "demo", demoId: "dusk-parallax" }) }}
  >
    <LibraryPanel onClose={noop} />
  </LibraryDataProvider>
);

// Empty state: no saved sketches yet.
export const Empty: Story = () => (
  <LibraryDataProvider
    data={{ sketches: [], open: makeOpenSketchState({ kind: "demo", demoId: "dusk-parallax" }) }}
  >
    <LibraryPanel onClose={noop} />
  </LibraryDataProvider>
);
