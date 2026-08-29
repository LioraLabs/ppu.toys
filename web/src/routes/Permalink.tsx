import { useEffect, useMemo, useState } from "react";
import { useParams, useNavigate, Link } from "react-router-dom";
import { getToy, forkToy, goToSignIn, type ToyFull } from "../api/apiClient";
import { useSession } from "../api/session";
import { openCloudToy } from "../studio/cloud/openCloudToy";
import { decodeBase64 } from "../api/base64";
import { ReadOnlyPlayer, type PlayerSource } from "../components/ReadOnlyPlayer";
import { HeartButton } from "../components/HeartButton";
import { useDocumentTitle } from "./useDocumentTitle";
import "./permalink.css";

type Load = { status: "loading" } | { status: "error" } | { status: "ok"; toy: ToyFull };

export function Permalink() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const { user } = useSession();
  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [retry, setRetry] = useState(0);
  const [active, setActive] = useState(0);
  const [forking, setForking] = useState(false);
  const [forkFailed, setForkFailed] = useState(false);
  useDocumentTitle(
    load.status === "ok" ? `${load.toy.title} · ${load.toy.author.handle}` : undefined,
  );

  useEffect(() => {
    if (!id) return;
    let live = true;
    setLoad({ status: "loading" });
    getToy(id)
      .then((toy) => live && setLoad({ status: "ok", toy }))
      .catch(() => live && setLoad({ status: "error" }));
    return () => {
      live = false;
    };
  }, [id, retry]);

  // Decode M10 source payloads (base64 → bytes) for the player. Builtin
  // reference sources carry no payload and are skipped.
  const decoded: PlayerSource[] = useMemo(() => {
    if (load.status !== "ok") return [];
    return load.toy.sources
      .filter((s) => s.payload)
      .map((s) => ({ name: s.name, payload: decodeBase64(s.payload as string) }));
  }, [load]);

  if (load.status === "loading")
    return (
      <p className="permalink-msg" role="status">
        Loading toy…
      </p>
    );
  if (load.status === "error")
    return (
      <div className="permalink-msg" role="alert">
        <p>We couldn’t load this toy.</p>
        <button type="button" className="fork-btn" onClick={() => setRetry((n) => n + 1)}>
          Try again
        </button>
      </div>
    );

  const toy = load.toy;
  const activeFile = toy.files[active] ?? toy.files[0];

  /** Owner path: reopen this toy in the Studio bound to the SAME cloud id, so
   *  Save/Publish update it in place instead of minting a fork. */
  async function edit() {
    if (load.status !== "ok") return;
    setForking(true);
    setForkFailed(false);
    try {
      await openCloudToy(load.toy);
      navigate("/studio");
    } catch {
      setForkFailed(true);
    } finally {
      setForking(false);
    }
  }

  async function fork() {
    if (!id) return;
    if (!user) {
      // Live sign-in ask instead of a dead disabled button — forking is the
      // loop we want visitors to enter.
      goToSignIn();
      return;
    }
    setForking(true);
    setForkFailed(false);
    try {
      const { id: forkId } = await forkToy(id);
      const toy = await getToy(forkId);
      await openCloudToy(toy);
      navigate("/studio");
    } catch {
      // Surface the failure instead of leaving the click silently dead — the
      // user stays on the page and can retry.
      setForkFailed(true);
    } finally {
      setForking(false);
    }
  }

  return (
    <div className="permalink">
      <div className="permalink-stage">
        <ReadOnlyPlayer files={toy.files} sources={decoded} />
      </div>
      <div className="permalink-side">
        <header className="permalink-head">
          <h1>{toy.title}</h1>
          <Link to={`/u/${toy.author.handle}`} className="permalink-author">
            by {toy.author.handle}
          </Link>
          {toy.description && <p className="permalink-desc">{toy.description}</p>}
        </header>
        <div className="permalink-actions">
          <HeartButton
            id={toy.id}
            heartCount={toy.heartCount}
            hearted={toy.hearted}
            signedIn={!!user}
          />
          {user && user.id === toy.author.id ? (
            <button
              className="fork-btn"
              onClick={() => void edit()}
              disabled={forking}
              title="Open your toy in the Studio"
            >
              {forking ? "Opening…" : "Edit"}
            </button>
          ) : (
            <button
              className="fork-btn"
              onClick={() => void fork()}
              disabled={forking}
              title={user ? "Fork into your Studio" : "Sign in with Discord to fork this toy"}
            >
              {forking ? "Forking…" : "Fork"}
            </button>
          )}
          {forkFailed && (
            <span className="fork-error" role="alert">
              Fork failed — try again.
            </span>
          )}
        </div>
        <div className="code-view">
          <div className="code-view-head">
            <h2>Source</h2>
            <span>
              {toy.files.length} {toy.files.length === 1 ? "file" : "files"}
            </span>
          </div>
          <div className="code-tabs" role="tablist" aria-label="Source files">
            {toy.files.map((f, i) => (
              <button
                key={f.name}
                id={`source-tab-${i}`}
                type="button"
                role="tab"
                aria-selected={i === active}
                aria-controls="source-code"
                tabIndex={i === active ? 0 : -1}
                className={`code-tab${i === active ? " code-tab--on" : ""}`}
                onClick={() => setActive(i)}
                onKeyDown={(event) => {
                  const last = toy.files.length - 1;
                  const next =
                    event.key === "ArrowRight"
                      ? Math.min(i + 1, last)
                      : event.key === "ArrowLeft"
                        ? Math.max(i - 1, 0)
                        : event.key === "Home"
                          ? 0
                          : event.key === "End"
                            ? last
                            : i;
                  if (next === i) return;
                  event.preventDefault();
                  setActive(next);
                  document.getElementById(`source-tab-${next}`)?.focus();
                }}
              >
                {f.name}
              </button>
            ))}
          </div>
          <pre
            id="source-code"
            className="code-body"
            role="tabpanel"
            aria-labelledby={`source-tab-${active}`}
          >
            <code>{activeFile?.source ?? ""}</code>
          </pre>
        </div>
      </div>
    </div>
  );
}
