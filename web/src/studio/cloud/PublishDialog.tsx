import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useOpenSketch, openContextLabel } from "../sketches/openSketch";
import { publishToy } from "../../api/apiClient";
import { recordClip } from "./clip";
import "./cloud.css";

type Phase = "idle" | "saving" | "recording" | "uploading";

const PHASE_LABEL: Record<Phase, string> = {
  idle: "Publish",
  saving: "Saving…",
  recording: "Recording clip…",
  uploading: "Uploading…",
};

export interface PublishDialogProps {
  onClose: () => void;
  /** Ensure-saved: the exact same create-or-update logic the Save button
   *  uses, passed down so both paths agree on what "saved" means. Resolves
   *  to the toy id to publish. */
  save: () => Promise<string>;
}

/** Title + description, then: save → record the loop clip → upload. Stays
 *  open on failure (413 from the caps, or any other apiClient error) so the
 *  user can retry without re-entering the form. */
export function PublishDialog({ onClose, save }: PublishDialogProps) {
  const state = useOpenSketch();
  const [title, setTitle] = useState(() => openContextLabel(state));
  const [description, setDescription] = useState("");
  const [phase, setPhase] = useState<Phase>("idle");
  const [error, setError] = useState<string | null>(null);
  const navigate = useNavigate();
  const busy = phase !== "idle";

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape" && phase === "idle") onClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [phase, onClose]);

  async function publish() {
    if (busy) return;
    setError(null);
    try {
      setPhase("saving");
      const id = await save();
      setPhase("recording");
      const { clip, thumb } = await recordClip();
      setPhase("uploading");
      await publishToy(id, { title, description }, clip, thumb);
      navigate(`/t/${id}`);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Publish failed");
      setPhase("idle");
    }
  }

  function close() {
    if (busy) return;
    onClose();
  }

  return (
    <div className="cloud-scrim" onClick={close}>
      <div className="cloud-dialog" role="dialog" aria-label="Publish" onClick={(e) => e.stopPropagation()}>
        <header className="cloud-head">
          <span className="cloud-title">Publish</span>
          <button type="button" className="btn-ghost" onClick={close} disabled={busy} aria-label="Close">
            ×
          </button>
        </header>

        <div className="cloud-body">
          <label className="cloud-field">
            title
            <input type="text" value={title} disabled={busy} onChange={(e) => setTitle(e.target.value)} />
          </label>
          <label className="cloud-field">
            description
            <textarea value={description} disabled={busy} rows={3} onChange={(e) => setDescription(e.target.value)} />
          </label>

          {error && <div className="cloud-error">{error}</div>}
          {!error && busy && <div className="cloud-status-line">{PHASE_LABEL[phase]}</div>}

          <div className="cloud-actions">
            <button type="button" className="btn-ghost" onClick={close} disabled={busy}>
              Cancel
            </button>
            <button type="button" className="btn-solid" disabled={busy || !title.trim()} onClick={() => void publish()}>
              {PHASE_LABEL[phase]}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
