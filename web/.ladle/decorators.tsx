import type { StoryDecorator } from "@ladle/react";
import { MemoryRouter } from "react-router-dom";

// Per-story decorator (opt-in via a story's `decorators` array) so only
// stories that render <Link> pull in a router.
export const withRouter: StoryDecorator = (Component) => (
  <MemoryRouter>
    <Component />
  </MemoryRouter>
);
