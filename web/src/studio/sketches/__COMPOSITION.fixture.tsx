import type { CSSProperties } from "react";
import { OverlayStage } from "../../cosmos/FixtureStage";
import { libraryOpenState, sketchMetaList } from "../../fixtures";
import { LibraryPanel } from "./LibraryPanel";
import { LibraryDataProvider } from "./useLibrary";
import "./sketches.css";

export default (
  <OverlayStage style={{ "--rail-w": "0px", "--toolbar-h": "0px" } as CSSProperties}>
    <LibraryDataProvider data={{ sketches: sketchMetaList, open: libraryOpenState }}>
      <LibraryPanel onClose={() => undefined} />
    </LibraryDataProvider>
  </OverlayStage>
);
