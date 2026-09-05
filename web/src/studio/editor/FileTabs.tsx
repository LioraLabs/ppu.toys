import { useRef, useState } from "react";
import "./editor-tabs.css";

export interface FileTabsProps {
  /** Ordered file names — order IS execution order (PICO-8). */
  files: string[];
  active: string;
  /** Files whose tab shows an error dot (shown on inactive tabs only —
   *  the active tab shows its errors inline in the editor). */
  errorFiles: ReadonlySet<string>;
  /** Machine-generated files (pokes.lua, timeline.lua): rendered with a ⚙ glyph, not
   *  draggable, not rename/delete-able, and pinned as a drop floor — nothing
   *  can be reordered before or onto a generated tab's position. */
  generated: ReadonlySet<string>;
  /** Files that are otherwise normal (draggable, editable) but can't be
   *  renamed or deleted: main.lua, the toy's entry file. Generated files are
   *  locked too, implicitly. */
  locked?: ReadonlySet<string>;
  /** Generated files with panel data — swaps the ⚙ glyph for an accent ⚡. */
  pokedFiles?: ReadonlySet<string>;
  /** Vim keybindings state shown (and toggled) by the status chip. */
  vimMode?: boolean;
  onToggleVim?: () => void;
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
  const { files, active, errorFiles, generated } = props;
  const [editing, setEditing] = useState<string | null>(null);
  const [dropAt, setDropAt] = useState<number | null>(null);
  const dragFrom = useRef<number | null>(null);
  // Generated tabs are pinned at the front (index 0..floor-1): no drop
  // target may land before or on them, so reordering can never displace one.
  const floor = files.filter((n) => generated.has(n)).length;
  const clampDrop = (i: number) => Math.max(i, floor);

  const commitRename = (from: string, to: string) => {
    setEditing(null);
    const next = to.trim();
    if (next && next !== from) props.onRename(from, next);
  };

  return (
    <div className="ftabs" role="tablist" aria-label="Toy files">
      {files.map((name, i) => {
        const isGenerated = generated.has(name);
        const isLocked = isGenerated || !!props.locked?.has(name);
        return (
          <div
            key={name}
            role="tab"
            aria-selected={name === active}
            aria-controls="code-editor"
            tabIndex={name === active ? 0 : -1}
            data-tab-index={i}
            className={
              "ftab" +
              (name === active ? " ftab--active" : "") +
              (dropAt === i ? " ftab--drop" : "")
            }
            draggable={!isGenerated && editing !== name}
            onDragStart={() => (dragFrom.current = i)}
            onDragEnd={() => {
              dragFrom.current = null;
              setDropAt(null);
            }}
            onDragOver={(e) => {
              if (dragFrom.current === null) return;
              e.preventDefault();
              setDropAt(clampDrop(i));
            }}
            onDrop={(e) => {
              e.preventDefault();
              const to = clampDrop(i);
              if (dragFrom.current !== null && dragFrom.current !== to)
                props.onReorder(dragFrom.current, to);
              dragFrom.current = null;
              setDropAt(null);
            }}
            onClick={() => props.onSelect(name)}
            onDoubleClick={() => {
              if (!isLocked) setEditing(name);
            }}
            onKeyDown={(event) => {
              if (event.altKey && (event.key === "ArrowLeft" || event.key === "ArrowRight")) {
                if (isGenerated) return;
                const to = Math.max(
                  floor,
                  Math.min(files.length - 1, i + (event.key === "ArrowLeft" ? -1 : 1)),
                );
                if (to !== i) props.onReorder(i, to);
                return;
              }
              if (event.key === "F2" && !isLocked) {
                event.preventDefault();
                setEditing(name);
                return;
              }
              const next =
                event.key === "ArrowRight"
                  ? Math.min(i + 1, files.length - 1)
                  : event.key === "ArrowLeft"
                    ? Math.max(i - 1, 0)
                    : event.key === "Home"
                      ? 0
                      : event.key === "End"
                        ? files.length - 1
                        : i;
              if (next === i) return;
              event.preventDefault();
              props.onSelect(files[next]);
              document.querySelector<HTMLElement>(`[data-tab-index="${next}"]`)?.focus();
            }}
          >
            {isGenerated &&
              (props.pokedFiles?.has(name) ? (
                <span className="ftab-gen ftab-gen--poked">⚡</span>
              ) : (
                <span className="ftab-gen">⚙</span>
              ))}
            {name === active && <span className="ftab-dot" />}
            {name !== active && errorFiles.has(name) && (
              <>
                <span className="ftab-err" aria-hidden="true" />
                <span className="sr-only">Errors in {name}</span>
              </>
            )}
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
            {files.length - floor > 1 && !isLocked && (
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
        );
      })}
      <button type="button" className="ftab-add" aria-label="New file" onClick={props.onAdd}>
        +
      </button>
      <div className="ftab-spacer" />
      <button
        type="button"
        className={"ftab-vim" + (props.vimMode ? " ftab-vim--on" : "")}
        aria-pressed={!!props.vimMode}
        title={
          props.vimMode
            ? "Vim keybindings on — click to switch to plain"
            : "Vim keybindings off — click to enable"
        }
        onClick={() => props.onToggleVim?.()}
      >
        <span className="ftab-status-dot" /> vim {props.vimMode ? "on" : "off"}
      </button>
      <div className="ftab-status">Lua 5.4</div>
    </div>
  );
}
