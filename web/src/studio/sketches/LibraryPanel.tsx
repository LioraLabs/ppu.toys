import { useEffect, useState } from "react";
import {
  listSketches,
  onSketchesChanged,
  renameSketch,
  duplicateSketch,
  deleteSketch,
  type SketchMeta,
} from "./sketchStore";
import { openSketchStore, useOpenSketch } from "./openSketch";
import "./sketches.css";

function timeAgo(ms: number): string {
  const s = Math.max(0, Math.floor((Date.now() - ms) / 1000));
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.floor(s / 60)}m ago`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
  return `${Math.floor(s / 86400)}d ago`;
}

function useSketchList(): SketchMeta[] {
  const [list, setList] = useState<SketchMeta[]>([]);
  useEffect(() => {
    let live = true;
    const refresh = () =>
      void listSketches().then((l) => {
        if (live) setList(l);
      });
    refresh();
    const off = onSketchesChanged(refresh);
    return () => {
      live = false;
      off();
    };
  }, []);
  return list;
}

/** The sketch library, mounted off the Files rail item. Plain chrome by
 *  design — the Workspace restyle ticket reworks the shell around it. */
export function LibraryPanel({ onClose }: { onClose: () => void }) {
  const sketches = useSketchList();
  const open = useOpenSketch();
  const openId = open.context.kind === "sketch" ? open.context.sketch.id : undefined;

  const logErr = (what: string) => (e: unknown) => console.error(what, e);

  const rename = (s: SketchMeta) => {
    const name = window.prompt("Rename sketch", s.name)?.trim();
    if (!name) return;
    // the OPEN sketch must be renamed through the live context — a direct
    // store write would be reverted by the next autosave flush
    if (s.id === openId) {
      openSketchStore.rename(name);
      openSketchStore.flush().catch(logErr("rename flush failed"));
    } else {
      renameSketch(s.id, name).catch(logErr("rename failed"));
    }
  };
  const duplicate = async (s: SketchMeta) => {
    // include any not-yet-flushed edits when duplicating the open sketch
    if (s.id === openId) await openSketchStore.flush();
    await duplicateSketch(s.id);
  };
  const remove = (s: SketchMeta) => {
    if (window.confirm(`Delete "${s.name}"?`))
      deleteSketch(s.id).catch(logErr("delete failed"));
  };

  return (
    <aside className="library" aria-label="Sketch library">
      <header className="library-head">
        <span className="library-title">SKETCHES</span>
        <button
          type="button"
          className="library-btn"
          onClick={() => openSketchStore.newSketch().catch(logErr("new sketch failed"))}
        >
          New
        </button>
        <button type="button" className="library-btn" onClick={onClose} aria-label="Close library">
          ×
        </button>
      </header>
      <ul className="library-list">
        {sketches.length === 0 && (
          <li className="library-empty">No sketches yet — edit a demo or hit New.</li>
        )}
        {sketches.map((s) => (
          <li key={s.id} className={"library-row" + (s.id === openId ? " library-row--open" : "")}>
            <button
              type="button"
              className="library-open"
              onClick={() => {
                openSketchStore.openSketch(s.id).catch(logErr("open sketch failed"));
                onClose();
              }}
            >
              <span className="library-name">{s.name}</span>
              <span className="library-updated">{timeAgo(s.updatedAt)}</span>
            </button>
            <span className="library-actions">
              <button type="button" className="library-btn" onClick={() => rename(s)}>
                Rename
              </button>
              <button
                type="button"
                className="library-btn"
                onClick={() => duplicate(s).catch(logErr("duplicate failed"))}
              >
                Dup
              </button>
              <button
                type="button"
                className="library-btn"
                onClick={() => remove(s)}
                disabled={s.id === openId}
                title={s.id === openId ? "Open — switch away first" : "Delete"}
              >
                Del
              </button>
            </span>
          </li>
        ))}
      </ul>
    </aside>
  );
}
