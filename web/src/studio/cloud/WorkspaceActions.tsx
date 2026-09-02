import { useEffect, useRef, useState } from "react";
import { useOpenSketch, openSketchStore } from "../sketches/openSketch";
import { useSession, sessionStore } from "../../api/session";
import { SIGN_IN_URL } from "../../api/apiClient";
import { saveLocalFile, openLocalFile } from "./localFile";
import { PublishDialog } from "./PublishDialog";
import "./cloud.css";

/** Save + Open + Publish, the toolbar's cloud seam. Save/Open write and read
 *  a local `.ppu.json` file and work signed out; only the link chip and
 *  Publish require a session, collapsing to a sign-in link when signed out. */
export function WorkspaceActions() {
  const state = useOpenSketch();
  const { user } = useSession();
  const origin = state.context.sketch.origin;
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [showPublish, setShowPublish] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);

  // /studio is outside Layout (the only other place sessionStore.refresh()
  // runs), so this is the sole seam that resolves the session here.
  useEffect(() => {
    void sessionStore.refresh();
  }, []);

  async function handleSave() {
    if (busy) return;
    setBusy(true);
    setStatus("Saving…");
    try {
      const saved = await saveLocalFile(openSketchStore.state());
      setStatus(saved ? "Saved" : null);
    } catch (e) {
      setStatus(e instanceof Error ? e.message : "Save failed");
    } finally {
      setBusy(false);
    }
  }

  async function handleOpen(file: File) {
    if (busy) return;
    setBusy(true);
    setStatus("Opening…");
    try {
      await openLocalFile(file);
      setStatus("Opened");
    } catch (e) {
      setStatus(e instanceof Error ? e.message : "Open failed");
    } finally {
      setBusy(false);
    }
  }

  // Ctrl/Cmd+S = Save, signed in or out. Capture phase beats the browser's
  // save-page dialog and CodeMirror; registered whenever this is mounted.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.key === "s" || e.key === "S") && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        e.stopPropagation();
        void handleSave();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
    // handleSave reads openSketchStore.state() live, so a stale closure can't
    // write stale data; only `busy` needs to be current here.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [busy]);

  const fileInput = (
    <input
      ref={fileInputRef}
      type="file"
      accept=".json"
      hidden
      onChange={(e) => {
        const file = e.target.files?.[0];
        if (file) void handleOpen(file);
        e.target.value = "";
      }}
    />
  );

  const chip = origin ? (
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
  );

  return (
    <div className="workspace-actions">
      {status && (
        <span className="cloud-status" role="status">
          {status}
        </span>
      )}
      {chip}
      <button type="button" className="btn-ghost" disabled={busy} onClick={() => void handleSave()}>
        Save
      </button>
      <button
        type="button"
        className="btn-ghost"
        disabled={busy}
        onClick={() => fileInputRef.current?.click()}
      >
        Open…
      </button>
      {fileInput}
      {user ? (
        <>
          <button
            type="button"
            className="btn-solid"
            disabled={busy}
            onClick={() => setShowPublish(true)}
          >
            Publish…
          </button>
          {showPublish && <PublishDialog onClose={() => setShowPublish(false)} />}
        </>
      ) : (
        <a className="btn-ghost" href={SIGN_IN_URL}>
          Sign in to publish
        </a>
      )}
    </div>
  );
}
