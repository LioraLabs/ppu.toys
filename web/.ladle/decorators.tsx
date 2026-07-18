import { useEffect, useState } from "react";
import type { ComponentType, ReactNode, CSSProperties } from "react";
import type { StoryDecorator } from "@ladle/react";
import { MemoryRouter } from "react-router-dom";
import { initCore } from "../src/ppu/instance";

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

// Opt-in: boot the REAL wasm PPU core before the story mounts, so a story can
// exercise genuine core logic (e.g. AddSourceDialog's `convertSource` image
// import, or a live OutputCanvas). Most stories stay wasm-free by design — add
// this only to stories that need the core. `initCore()` loads the same wasm the
// app does (Ladle reuses the app's Vite config), and is shared across every
// live-core story via one cached promise so the module instantiates once.
let corePromise: Promise<void> | null = null;

export const withCore: StoryDecorator = (Component) => <CoreBoot Component={Component} />;

function CoreBoot({ Component }: { Component: ComponentType }) {
  const [ready, setReady] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    (corePromise ??= initCore())
      .then(() => live && setReady(true))
      .catch((e) => live && setError(e instanceof Error ? e.message : String(e)));
    return () => {
      live = false;
    };
  }, []);
  if (error) {
    return <div style={{ padding: 16, fontFamily: "system-ui", color: "#ff5d6a" }}>PPU core failed to load: {error}</div>;
  }
  if (!ready) {
    return <div style={{ padding: 16, fontFamily: "system-ui", color: "var(--mid, #9aa1ae)" }}>Booting the PPU core…</div>;
  }
  return <Component />;
}
