import { useState } from "react";
import { OverlayStage } from "../../cosmos/FixtureStage";
import { openSketchStore, useOpenSketch } from "../sketches/openSketch";
import { makeSketchSource } from "../../fixtures";
import { AssetsPanel } from "./AssetsPanel";
import "../studio.css";

// AssetsPanel reads the open-sketch store (its production seam), so the
// stories drive that store rather than faking props: seed sources through the
// same addSource path the dialog/drop-zone use. OverlayStage bounds the
// fixed-position flyout.
function Seeded({ names }: { names: string[] }) {
  const [ready] = useState(() => {
    openSketchStore._resetForTests();
    for (const name of names) openSketchStore.addSource(makeSketchSource(name));
    return true;
  });
  useOpenSketch(); // re-render with the seeded store
  return ready ? <OverlayStage><AssetsPanel onClose={() => {}} /></OverlayStage> : null;
}

// Starter context: no assets yet — the teaching empty state.
const Empty = () => {
  const [ready] = useState(() => {
    openSketchStore._resetForTests();
    return true;
  });
  return ready ? <OverlayStage><AssetsPanel onClose={() => {}} /></OverlayStage> : null;
};

const WithAssets = () => <Seeded names={["sky", "hills", "hero"]} />;

export default {
  WithAssets,
  Empty,
};
