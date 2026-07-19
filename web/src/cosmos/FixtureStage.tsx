import { useEffect, useState } from "react";
import type { CSSProperties, ReactNode } from "react";
import { MemoryRouter } from "react-router-dom";
import { initCore } from "../ppu/instance";

export function RouterStage({ children }: { children: ReactNode }) {
  return <MemoryRouter>{children}</MemoryRouter>;
}

export function OverlayStage({ children, style }: { children: ReactNode; style?: CSSProperties }) {
  return (
    <div style={{ position: "relative", transform: "translateZ(0)", minHeight: "100vh", ...style }}>
      {children}
    </div>
  );
}

let corePromise: Promise<void> | null = null;

export function CoreStage({ children }: { children: ReactNode }) {
  const [ready, setReady] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    (corePromise ??= initCore())
      .then(() => live && setReady(true))
      .catch((reason) => live && setError(reason instanceof Error ? reason.message : String(reason)));
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
  return <>{children}</>;
}
