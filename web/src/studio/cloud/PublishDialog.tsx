import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useOpenSketch, openContextLabel, openSketchStore } from "../sketches/openSketch";
import { useSession } from "../../api/session";
import {
  getToy,
  createToy,
  updateToy,
  publishToy,
  ApiError,
  type SaveToyBody,
} from "../../api/apiClient";
import { serializeWorkspace } from "./serialize";
import { recordClip } from "./clip";
import { transport } from "../transport/transport";
import { useModalFocus } from "../useModalFocus";
import "./cloud.css";

type Phase = "idle" | "saving" | "recording" | "uploading";

const PHASE_LABEL: Record<Exclude<Phase, "idle">, string> = {
  saving: "Saving…",
  recording: "Recording clip…",
  uploading: "Uploading…",
};

export interface PublishDialogProps {
  onClose: () => void;
}

/** Maps a create/update failure to dialog copy. A 409 means different things
 *  on each verb (stale revision vs. quota), so the caller says which. */
function errorMessage(e: unknown, verb: "create" | "update"): string {
  if (e instanceof ApiError) {
    if (e.status === 429) return "Updates are limited to one per minute — try again shortly.";
    if (e.status === 409) {
      return verb === "update"
        ? "This toy changed elsewhere — reopen it to update, or publish as new."
        : "Toy quota reached.";
    }
  }
  return e instanceof Error ? e.message : "Publish failed";
}

/** Publish, branched on the open sketch's origin and the signed-in user: no
 *  origin mints a toy; an origin the user owns updates it (with a "publish as
 *  new" escape hatch); anyone else's origin always forks. Owns the whole
 *  create/update-then-publish sequence — the clip is recorded FIRST (so a
 *  doomed recording never touches the server), then the workspace is saved,
 *  then uploaded. Stays open on failure so the user can retry. */
export function PublishDialog({ onClose }: PublishDialogProps) {
  const state = useOpenSketch();
  const { user } = useSession();
  const origin = state.context.sketch.origin;
  const owned = !!(user && origin && origin.authorId === user.id);

  const [title, setTitle] = useState(() => openContextLabel(state));
  const [description, setDescription] = useState("");
  const [clipStart, setClipStart] = useState(() => Math.floor(transport.getSnapshot().t));
  const [phase, setPhase] = useState<Phase>("idle");
  const [error, setError] = useState<string | null>(null);
  // Flips true when the owned origin's toy is gone server-side (404 on the
  // prefill fetch, or on the update itself) — from then on this behaves like
  // the no-origin branch: create only, no forkedFrom.
  const [gone, setGone] = useState(false);
  const [existing, setExisting] = useState<string | null>(null);
  const navigate = useNavigate();
  const busy = phase !== "idle";

  const dialogRef = useModalFocus(onClose, !busy);

  // Owned origin: prefill title/description from the live toy and show its
  // current thumb. Other fetch errors just leave the form at its defaults.
  useEffect(() => {
    if (!owned || !origin) return;
    let live = true;
    getToy(origin.id)
      .then((toy) => {
        if (!live) return;
        setTitle(toy.title);
        setDescription(toy.description);
        setExisting(toy.title);
      })
      .catch((e) => {
        if (live && e instanceof ApiError && e.status === 404) setGone(true);
      });
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [owned, origin?.id]);

  const mode: "new" | "owned" | "forked" = !origin || gone ? "new" : owned ? "owned" : "forked";

  // Records the clip, serializes the workspace, saves it (create or update —
  // `save` owns that difference and returns the id to publish plus, for a
  // create, an `afterPublish` hook that binds the origin once the upload
  // actually lands). A 404 from `save` always flips to the gone state: on
  // update it means the toy vanished server-side; on create it means a
  // `forkedFrom` origin that no longer exists, and since `afterPublish` never
  // ran, origin is never bound to it.
  async function run(
    verb: "create" | "update",
    save: (body: SaveToyBody) => Promise<{ id: string; afterPublish?: () => void }>,
  ) {
    setError(null);
    try {
      setPhase("recording");
      const { clip, thumb } = await recordClip({ startTime: clipStart });
      const { files, sources } = serializeWorkspace(state);
      setPhase("saving");
      const { id, afterPublish } = await save({ title, description, files, sources });
      setPhase("uploading");
      await publishToy(id, { title, description }, clip, thumb);
      afterPublish?.();
      navigate(`/t/${id}`);
    } catch (e) {
      if (e instanceof ApiError && e.status === 404) {
        setGone(true);
        setPhase("idle");
        return;
      }
      setError(errorMessage(e, verb));
      setPhase("idle");
    }
  }

  async function publishNew(fork: boolean) {
    if (busy || !user) return;
    await run("create", async (body) => {
      const created = await createToy({
        ...body,
        ...(fork && origin ? { forkedFrom: origin.id } : {}),
      });
      return {
        id: created.id,
        // A failed upload leaves a sweepable draft — only bind origin to it
        // once the publish actually lands.
        afterPublish: () =>
          openSketchStore.setOrigin({
            id: created.id,
            revision: created.revision,
            authorId: user.id,
          }),
      };
    });
  }

  async function publishUpdate() {
    if (busy || !origin) return;
    await run("update", async (body) => {
      const updated = await updateToy(origin.id, origin.revision, body);
      // The toy is already published, not a sweepable draft: rebind right
      // away, before the upload, so a retry never 409s on a stale revision.
      openSketchStore.setOrigin({ ...origin, revision: updated.revision });
      return { id: origin.id };
    });
  }

  function close() {
    if (busy) return;
    onClose();
  }

  if (!user) return null; // WorkspaceActions never mounts this signed-out

  return (
    <div className="cloud-scrim" onClick={close}>
      <div
        className="cloud-dialog"
        role="dialog"
        aria-modal="true"
        aria-label="Publish"
        ref={dialogRef}
        onClick={(e) => e.stopPropagation()}
      >
        <header className="cloud-head">
          <span className="cloud-title">Publish</span>
          <button
            type="button"
            className="btn-ghost"
            onClick={close}
            disabled={busy}
            aria-label="Close"
          >
            ×
          </button>
        </header>

        <div className="cloud-body">
          <p className="cloud-explainer">
            Publishing records a short loop clip of your toy and puts it on the public wall,
            playable by anyone at its own link.
          </p>

          {gone && origin && (
            <div className="cloud-error" role="alert">
              {`t/${origin.id} no longer exists`}
            </div>
          )}
          {mode === "owned" && origin && (
            <div className="cloud-preview">
              <img src={`/blobs/thumb/${origin.id}`} alt="" />
              {existing && <span>{existing}</span>}
            </div>
          )}

          <label className="cloud-field">
            Title
            <input
              type="text"
              value={title}
              disabled={busy}
              onChange={(e) => setTitle(e.target.value)}
            />
          </label>
          <label className="cloud-field">
            Description
            <textarea
              value={description}
              disabled={busy}
              rows={3}
              onChange={(e) => setDescription(e.target.value)}
            />
          </label>
          <label className="cloud-field">
            Clip time (5 seconds)
            <input
              type="number"
              min={0}
              step={0.1}
              value={clipStart}
              disabled={busy}
              onChange={(e) => setClipStart(Math.max(0, e.target.valueAsNumber || 0))}
            />
            <span>
              t={clipStart.toFixed(1)}s–{(clipStart + 5).toFixed(1)}s
            </span>
          </label>

          {error && (
            <div className="cloud-error" role="alert">
              {error}
            </div>
          )}
          {!error && busy && (
            <div className="cloud-status-line" role="status">
              {PHASE_LABEL[phase as Exclude<Phase, "idle">]}
            </div>
          )}

          <div className="cloud-actions">
            <button type="button" className="btn-ghost" onClick={close} disabled={busy}>
              Cancel
            </button>
            {mode === "owned" && origin && (
              <>
                <button
                  type="button"
                  className="btn-ghost"
                  disabled={busy || !title.trim()}
                  onClick={() => void publishNew(false)}
                >
                  Publish as new toy
                </button>
                <button
                  type="button"
                  className="btn-solid"
                  disabled={busy || !title.trim()}
                  onClick={() => void publishUpdate()}
                >
                  {`Update t/${origin.id}`}
                </button>
              </>
            )}
            {mode === "forked" && origin && (
              <button
                type="button"
                className="btn-solid"
                disabled={busy || !title.trim()}
                onClick={() => void publishNew(true)}
              >
                {`Publish new toy, forked from t/${origin.id}`}
              </button>
            )}
            {mode === "new" && (
              <button
                type="button"
                className="btn-solid"
                disabled={busy || !title.trim()}
                onClick={() => void publishNew(false)}
              >
                Publish new toy
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
