import {
  renameSketch,
  duplicateSketch,
  deleteSketch,
  type SketchMeta,
} from "./sketchStore";
import { openSketchStore } from "./openSketch";
import { useLibraryData } from "./useLibrary";
import { DEMOS } from "../demos/demos";
import "./sketches.css";

function timeAgo(ms: number): string {
  const s = Math.max(0, Math.floor((Date.now() - ms) / 1000));
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.floor(s / 60)}m ago`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
  return `${Math.floor(s / 86400)}d ago`;
}

/** The sketch library, mounted off the Files rail item. Plain chrome by
 *  design — the Workspace restyle ticket reworks the shell around it. Reads
 *  the sketch store through the useLibraryData seam so stories/tests can drive
 *  it from fixtures with no IndexedDB and no wasm core. */
export function LibraryPanel({ onClose }: { onClose: () => void }) {
  const { sketches, open } = useLibraryData();
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
        {DEMOS.map((d) => {
          const isOpen = open.context.kind === "demo" && open.context.demoId === d.id;
          return (
            <li key={d.id} className={"library-row" + (isOpen ? " library-row--open" : "")}>
              <button
                type="button"
                className="library-open"
                onClick={() => {
                  openSketchStore.openDemo(d.id).catch(logErr("open demo failed"));
                  onClose();
                }}
              >
                <span className="library-name">{d.label}</span>
                <span className="library-updated">demo · read-only</span>
              </button>
            </li>
          );
        })}
      </ul>
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
