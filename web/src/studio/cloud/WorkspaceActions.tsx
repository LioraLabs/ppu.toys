import { useEffect, useState } from "react";
import { useOpenSketch, openContextLabel } from "../sketches/openSketch";
import { useSession, sessionStore } from "../../api/session";
import { SIGN_IN_URL, createToy, updateToy } from "../../api/apiClient";
import { serializeWorkspace } from "./serialize";
import { cloudDraft, useCloudDraft } from "./cloudDraft";
import { PublishDialog } from "./PublishDialog";
import "./cloud.css";

/** Save + Publish, the toolbar's cloud seam. Signed-out collapses to a single
 *  sign-in link — Save/Publish/PublishDialog never mount without a session. */
export function WorkspaceActions() {
  const state = useOpenSketch();
  const { user } = useSession();
  const draftId = useCloudDraft(state.session);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [showPublish, setShowPublish] = useState(false);

  // /studio is outside Layout (the only other place sessionStore.refresh()
  // runs), so this is the sole seam that resolves the session here.
  useEffect(() => {
    void sessionStore.refresh();
  }, []);

  /** Ensure-saved: serialize the open workspace and create-or-update the
   *  bound cloud draft, returning its id. Shared by the Save button and the
   *  publish flow (PublishDialog calls this same function via prop). */
  async function save(): Promise<string> {
    const { files, sources } = serializeWorkspace(state);
    const title = openContextLabel(state);
    const existing = cloudDraft.current(state.session);
    if (existing) {
      await updateToy(existing, { title, files, sources });
      return existing;
    }
    const created = await createToy({ title, files, sources });
    cloudDraft.set(created.id, state.session);
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

  if (!user) {
    return (
      <a className="btn-ghost" href={SIGN_IN_URL}>
        Sign in to publish
      </a>
    );
  }

  return (
    <div className="workspace-actions">
      {status && <span className="cloud-status">{status}</span>}
      {draftId && <span className="cloud-draft-dot" title="Saved to a cloud draft" />}
      <button type="button" className="btn-ghost" disabled={busy} onClick={() => void handleSave()}>
        Save
      </button>
      <button type="button" className="btn-solid" disabled={busy} onClick={() => setShowPublish(true)}>
        Publish…
      </button>
      {showPublish && <PublishDialog onClose={() => setShowPublish(false)} save={save} />}
    </div>
  );
}
