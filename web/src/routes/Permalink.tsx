import { useEffect, useMemo, useState } from "react";
import { useParams, useNavigate, Link } from "react-router-dom";
import { getToy, forkToy, type ToyFull } from "../api/apiClient";
import { useSession } from "../api/session";
import { decodeBase64 } from "../api/base64";
import { ReadOnlyPlayer, type PlayerSource } from "../components/ReadOnlyPlayer";
import { HeartButton } from "../components/HeartButton";
import "./permalink.css";

type Load = { status: "loading" } | { status: "error" } | { status: "ok"; toy: ToyFull };

export function Permalink() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const { user } = useSession();
  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [active, setActive] = useState(0);
  const [forking, setForking] = useState(false);
  const [forkFailed, setForkFailed] = useState(false);

  useEffect(() => {
    if (!id) return;
    let live = true;
    setLoad({ status: "loading" });
    getToy(id)
      .then((toy) => live && setLoad({ status: "ok", toy }))
      .catch(() => live && setLoad({ status: "error" }));
    return () => { live = false; };
  }, [id]);

  // Decode M10 source payloads (base64 → bytes) for the player. Builtin
  // reference sources carry no payload and are skipped.
  const decoded: PlayerSource[] = useMemo(() => {
    if (load.status !== "ok") return [];
    return load.toy.sources
      .filter((s) => s.payload)
      .map((s) => ({ name: s.name, payload: decodeBase64(s.payload as string) }));
  }, [load]);

  if (load.status === "loading") return <p className="permalink-msg">Loading…</p>;
  if (load.status === "error") return <p className="permalink-msg">Toy not found.</p>;

  const toy = load.toy;
  const activeFile = toy.files[active] ?? toy.files[0];

  async function fork() {
    if (!id) return;
    setForking(true);
    setForkFailed(false);
    try {
      await forkToy(id);
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
          <HeartButton id={toy.id} heartCount={toy.heartCount} hearted={toy.hearted} signedIn={!!user} />
          <button className="fork-btn" onClick={() => void fork()} disabled={!user || forking}>
            {forking ? "Forking…" : "Fork"}
          </button>
          {forkFailed && <span className="fork-error" role="alert">Fork failed — try again.</span>}
        </div>
        <div className="code-view">
          <div className="code-tabs">
            {toy.files.map((f, i) => (
              <button
                key={f.name}
                className={`code-tab${i === active ? " code-tab--on" : ""}`}
                onClick={() => setActive(i)}
              >{f.name}</button>
            ))}
          </div>
          <pre className="code-body"><code>{activeFile?.source ?? ""}</code></pre>
        </div>
      </div>
    </div>
  );
}
