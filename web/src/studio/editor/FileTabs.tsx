import { useRef, useState } from "react";
import "./editor-tabs.css";

export interface FileTabsProps {
  /** Ordered file names — order IS execution order (PICO-8). */
  files: string[];
  active: string;
  /** Files whose tab shows an error dot (shown on inactive tabs only —
   *  the active tab shows its errors inline in the editor). */
  errorFiles: ReadonlySet<string>;
  onSelect: (name: string) => void;
  onAdd: () => void;
  /** Store-validated rename; returns false when rejected (dup/empty). */
  onRename: (from: string, to: string) => boolean;
  onDelete: (name: string) => void;
  /** Move files[from] to index `to` (drag-reorder). */
  onReorder: (from: number, to: number) => void;
}

/** The editor's file tab bar (handoff "Code editor"): CRUD + drag-reorder +
 *  error dots. Purely presentational — the pane owns all state. */
export function FileTabs(props: FileTabsProps) {
  const { files, active, errorFiles } = props;
  const [editing, setEditing] = useState<string | null>(null);
  const [dropAt, setDropAt] = useState<number | null>(null);
  const dragFrom = useRef<number | null>(null);

  const commitRename = (from: string, to: string) => {
    setEditing(null);
    const next = to.trim();
    if (next && next !== from) props.onRename(from, next);
  };

  return (
    <div className="ftabs" role="tablist" aria-label="Sketch files">
      {files.map((name, i) => (
        <div
          key={name}
          role="tab"
          aria-selected={name === active}
          className={
            "ftab" +
            (name === active ? " ftab--active" : "") +
            (dropAt === i ? " ftab--drop" : "")
          }
          draggable={editing !== name}
          onDragStart={() => (dragFrom.current = i)}
          onDragEnd={() => {
            dragFrom.current = null;
            setDropAt(null);
          }}
          onDragOver={(e) => {
            if (dragFrom.current === null) return;
            e.preventDefault();
            setDropAt(i);
          }}
          onDrop={(e) => {
            e.preventDefault();
            if (dragFrom.current !== null && dragFrom.current !== i)
              props.onReorder(dragFrom.current, i);
            dragFrom.current = null;
            setDropAt(null);
          }}
          onClick={() => props.onSelect(name)}
          onDoubleClick={() => setEditing(name)}
        >
          {name === active && <span className="ftab-dot" />}
          {name !== active && errorFiles.has(name) && <span className="ftab-err" />}
          {editing === name ? (
            <input
              className="ftab-rename"
              defaultValue={name}
              autoFocus
              onFocus={(e) => e.currentTarget.select()}
              onBlur={(e) => commitRename(name, e.currentTarget.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") commitRename(name, e.currentTarget.value);
                if (e.key === "Escape") setEditing(null);
              }}
              onClick={(e) => e.stopPropagation()}
            />
          ) : (
            <span className="ftab-name">{name}</span>
          )}
          {files.length > 1 && (
            <button
              type="button"
              className="ftab-close"
              aria-label={`Delete ${name}`}
              onClick={(e) => {
                e.stopPropagation();
                if (window.confirm(`Delete "${name}"?`)) props.onDelete(name);
              }}
            >
              ×
            </button>
          )}
        </div>
      ))}
      <button type="button" className="ftab-add" aria-label="New file" onClick={props.onAdd}>
        +
      </button>
      <div className="ftab-spacer" />
      <div className="ftab-status">
        <span className="ftab-status-dot" /> vim · Lua 5.4
      </div>
    </div>
  );
}
