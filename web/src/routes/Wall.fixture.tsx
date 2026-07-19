import { useEffect, useState } from "react";
import { http, HttpResponse } from "msw";
import { worker } from "../mocks/browser";
import { makeWallPage } from "../fixtures";
import { Wall } from "./Wall";
import { RouterStage } from "../cosmos/FixtureStage";

// Page story: <Wall/> fetches getWall() internally; the global MSW worker answers
// GET /api/toys from the shared fixtures. No payload is prop-injected. The Empty
// variant overrides the handler at the network seam (still MSW), not the props.
const Default = () => <RouterStage><Wall /></RouterStage>;

const Empty = () => {
  // Apply the override during render, before <Wall/>'s fetch effect runs; reset
  // the handlers when the story unmounts so it doesn't leak into other stories.
  useState(() => {
    worker.use(http.get("/api/toys", () => HttpResponse.json(makeWallPage({ toys: [], nextPage: null }))));
    return null;
  });
  useEffect(() => () => worker.resetHandlers(), []);
  return <RouterStage><Wall /></RouterStage>;
};

export default {
  Default,
  Empty,
};
