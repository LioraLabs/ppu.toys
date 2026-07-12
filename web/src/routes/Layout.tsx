import { useEffect } from "react";
import { Link, Outlet } from "react-router-dom";
import { useSession, sessionStore } from "../api/session";
import { SIGN_IN_URL } from "../api/apiClient";
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
        <Link to="/" className="brand">ppu.toys</Link>
        <nav className="site-nav">
          <Link to="/studio">Studio</Link>
          {!loading && user && (
            <>
              <Link to={`/u/${user.handle}`}>{user.handle}</Link>
              <button className="linklike" onClick={() => void sessionStore.signOut()}>
                Sign out
              </button>
            </>
          )}
          {!loading && !user && (
            <a className="btn-discord" href={SIGN_IN_URL}>Sign in with Discord</a>
          )}
        </nav>
      </header>
      <main className="site-main">
        <Outlet />
      </main>
    </div>
  );
}
