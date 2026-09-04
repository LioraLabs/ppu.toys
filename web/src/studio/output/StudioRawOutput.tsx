import { useEffect, useState } from "react";
import { ReadOnlyPlayer, type PlayerSource } from "../../components/ReadOnlyPlayer";
import type { SketchFile } from "../sketches/sketchStore";
import { useDocumentTitle } from "../../routes/useDocumentTitle";
import { transport } from "../transport/transport";

export const STUDIO_RAW_CHANNEL = "ppu-toys-studio-raw";

export interface StudioRawState {
  type: "state";
  files: SketchFile[];
  sources: PlayerSource[];
}
type StudioRawMessage =
  | StudioRawState
  | { type: "clock"; t: number; playing: boolean }
  | { type: "request" | "toggle" | "restart" };

export function StudioRawOutput() {
  const [state, setState] = useState<StudioRawState | null>(null);
  useDocumentTitle("Studio raw output");

  useEffect(() => {
    if (typeof BroadcastChannel === "undefined") return;
    const channel = new BroadcastChannel(STUDIO_RAW_CHANNEL);
    channel.onmessage = (event: MessageEvent<StudioRawMessage>) => {
      if (event.data?.type === "state") setState(event.data);
      else if (event.data?.type === "clock") {
        transport.setPlaying(false);
        transport.seek(event.data.t);
      }
    };
    channel.postMessage({ type: "request" });
    const control = (event: KeyboardEvent) => {
      if (event.repeat) return;
      if (event.code === "Space") channel.postMessage({ type: "toggle" });
      else if (event.code === "KeyR") channel.postMessage({ type: "restart" });
      else return;
      event.preventDefault();
    };
    window.addEventListener("keydown", control);
    return () => {
      window.removeEventListener("keydown", control);
      channel.close();
    };
  }, []);

  return (
    <main className="raw-output">
      {state ? (
        <ReadOnlyPlayer files={state.files} sources={state.sources} raw rawKeys={false} />
      ) : (
        <p className="raw-output-status" role="status">
          Waiting for Studio…
        </p>
      )}
    </main>
  );
}
