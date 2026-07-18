import { useCompositor } from "./compose/useCompositor";
import { screensFor } from "./compose/screens";
import { ComposeTab } from "./ComposeTab";

/** Wired container: the live compositor (seam-backed frame + pokes) plus the
 *  core's per-frame compositor screens. */
export function ComposeTabWired() {
  const c = useCompositor();
  return <ComposeTab c={c} screens={screensFor(c.frame)} />;
}
