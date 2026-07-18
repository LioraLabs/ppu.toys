import type { ReactNode, CSSProperties } from "react";
import type { StoryDecorator } from "@ladle/react";
import { MemoryRouter } from "react-router-dom";

// Per-story decorator (opt-in via a story's `decorators` array) so only
// stories that render <Link> pull in a router.
export const withRouter: StoryDecorator = (Component) => (
  <MemoryRouter>
    <Component />
  </MemoryRouter>
);

// Stage for stories whose component renders a `position: fixed` scrim/overlay
// (modals, the sketch library drawer). Ladle renders stories in the SAME
// document as its sidebar (no iframe), so a viewport-fixed scrim would cover the
// nav and swallow every click — trapping you on that one story. The `transform`
// makes this wrapper the containing block for its fixed descendants, so the
// overlay fills only the story pane and the sidebar stays clickable. `minHeight`
// gives the pane a viewport-sized box (fixed children collapse it to 0
// otherwise, which also breaks the screenshot target). Extra `style` lets a
// story zero shell CSS vars (e.g. --rail-w) the overlay positions against.
export function OverlayStage({ children, style }: { children: ReactNode; style?: CSSProperties }) {
  return (
    <div style={{ position: "relative", transform: "translateZ(0)", minHeight: "100vh", ...style }}>
      {children}
    </div>
  );
}
