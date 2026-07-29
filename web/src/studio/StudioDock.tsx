import "dockview-react/dist/styles/dockview.css";
import "../styles/tokens.css";
import "./studio.css";
import { createContext, useContext, useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { DockviewReact } from "dockview-react";
import type { DockviewApi, DockviewReadyEvent } from "dockview-react";
import { INSPECTOR_TABS, type TabId } from "./inspector/tabs";

/** Every dockable page. The former inspector tabs are first-class panels —
 *  "the inspector" is just whatever tab group you keep them stacked in. */
export type PanelId = "editor" | "assets" | "output" | TabId;

export type LayoutPreset = "default" | "code" | "showcase";

export type DockSlots = Record<PanelId, ReactNode>;

export interface StudioDockProps {
  /** Panel bodies, injected as slots (wired app or fixture mocks). */
  slots: DockSlots;
  /** Receives the live dockview api once ready (LayoutMenu consumes it). */
  onApi?: (api: DockviewApi) => void;
}

/** v3: envelope { layout, known } — `known` records which panels existed
 *  when the layout was saved, so a panel added in a later release auto-joins
 *  a restored layout instead of silently not existing (a closed panel stays
 *  closed: it IS in `known`). v2 and earlier raw layouts are discarded. */
const LAYOUT_KEY = "ppu.dockLayout.v3";

export const INSPECTOR_PAGES: TabId[] = INSPECTOR_TABS.map((t) => t.id);

const PANEL_TITLES: Record<PanelId, string> = {
  editor: "CODE",
  assets: "ASSETS",
  output: "OUTPUT",
  ...Object.fromEntries(INSPECTOR_TABS.map((t) => [t.id, t.label.toUpperCase()])),
} as Record<PanelId, string>;

const ALL_PANELS: PanelId[] = ["editor", "assets", "output", ...INSPECTOR_PAGES];

/** Slot content rides through context so the dockview component map can stay
 *  module-stable (a changing `components` prop identity would remount panels). */
const SlotContext = createContext<Partial<DockSlots>>({});

function slotPanel(id: PanelId) {
  return function SlotPanel() {
    const slots = useContext(SlotContext);
    return <div className="dock-panel-body">{slots[id]}</div>;
  };
}

const COMPONENTS = Object.fromEntries(ALL_PANELS.map((id) => [id, slotPanel(id)]));

function addPanel(
  api: DockviewApi,
  id: PanelId,
  position?: Parameters<DockviewApi["addPanel"]>[0]["position"],
) {
  return api.addPanel({ id, component: id, title: PANEL_TITLES[id], position });
}

/** Rebuild one of the predefined arrangements from scratch. Code and assets
 *  share a tab group (the toggle pair); output owns the right; the inspector
 *  pages stack into one tab group under code (default preset only). */
export function applyPreset(api: DockviewApi, preset: LayoutPreset) {
  api.clear();
  const editor = addPanel(api, "editor");
  addPanel(api, "assets", { referencePanel: "editor" });
  const output = addPanel(api, "output", { referencePanel: "editor", direction: "right" });
  if (preset === "default") {
    const trace = addPanel(api, "trace", { referencePanel: "editor", direction: "below" });
    for (const page of INSPECTOR_PAGES.filter((p) => p !== "trace")) {
      addPanel(api, page, { referencePanel: "trace" });
    }
    trace.api.setActive();
    trace.api.setSize({ height: 280 });
  }
  output.api.setSize({ width: preset === "showcase" ? Math.round(window.innerWidth * 0.6) : 620 });
  if (preset === "showcase") output.api.setActive();
  else editor.api.setActive();
}

function isInspectorPage(id: PanelId): id is TabId {
  return (INSPECTOR_PAGES as PanelId[]).includes(id);
}

/** Re-add one closed panel at its home position (used by the layout menu). */
export function reopenPanel(api: DockviewApi, id: PanelId) {
  const existing = api.getPanel(id);
  if (existing) {
    existing.api.setActive();
    return;
  }
  const anchor = (...ids: PanelId[]) => ids.find((p) => api.getPanel(p));
  if (isInspectorPage(id)) {
    // join the surviving inspector group; else open a fresh one under code
    const sibling = INSPECTOR_PAGES.find((p) => p !== id && api.getPanel(p));
    if (sibling) {
      addPanel(api, id, { referencePanel: sibling });
      return;
    }
    const ref = anchor("editor", "assets", "output");
    const p = addPanel(api, id, ref ? { referencePanel: ref, direction: "below" } : undefined);
    p.api.setSize({ height: 280 });
    return;
  }
  if (id === "editor") {
    const tabRef = anchor("assets");
    const position = tabRef
      ? { referencePanel: tabRef }
      : api.getPanel("output")
        ? { referencePanel: "output", direction: "left" as const }
        : undefined;
    addPanel(api, "editor", position);
  } else if (id === "assets") {
    const ref = anchor("editor");
    addPanel(api, "assets", ref ? { referencePanel: ref } : undefined);
  } else {
    const ref = anchor("editor", "assets");
    const p = addPanel(
      api,
      "output",
      ref ? { referencePanel: ref, direction: "right" } : undefined,
    );
    p.api.setSize({ width: 620 });
  }
}

/** The dockable studio shell: every page is a movable/closable/tab-stackable
 *  panel with a persisted user layout (ppu.dockLayout.v2) and preset
 *  arrangements. Replaced StudioLayout + the ActivityRail + the Inspector
 *  chrome — a dockview tab group is the tab view. */
export function StudioDock({ slots, onApi }: StudioDockProps) {
  const saveTimer = useRef<number | null>(null);

  const onReady = (event: DockviewReadyEvent) => {
    const api = event.api;
    let restored = false;
    const saved = localStorage.getItem(LAYOUT_KEY);
    if (saved) {
      try {
        const env = JSON.parse(saved) as { layout: unknown; known?: string[] };
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        api.fromJSON(env.layout as any);
        // panels shipped since this layout was saved join at their home spot
        const known = new Set(env.known ?? []);
        for (const id of ALL_PANELS) {
          if (!known.has(id)) reopenPanel(api, id);
        }
        restored = true;
      } catch {
        localStorage.removeItem(LAYOUT_KEY);
      }
    }
    if (!restored) applyPreset(api, "default");
    api.onDidLayoutChange(() => {
      if (saveTimer.current !== null) window.clearTimeout(saveTimer.current);
      saveTimer.current = window.setTimeout(() => {
        localStorage.setItem(
          LAYOUT_KEY,
          JSON.stringify({ layout: api.toJSON(), known: ALL_PANELS }),
        );
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

/** Toolbar dropdown: preset arrangements + reopen/focus for every panel. */
export function LayoutMenu({ api }: { api: DockviewApi }) {
  const [open, setOpen] = useState(false);
  const [, bump] = useState(0);
  useEffect(() => {
    const d = api.onDidLayoutChange(() => bump((n) => n + 1));
    return () => d.dispose();
  }, [api]);
  const item = (id: PanelId) => (
    <button
      key={id}
      type="button"
      className="layout-menu-item"
      onClick={() => reopenPanel(api, id)}
    >
      <span className="layout-menu-tick">{api.getPanel(id) ? "✓" : ""}</span>
      {PANEL_TITLES[id]}
    </button>
  );
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
            {(["editor", "assets", "output"] as PanelId[]).map(item)}
            <div className="layout-menu-head">INSPECTOR</div>
            {INSPECTOR_PAGES.map(item)}
          </div>
        </>
      )}
    </div>
  );
}
