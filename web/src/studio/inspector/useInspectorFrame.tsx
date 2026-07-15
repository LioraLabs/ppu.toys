import { createContext, useContext, type ReactNode } from "react";
import { useTransport } from "../transport/transport";
import type { FrameResult } from "../../ppu/core";

/** Story/test override for the inspector frame. In the app NO provider is
 *  mounted, so the hook falls through to the live shared transport frame.
 *  Stories/tests wrap a panel in <InspectorFrameProvider frame={fixture}> to
 *  drive it from fixture data with no wasm core and no initCore(). */
const InspectorFrameContext = createContext<FrameResult | null>(null);

export function InspectorFrameProvider({
  frame,
  children,
}: {
  frame: FrameResult;
  children: ReactNode;
}) {
  return <InspectorFrameContext.Provider value={frame}>{children}</InspectorFrameContext.Provider>;
}

/** The inspector tabs read the SHARED transport frame — same core, same clock
 *  as the Output canvas and dock — UNLESS a fixture frame is injected via
 *  InspectorFrameProvider (stories/tests), in which case that wins. */
export function useInspectorFrame(): FrameResult {
  const injected = useContext(InspectorFrameContext);
  // Provider presence is fixed for a given mounted subtree (app: never;
  // story/test: always), so this early return keeps hook order stable while
  // avoiding a transport subscription — and thus any wasm read — on the
  // injected path.
  // eslint-disable-next-line react-hooks/rules-of-hooks
  if (injected) return injected;
  return useTransport().frame;
}
