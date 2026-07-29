import "dockview-react/dist/styles/dockview.css";
import "../styles/tokens.css";
import "./studio.css";
import { createContext, useContext, useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { DockviewReact } from "dockview-react";
import type { DockviewApi, DockviewReadyEvent } from "dockview-react";

/** The four studio panels. Every layout operation speaks in these ids. */
export type PanelId = "editor" | "assets" | "output" | "inspector";

export type LayoutPreset = "default" | "code" | "showcase";

export interface StudioDockProps {
  /** Panel bodies, injected as slots (wired app or fixture mocks). */
  editor: ReactNode;
  assets: ReactNode;
  output: ReactNode;
  inspector: ReactNode;
  /** Receives the live dockview api once ready (LayoutMenu consumes it). */
  onApi?: (api: DockviewApi) => void;
}

const LAYOUT_KEY = "ppu.dockLayout.v1";

const PANEL_TITLES: Record<PanelId, string> = {
  editor: "CODE",
  assets: "ASSETS",
  output: "OUTPUT",
  inspector: "INSPECTOR",
};

/** Slot content rides through context so the dockview component map can stay
 *  module-stable (a changing `components` prop identity would remount panels). */
const SlotContext = createContext<Record<PanelId, ReactNode>>({
  editor: null,
  assets: null,
  output: null,
  inspector: null,
});

function slotPanel(id: PanelId) {
  return function SlotPanel() {
    const slots = useContext(SlotContext);
    return <div className="dock-panel-body">{slots[id]}</div>;
  };
}

const COMPONENTS = {
  editor: slotPanel("editor"),
  assets: slotPanel("assets"),
  output: slotPanel("output"),
  inspector: slotPanel("inspector"),
};

/** Rebuild one of the predefined arrangements from scratch. Code and assets
 *  share a tab group (the "toggle" pair); output owns the right; inspector,
 *  when present, docks under the code group. */
export function applyPreset(api: DockviewApi, preset: LayoutPreset) {
  api.clear();
  const editor = api.addPanel({
    id: "editor",
    component: "editor",
    title: PANEL_TITLES.editor,
  });
  api.addPanel({
    id: "assets",
    component: "assets",
    title: PANEL_TITLES.assets,
    position: { referencePanel: "editor" },
  });
  const output = api.addPanel({
    id: "output",
    component: "output",
    title: PANEL_TITLES.output,
    position: { referencePanel: "editor", direction: "right" },
  });
  if (preset === "default") {
    const inspector = api.addPanel({
      id: "inspector",
      component: "inspector",
      title: PANEL_TITLES.inspector,
      position: { referencePanel: "editor", direction: "below" },
    });
    inspector.api.setSize({ height: 280 });
  }
  output.api.setSize({ width: preset === "showcase" ? Math.round(window.innerWidth * 0.6) : 620 });
  if (preset === "showcase") output.api.setActive();
  else editor.api.setActive();
}

/** Re-add one closed panel at its home position (used by the layout menu). */
export function reopenPanel(api: DockviewApi, id: PanelId) {
  if (api.getPanel(id)) {
    api.getPanel(id)?.api.setActive();
    return;
  }
  const anchor = (...ids: PanelId[]) => ids.find((p) => api.getPanel(p));
  const home: Record<PanelId, () => void> = {
    editor: () => {
      const tabRef = anchor("assets", "inspector");
      const position = tabRef
        ? { referencePanel: tabRef }
        : api.getPanel("output")
          ? { referencePanel: "output", direction: "left" as const }
          : undefined;
      api.addPanel({ id: "editor", component: "editor", title: PANEL_TITLES.editor, position });
    },
    assets: () => {
      const ref = anchor("editor");
      api.addPanel({
        id: "assets",
        component: "assets",
        title: PANEL_TITLES.assets,
        position: ref ? { referencePanel: ref } : undefined,
      });
    },
    output: () => {
      const ref = anchor("editor", "inspector", "assets");
      const p = api.addPanel({
        id: "output",
        component: "output",
        title: PANEL_TITLES.output,
        position: ref ? { referencePanel: ref, direction: "right" } : undefined,
      });
      p.api.setSize({ width: 620 });
    },
    inspector: () => {
      const ref = anchor("editor", "assets", "output");
      const p = api.addPanel({
        id: "inspector",
        component: "inspector",
        title: PANEL_TITLES.inspector,
        position: ref ? { referencePanel: ref, direction: "below" } : undefined,
      });
      p.api.setSize({ height: 280 });
    },
  };
  home[id]();
}

/** The dockable studio shell: four movable/closable/tab-stackable panels with
 *  a persisted user layout (ppu.dockLayout.v1) and preset arrangements. This
 *  replaced StudioLayout + the ActivityRail + three bespoke splitters. */
export function StudioDock({ editor, assets, output, inspector, onApi }: StudioDockProps) {
  const slots = { editor, assets, output, inspector };
  const saveTimer = useRef<number | null>(null);

  const onReady = (event: DockviewReadyEvent) => {
    const api = event.api;
    let restored = false;
    const saved = localStorage.getItem(LAYOUT_KEY);
    if (saved) {
      try {
        api.fromJSON(JSON.parse(saved));
        restored = true;
      } catch {
        localStorage.removeItem(LAYOUT_KEY);
      }
    }
    if (!restored) applyPreset(api, "default");
    api.onDidLayoutChange(() => {
      if (saveTimer.current !== null) window.clearTimeout(saveTimer.current);
      saveTimer.current = window.setTimeout(() => {
        localStorage.setItem(LAYOUT_KEY, JSON.stringify(api.toJSON()));
      }, 300);
    });
    onApi?.(api);
  };

  useEffect(
    () => () => {
      if (saveTimer.current !== null) window.clearTimeout(saveTimer.current);
    },
    [],
  );

  return (
    <SlotContext.Provider value={slots}>
      <div className="studio-body dockview-theme-dark dockview-theme-ppu">
        <DockviewReact components={COMPONENTS} onReady={onReady} />
      </div>
    </SlotContext.Provider>
  );
}

/** Toolbar dropdown: preset arrangements + reopen/focus for each panel. */
export function LayoutMenu({ api }: { api: DockviewApi }) {
  const [open, setOpen] = useState(false);
  const [, bump] = useState(0);
  useEffect(() => {
    const d = api.onDidLayoutChange(() => bump((n) => n + 1));
    return () => d.dispose();
  }, [api]);
  const panels: PanelId[] = ["editor", "assets", "output", "inspector"];
  return (
    <div className="layout-menu">
      <button type="button" className="btn-ghost" onClick={() => setOpen((o) => !o)}>
        Layout ▾
      </button>
      {open && (
        <>
          <div className="layout-menu-scrim" onClick={() => setOpen(false)} />
          <div className="layout-menu-pop">
            <div className="layout-menu-head">PRESETS</div>
            {(["default", "code", "showcase"] as LayoutPreset[]).map((p) => (
              <button
                key={p}
                type="button"
                className="layout-menu-item"
                onClick={() => {
                  applyPreset(api, p);
                  setOpen(false);
                }}
              >
                {p}
              </button>
            ))}
            <div className="layout-menu-head">PANELS</div>
            {panels.map((id) => {
              const openNow = !!api.getPanel(id);
              return (
                <button
                  key={id}
                  type="button"
                  className="layout-menu-item"
                  onClick={() => reopenPanel(api, id)}
                >
                  <span className="layout-menu-tick">{openNow ? "✓" : ""}</span>
                  {PANEL_TITLES[id]}
                </button>
              );
            })}
          </div>
        </>
      )}
    </div>
  );
}
