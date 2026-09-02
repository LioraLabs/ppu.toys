import { useState } from "react";
import { MemoryRouter } from "react-router-dom";
import { OverlayStage } from "../../cosmos/FixtureStage";
import { PublishDialog } from "./PublishDialog";
import { openSketchStore, useOpenSketch } from "../sketches/openSketch";
import { sessionStore } from "../../api/session";
import type { SketchOrigin } from "../sketches/sketchStore";
import "../studio.css"; // btn-ghost/btn-solid — the dialog's buttons style from the studio chrome
import "./cloud.css";

// PublishDialog reads origin (open-sketch store) and user (session store)
// itself now — no more `save` prop. MSW supplies /api/me (fixture `me`, id
// "1") for the session and /api/toys/:id (fixture `toyFull`) for the owned
// branch's prefill fetch.
const noop = () => undefined;

function Stage({ origin }: { origin?: SketchOrigin }) {
  const [ready] = useState(() => {
    openSketchStore._resetForTests();
    if (origin) openSketchStore.setOrigin(origin);
    void sessionStore.refresh();
    return true;
  });
  useOpenSketch();
  return ready ? (
    <MemoryRouter>
      <OverlayStage>
        <PublishDialog onClose={noop} />
      </OverlayStage>
    </MemoryRouter>
  ) : null;
}

export default {
  unlinked: <Stage />,
  owned: <Stage origin={{ id: "abc123", revision: 1, authorId: "1" }} />,
  forked: <Stage origin={{ id: "abc123", revision: 1, authorId: "2" }} />,
};
