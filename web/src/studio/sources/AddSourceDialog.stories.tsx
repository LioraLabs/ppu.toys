import type { Story, StoryDefault } from "@ladle/react";
import { AddSourceDialog } from "./AddSourceDialog";
import "./sources.css";

// AddSourceDialog rendered open. The wasm core is only touched once an image is
// dropped (ppuCore.convertSource) — in its initial empty state it renders the
// drop zone, kind/depth controls and the preview hint with no wasm. This is the
// documented wasm-free render surface: the convert + transport.addSource paths
// need the real core and are exercised by AddSourceDialog.test.tsx, not here.
export default {
  title: "Studio/Sources/AddSourceDialog",
} satisfies StoryDefault;

const noop = () => undefined;

// The scrim is position:fixed, so give #ladle-root a viewport-sized in-flow box
// (otherwise it collapses to 0 height and the screenshot target is "not visible").
export const Open: Story = () => (
  <div style={{ minHeight: "100vh" }}>
    <AddSourceDialog onClose={noop} />
  </div>
);
