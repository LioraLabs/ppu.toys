import { useEffect, useState } from "react";
import type { Story, StoryDefault } from "@ladle/react";
import { http, HttpResponse } from "msw";
import { withRouter } from "../../.ladle/decorators";
import { worker } from "../mocks/browser";
import { makeWallPage } from "../fixtures";
import { Wall } from "./Wall";

// Page story: <Wall/> fetches getWall() internally; the global MSW worker answers
// GET /api/toys from the shared fixtures. No payload is prop-injected. The Empty
// variant overrides the handler at the network seam (still MSW), not the props.
export default {
  title: "Pages/Wall",
  decorators: [withRouter],
} satisfies StoryDefault;

export const Default: Story = () => <Wall />;

export const Empty: Story = () => {
  // Apply the override during render, before <Wall/>'s fetch effect runs; reset
  // the handlers when the story unmounts so it doesn't leak into other stories.
  useState(() => {
    worker.use(http.get("/api/toys", () => HttpResponse.json(makeWallPage({ toys: [], nextPage: null }))));
    return null;
  });
  useEffect(() => () => worker.resetHandlers(), []);
  return <Wall />;
};
