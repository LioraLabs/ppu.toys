import { useEffect, useState } from "react";
import { http, HttpResponse } from "msw";
import { worker } from "../../mocks/browser";
import { WorkspaceActions } from "./WorkspaceActions";
import { openSketchStore, useOpenSketch } from "../sketches/openSketch";
import "../../styles/tokens.css";
import "../studio.css";
import "./cloud.css";

// The production toolbar cloud composition. MSW supplies the session seam.
function Stage() {
  const [ready] = useState(() => {
    openSketchStore._resetForTests();
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
  signedIn: <Stage />,
  signedOut: <SignedOut />,
};
