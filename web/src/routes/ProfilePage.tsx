import { useEffect, useState } from "react";
import { useParams } from "react-router-dom";
import { getProfile, type Profile } from "../api/apiClient";
import { useSession } from "../api/session";
import { ToyCard } from "../components/ToyCard";
import "../components/cards.css";
import "./profile.css";

export function ProfilePage() {
  const { handle } = useParams<{ handle: string }>();
  const { user } = useSession();
  const [profile, setProfile] = useState<Profile | null>(null);
  const [missing, setMissing] = useState(false);

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

  return (
    <div className="profile">
      <header className="profile-head">
        <h1>{profile.user.handle}</h1>
        <span className="profile-count">{profile.toys.length} toys</span>
      </header>
      {profile.toys.length === 0 ? (
        <p className="profile-msg">No published toys yet.</p>
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
