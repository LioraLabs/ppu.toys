import { useEffect, useState } from "react";
import {
  adminBanUser,
  adminDeleteToy,
  adminUnbanUser,
  getAdminOverview,
  getStarterTemplate,
  updateStarterTemplate,
  type AdminOverview,
} from "../api/apiClient";
import { useSession } from "../api/session";
import "./admin.css";

export function AdminPage() {
  const { user, loading } = useSession();
  const [data, setData] = useState<AdminOverview | null>(null);
  const [starter, setStarter] = useState("");
  const [status, setStatus] = useState("");

  useEffect(() => {
    if (!user?.isAdmin) return;
    Promise.all([getAdminOverview(), getStarterTemplate()])
      .then(([overview, template]) => {
        setData(overview);
        setStarter(JSON.stringify(template, null, 2));
      })
      .catch((error) => setStatus(String(error)));
  }, [user]);

  if (loading) return <div className="admin-state">Loading…</div>;
  if (!user?.isAdmin) return <div className="admin-state">Admin access required.</div>;

  async function saveStarter() {
    try {
      await updateStarterTemplate(JSON.parse(starter));
      setStatus("Starter template saved.");
    } catch (error) {
      setStatus(`Could not save starter: ${String(error)}`);
    }
  }

  async function removeToy(id: string, title: string) {
    if (!confirm(`Delete “${title}” (${id}) permanently?`)) return;
    try {
      await adminDeleteToy(id);
      setData((current) =>
        current ? { ...current, toys: current.toys.filter((toy) => toy.id !== id) } : current,
      );
      setStatus("Toy deleted.");
    } catch (error) {
      setStatus(`Could not delete toy: ${String(error)}`);
    }
  }

  async function toggleBan(id: string, banned: boolean) {
    if (!banned && !confirm(`Ban Discord user ${id} and revoke their sessions?`)) return;
    try {
      await (banned ? adminUnbanUser(id) : adminBanUser(id));
      setData((current) =>
        current
          ? {
              ...current,
              users: current.users.map((member) =>
                member.id === id ? { ...member, banned: !banned } : member,
              ),
            }
          : current,
      );
      setStatus(banned ? "User unbanned." : "User banned and sessions revoked.");
    } catch (error) {
      setStatus(`Could not update user: ${String(error)}`);
    }
  }

  return (
    <div className="admin-page">
      <header className="admin-heading">
        <div>
          <p className="admin-kicker">REMOTE OPERATIONS</p>
          <h1>Admin</h1>
        </div>
        {status && (
          <div className="admin-status" role="status">
            {status}
          </div>
        )}
      </header>

      <section className="admin-card admin-starter">
        <div>
          <h2>Starter project</h2>
          <p>JSON served to every fresh Studio project.</p>
        </div>
        <textarea
          aria-label="Starter project JSON"
          value={starter}
          onChange={(event) => setStarter(event.target.value)}
          spellCheck={false}
        />
        <button className="admin-primary" onClick={() => void saveStarter()}>
          Save starter
        </button>
      </section>

      <section className="admin-card">
        <h2>
          Users <span>{data?.users.length ?? 0}</span>
        </h2>
        <div className="admin-table-wrap">
          <table>
            <thead>
              <tr>
                <th>Handle</th>
                <th>Discord ID</th>
                <th>Status</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {data?.users.map((member) => (
                <tr key={member.id}>
                  <td>
                    {member.handle}
                    {member.is_admin && <small> admin</small>}
                  </td>
                  <td>
                    <code>{member.id}</code>
                  </td>
                  <td>{member.banned ? "Banned" : "Active"}</td>
                  <td>
                    <button
                      className={member.banned ? "admin-secondary" : "admin-danger"}
                      disabled={member.is_admin}
                      onClick={() => void toggleBan(member.id, member.banned)}
                    >
                      {member.banned ? "Unban" : "Ban"}
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>

      <section className="admin-card">
        <h2>
          Toys <span>{data?.toys.length ?? 0}</span>
        </h2>
        <div className="admin-table-wrap">
          <table>
            <thead>
              <tr>
                <th>Title</th>
                <th>Author</th>
                <th>State</th>
                <th>ID</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {data?.toys.map((toy) => (
                <tr key={toy.id}>
                  <td>
                    <a href={`/t/${toy.id}`}>{toy.title}</a>
                  </td>
                  <td>{toy.author}</td>
                  <td>{toy.state}</td>
                  <td>
                    <code>{toy.id}</code>
                  </td>
                  <td>
                    <button
                      className="admin-danger"
                      onClick={() => void removeToy(toy.id, toy.title)}
                    >
                      Delete
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  );
}
