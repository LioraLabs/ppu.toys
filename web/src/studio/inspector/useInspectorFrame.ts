import { useTransport } from "../transport/transport";
import type { FrameResult } from "../../ppu/core";

/** The inspector tabs read the SHARED transport frame — same core, same clock as
 *  the Output canvas and dock. */
export function useInspectorFrame(): FrameResult {
  return useTransport().frame;
}
