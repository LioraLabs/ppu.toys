import { useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import {
  createToken,
  deleteToken,
  getProfile,
  getTokens,
  getToy,
  type ApiToken,
  type Profile,
} from "../api/apiClient";
import { useSession } from "../api/session";
import { openCloudToy } from "../studio/cloud/openCloudToy";
import { Avatar } from "../components/Avatar";
import { ToyCard } from "../components/ToyCard";
import { useDocumentTitle } from "./useDocumentTitle";
import "../components/cards.css";
import "./profile.css";

export function ProfilePage() {
  const { handle } = useParams<{ handle: string }>();
  const { user } = useSession();
  const navigate = useNavigate();
  const [profile, setProfile] = useState<Profile | null>(null);
  const [missing, setMissing] = useState(false);
  const [opening, setOpening] = useState<string | null>(null);
  const [tokens, setTokens] = useState<ApiToken[]>([]);
  const [newToken, setNewToken] = useState<string | null>(null);
  useDocumentTitle(handle);

  async function editDraft(id: string) {
    if (opening) return;
    setOpening(id);
    try {
      await openCloudToy(await getToy(id));
      navigate("/studio");
    } catch (e) {
      console.error("open draft failed", e);
      setOpening(null);
    }
  }

  useEffect(() => {
    if (!handle) return;
    let live = true;
    setProfile(null);
    setMissing(false);
    getProfile(handle)
      .then((p) => live && setProfile(p))
      .catch(() => live && setMissing(true));
    return () => {
      live = false;
    };
  }, [handle]);

  useEffect(() => {
    if (user?.handle !== handle) return;
    void getTokens().then(setTokens);
  }, [handle, user?.handle]);

  if (missing) return <p className="profile-msg">No such user.</p>;
  if (!profile) return <p className="profile-msg">Loading…</p>;

  const own = user?.handle === profile.user.handle;
  const toyCount = profile.toys.length;
  const heartTotal = profile.toys.reduce((n, t) => n + t.heartCount, 0);

  return (
    <div className="profile">
      <header className="profile-head">
        <Avatar
          handle={profile.user.handle}
          id={profile.user.id}
          avatar={profile.user.avatar}
          size={56}
        />
        <div className="profile-id">
          <h1>{profile.user.handle}</h1>
          <div className="profile-stats">
            <span>{toyCount === 1 ? "1 toy" : `${toyCount} toys`}</span>
            <span aria-hidden="true">·</span>
            <span>♡ {heartTotal}</span>
          </div>
        </div>
      </header>
      {own && (
        <section className="profile-tokens">
          <h2>Local editing</h2>
          <p>Create a personal token, then run the command once on your machine.</p>
          <button
            type="button"
            className="profile-cta"
            onClick={() =>
              void createToken().then((created) => {
                setTokens((current) => [created, ...current]);
                setNewToken(created.token);
              })
            }
          >
            Create CLI token
          </button>
          {newToken && <pre className="profile-token-secret">ppu login {newToken}</pre>}
          {tokens.length > 0 && (
            <ul className="profile-token-list">
              {tokens.map((token) => (
                <li key={token.id}>
                  <span>{token.name}</span>
                  <button
                    type="button"
                    onClick={() =>
                      void deleteToken(token.id).then(() =>
                        setTokens((current) => current.filter((t) => t.id !== token.id)),
                      )
                    }
                  >
                    Revoke
                  </button>
                </li>
              ))}
            </ul>
          )}
        </section>
      )}
      {own && profile.drafts && profile.drafts.length > 0 && (
        <section className="profile-drafts">
          <h2>
            Drafts <span className="profile-drafts-note">only you see these</span>
          </h2>
          <ul>
            {profile.drafts.map((d) => (
              <li key={d.id}>
                <span className="profile-draft-title">{d.title || "untitled toy"}</span>
                <button
                  type="button"
                  className="profile-draft-edit"
                  disabled={opening !== null}
                  onClick={() => void editDraft(d.id)}
                >
                  {opening === d.id ? "Opening…" : "Edit"}
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
      {toyCount === 0 ? (
        own ? (
          <div className="profile-empty">
            <p>No toys yet.</p>
            <Link className="profile-cta" to="/studio">
              Open the Studio and publish your first one
            </Link>
          </div>
        ) : (
          <p className="profile-msg">No published toys yet.</p>
        )
      ) : (
        <div className="wall-grid">
          {profile.toys.map((c) => (
            <ToyCard key={c.id} card={c} signedIn={!!user} />
          ))}
        </div>
      )}
    </div>
  );
}
