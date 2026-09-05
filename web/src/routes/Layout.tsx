import { useEffect } from "react";
import { Link, Outlet } from "react-router-dom";
import { useSession, sessionStore } from "../api/session";
import { SIGN_IN_URL } from "../api/apiClient";
import { Avatar } from "../components/Avatar";
import "./layout.css";

export function Layout() {
  const { user, loading } = useSession();

  // Resolve the current session once when the shell mounts.
  useEffect(() => {
    void sessionStore.refresh();
  }, []);

  return (
    <div className="site">
      <header className="site-header">
        <Link to="/" className="brand">
          <span className="brand-mark" aria-hidden="true" />
          <span>
            ppu<span className="brand-dot">.</span>toys
          </span>
        </Link>
        <nav className="site-nav">
          <Link to="/studio">Studio</Link>
          <Link to="/browse">Browse</Link>
          <Link to="/docs">Docs</Link>
          {!loading && user && (
            <>
              {user.isAdmin && <Link to="/admin">Admin</Link>}
              <Link className="site-nav-user" to={`/u/${user.handle}`}>
                <Avatar handle={user.handle} id={user.id} avatar={user.avatar} size={20} />
                <span>{user.handle}</span>
              </Link>
              <button className="linklike" onClick={() => void sessionStore.signOut()}>
                Sign out
              </button>
            </>
          )}
          {!loading && !user && (
            <a className="btn-discord" href={SIGN_IN_URL}>
              {import.meta.env.DEV ? "Sign in locally" : "Sign in with Discord"}
            </a>
          )}
        </nav>
      </header>
      <main className="site-main">
        <Outlet />
      </main>
      <footer className="site-footer">
        <span>ppu.toys — little graphics toys for a real SNES PPU</span>
        <nav>
          <Link to="/docs">Reference</Link>
          <Link to="/tos">Terms</Link>
          <Link to="/privacy">Privacy</Link>
          <a href="https://github.com/LioraLabs/ppu.toys" target="_blank" rel="noreferrer">
            GitHub
          </a>
        </nav>
      </footer>
    </div>
  );
}
