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
      />
      <StudioDock slots={slots} onApi={setDockApi} />
    </div>
  );
}
