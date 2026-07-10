import { useInspectorFrame } from "./useInspectorFrame";
import { useCopyToast } from "./tracemem/copyToast";
import { ModeBadge, PlaneSeg, TraceCaption, TraceChain } from "./tracemem/TraceChain";
import "./tracemem/tracemem.css";

/** TRACE — the pixel-pipeline explorer. Plane is user-selected; mode is
 *  REPORTED from the live frame (M9 deviation — scripts own the mode). */
export function TraceTab() {
  const frame = useInspectorFrame();
  const { toast, copy } = useCopyToast();
  return (
    <div className="insp-scroll">
      <div className="tm-controls">
        <PlaneSeg />
        <ModeBadge frame={frame} />
      </div>
      <TraceCaption frame={frame} />
      <TraceChain frame={frame} copy={copy} variant="tab" />
      {toast}
    </div>
  );
}
