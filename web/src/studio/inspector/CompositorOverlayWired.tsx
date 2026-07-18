import { useCompositor } from "./compose/useCompositor";
import { screensFor } from "./compose/screens";
import { CompositorOverlay } from "./CompositorOverlay";

export function CompositorOverlayWired({ onCollapse }: { onCollapse: () => void }) {
  const c = useCompositor();
  return <CompositorOverlay onCollapse={onCollapse} c={c} screens={screensFor(c.frame)} />;
}
