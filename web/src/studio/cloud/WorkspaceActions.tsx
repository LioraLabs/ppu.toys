import { useEffect, useState } from "react";
import { useOpenSketch, openContextLabel, openSketchStore } from "../sketches/openSketch";
import { useSession, sessionStore } from "../../api/session";
import { SIGN_IN_URL, createToy, updateToy } from "../../api/apiClient";
import { serializeWorkspace } from "./serialize";
import { PublishDialog } from "./PublishDialog";
import "./cloud.css";

/** Save + Publish, the toolbar's cloud seam. Signed-out collapses to a single
 *  sign-in link — Save/Publish/PublishDialog never mount without a session. */
export function WorkspaceActions() {
  const state = useOpenSketch();
  const { user } = useSession();
  const origin = state.context.sketch.origin;
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [showPublish, setShowPublish] = useState(false);

  // /studio is outside Layout (the only other place sessionStore.refresh()
  // runs), so this is the sole seam that resolves the session here.
  useEffect(() => {
    void sessionStore.refresh();
  }, []);

  /** Ensure-saved: serialize the open workspace and create-or-update the
   *  toy the sketch is linked to (origin), returning its id. Updates only
   *  when the origin is owned by the signed-in user; otherwise mints a new
   *  toy and re-links to it. Backs the Save button only — PublishDialog owns
   *  its own create/update-then-publish sequence (PPU-122). */
  async function save(meta?: { title?: string; description?: string }): Promise<string> {
    if (!user) throw new Error("not signed in");
    const { files, sources } = serializeWorkspace(state);
    const title = meta?.title ?? openContextLabel(state);
    const description = meta?.description ?? "";
    if (origin && origin.authorId === user.id) {
      const updated = await updateToy(origin.id, origin.revision, {
        title,
        description,
        files,
        sources,
      });
      openSketchStore.setOrigin({ ...origin, revision: updated.revision });
      return origin.id;
    }
    const created = await createToy({ title, description, files, sources });
    openSketchStore.setOrigin({ id: created.id, revision: created.revision, authorId: user.id });
    return created.id;
  }

  async function handleSave() {
    if (busy) return;
    setBusy(true);
    setStatus("Saving…");
    try {
      await save();
      setStatus("Saved");
    } catch (e) {
      setStatus(e instanceof Error ? e.message : "Save failed");
    } finally {
      setBusy(false);
    }
  }

  // Ctrl/Cmd+S = Save (signed in). Capture phase beats the browser's
  // save-page dialog and CodeMirror; registered only while this is mounted.
  useEffect(() => {
    if (!user) return;
    const onKey = (e: KeyboardEvent) => {
      if ((e.key === "s" || e.key === "S") && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        e.stopPropagation();
        void handleSave();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
    // handleSave identity changes per render; the latest closure is fine
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [user, busy]);

  if (!user) {
    return (
      <a className="btn-ghost" href={SIGN_IN_URL}>
        Sign in to publish
      </a>
    );
  }

  return (
    <div className="workspace-actions">
      {status && (
        <span className="cloud-status" role="status">
          {status}
        </span>
      )}
      {origin ? (
        <span className="cloud-link-chip cloud-link-chip--linked">
          {`linked to t/${origin.id}`}
          <button
            type="button"
            className="cloud-link-unlink"
            aria-label="Unlink"
            onClick={() => openSketchStore.clearOrigin()}
          >
            ×
          </button>
        </span>
      ) : (
        <span className="cloud-link-chip">unlinked</span>
      )}
      <button
        type="button"
        className="btn-ghost"
        disabled={busy || showPublish}
        onClick={() => void handleSave()}
      >
        Save
      </button>
      <button
        type="button"
        className="btn-solid"
        disabled={busy}
        onClick={() => setShowPublish(true)}
      >
        Publish…
      </button>
      {showPublish && <PublishDialog onClose={() => setShowPublish(false)} />}
    </div>
  );
}
