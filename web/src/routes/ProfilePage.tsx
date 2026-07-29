import { useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { getProfile, type Profile } from "../api/apiClient";
import { useSession } from "../api/session";
import { Avatar } from "../components/Avatar";
import { ToyCard } from "../components/ToyCard";
import { useDocumentTitle } from "./useDocumentTitle";
import "../components/cards.css";
import "./profile.css";

export function ProfilePage() {
  const { handle } = useParams<{ handle: string }>();
  const { user } = useSession();
  const [profile, setProfile] = useState<Profile | null>(null);
  const [missing, setMissing] = useState(false);
  useDocumentTitle(handle);

  useEffect(() => {
    if (!handle) return;
    let live = true;
    setProfile(null);
    setMissing(false);
    getProfile(handle)
      .then((p) => live && setProfile(p))
      .catch(() => live && setMissing(true));
    return () => { live = false; };
  }, [handle]);

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
