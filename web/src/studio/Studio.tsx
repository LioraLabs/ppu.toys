import { useEffect, useState } from "react";
import type { DockviewApi } from "dockview-react";
import { StudioDock, LayoutMenu, INSPECTOR_PAGES } from "./StudioDock";
import type { DockSlots } from "./StudioDock";
import { ToolbarWired } from "./ToolbarWired";
import { EditorPane } from "./EditorPane";
import { OutputCanvas } from "./output/OutputCanvas";
import { WIRED_INSPECTOR_PANELS } from "./inspector/panels";
import { AssetsPanel } from "./sources/AssetsPanel";
import { ErrorBoundary } from "./ErrorBoundary";
import { transport } from "./transport/transport";
import { openSketchStore, useOpenSketch, openContextLabel } from "./sketches/openSketch";
import { useDocumentTitle } from "../routes/useDocumentTitle";
import { STUDIO_RAW_CHANNEL, type StudioRawState } from "./output/StudioRawOutput";
import { TimelinePanel } from "./output/TimelinePanel";
import { timelineSettings } from "./output/timeline";

/** The wired studio: a toolbar over the dockable shell (StudioDock). Every
 *  page — CODE, ASSETS, OUTPUT and each inspector page — is its own panel the
 *  user arranges freely. The composition is available wasm-free via
 *  StudioDock.fixture (fixture-fed slots); this wired shell is available there
 *  too as the opt-in `CoreStage` live fixture. */
export function Studio() {
  const state = useOpenSketch();
  const { dirty } = state;
  const sketchName = openContextLabel(state);
  const [dockApi, setDockApi] = useState<DockviewApi | null>(null);
  useDocumentTitle(`${sketchName} · Studio`);

  useEffect(() => {
    void openSketchStore.initializeStarter();
  }, []);

  useEffect(
    () =>
      transport.subscribe(() => {
        const { loopIn, loopOut, looping } = timelineSettings.get();
        const current = transport.getSnapshot();
        if (current.playing && looping && loopOut > loopIn && current.t >= loopOut)
          transport.seek(loopIn);
      }),
    [],
  );

  useEffect(() => {
    if (typeof BroadcastChannel === "undefined") return;
    const channel = new BroadcastChannel(STUDIO_RAW_CHANNEL);
    const sendFiles = () => {
      const sketch = openSketchStore.state().context.sketch;
      const message: StudioRawState = {
        type: "state",
        files: sketch.files,
        sources: sketch.sources.map(({ name, payload }) => ({ name, payload })),
      };
      channel.postMessage(message);
    };
    const sendClock = () => {
      const { t, playing } = transport.getSnapshot();
      channel.postMessage({ type: "clock", t, playing });
    };
    channel.onmessage = (event) => {
      if (event.data?.type === "request") {
        sendFiles();
        sendClock();
      } else if (event.data?.type === "toggle") transport.toggle();
      else if (event.data?.type === "restart") transport.seek(0);
    };
    const unsubscribeFiles = openSketchStore.subscribe(sendFiles);
    const unsubscribeClock = transport.subscribe(sendClock);
    sendFiles();
    sendClock();
    return () => {
      unsubscribeFiles();
      unsubscribeClock();
      channel.close();
    };
  }, []);

  const slots: DockSlots = {
    editor: <EditorPane onSources={transport.setSources} />,
    assets: (
      <ErrorBoundary label="Assets">
        <AssetsPanel />
      </ErrorBoundary>
    ),
    output: (
      <ErrorBoundary label="Output">
        <OutputCanvas />
      </ErrorBoundary>
    ),
    timeline: <TimelinePanel />,
    ...Object.fromEntries(
      INSPECTOR_PAGES.map((id) => [
        id,
        <ErrorBoundary key={id} label={id}>
          {WIRED_INSPECTOR_PANELS[id]()}
        </ErrorBoundary>,
      ]),
    ),
  } as DockSlots;

  return (
    <div className="studio">
      <ToolbarWired
        sketchName={sketchName}
        dirty={dirty}
        layoutSlot={dockApi ? <LayoutMenu api={dockApi} /> : null}
        rawOutputHref="/studio/raw"
      />
      <StudioDock slots={slots} onApi={setDockApi} />
    </div>
  );
}
