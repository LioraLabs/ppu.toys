import { useEffect, useState } from "react";
import { http, HttpResponse } from "msw";
import { worker } from "../../mocks/browser";
import { WorkspaceActions } from "./WorkspaceActions";
import { openSketchStore, useOpenSketch } from "../sketches/openSketch";
import type { SketchOrigin } from "../sketches/sketchStore";
import "../../styles/tokens.css";
import "../studio.css";
import "./cloud.css";

// The production toolbar cloud composition. MSW supplies the session seam.
function Stage({ origin }: { origin?: SketchOrigin }) {
  const [ready] = useState(() => {
    openSketchStore._resetForTests();
    if (origin) openSketchStore.setOrigin(origin);
    return true;
  });
  useOpenSketch();
  return ready ? (
    <div style={{ display: "flex", justifyContent: "flex-end", padding: 24 }}>
      <WorkspaceActions />
    </div>
  ) : null;
}

function SignedOut() {
  useState(() => {
    worker.use(http.get("/api/me", () => new HttpResponse(null, { status: 401 })));
    return null;
  });
  useEffect(() => () => worker.resetHandlers(), []);
  return <Stage />;
}

export default {
  unlinked: <Stage />,
  linked: <Stage origin={{ id: "abc123", revision: 1, authorId: "u1" }} />,
  signedOut: <SignedOut />,
};
