import { useEffect, useState } from "react";
import {
  adminBanUser,
  adminDeleteToy,
  adminUnbanUser,
  getAdminOverview,
  getStarterTemplate,
  setFeaturedToy,
  setFeaturedToys,
  updateStarterTemplate,
  type AdminOverview,
} from "../api/apiClient";
import { useSession } from "../api/session";
import "./admin.css";
import { AdminToyPicker } from "./AdminToyPicker";

export function AdminPage() {
  const { user, loading } = useSession();
  const [data, setData] = useState<AdminOverview | null>(null);
  const [starter, setStarter] = useState("");
  const [status, setStatus] = useState("");
  const formatBytes = (bytes: number) =>
    bytes >= 1024 ** 3
      ? `${(bytes / 1024 ** 3).toFixed(1)} GiB`
      : `${(bytes / 1024 ** 2).toFixed(1)} MiB`;

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
        current
          ? {
              ...current,
              toys: current.toys.filter((toy) => toy.id !== id),
              featuredToys: current.featuredToys.filter((toy) => toy.id !== id),
              featuredToyId: current.featuredToyId === id ? "" : current.featuredToyId,
              featuredToyIds: current.featuredToyIds.filter((toyId) => toyId !== id),
            }
          : current,
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

      {!data && (
        <p role="status">
          {status
            ? "Admin data could not be loaded. Reload the page to retry."
            : "Loading admin data…"}
        </p>
      )}
      {data && (
        <section className="admin-card">
          <h2>Community activity</h2>
          <p>
            As of {new Date(data.activity.asOf * 1000).toLocaleString()}. Rolling periods; toys
            include saved drafts and published toys, excluding deleted toys. Signups exclude system
            accounts.
          </p>
          <div className="admin-table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Metric</th>
                  <th>Last hour</th>
                  <th>Last 24 hours</th>
                  <th>Last 7 days</th>
                  <th>Total</th>
                </tr>
              </thead>
              <tbody>
                {(
                  [
                    ["users", "User signups"],
                    ["toys", "Toys created"],
                    ["published", "Toys first published"],
                    ["creators", "Active creators"],
                  ] as const
                ).map(([key, label]) => (
                  <tr key={key}>
                    <th scope="row">{label}</th>
                    {(["hour", "day", "week", "total"] as const).map((period) => (
                      <td key={period} className="admin-metric">
                        {data.activity[key][period].toLocaleString()}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <p>
            Active creators are unique people who saved a new toy or first published a toy in the
            period. Edits and repeat publishes are not counted. Creator and publishing metrics
            exclude system accounts.
          </p>
          <details className="admin-daily">
            <summary>Daily activity · past 14 days (UTC)</summary>
            <div className="admin-table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Date</th>
                    <th>Signups</th>
                    <th>Toys created</th>
                    <th>First published</th>
                    <th>Active creators</th>
                  </tr>
                </thead>
                <tbody>
                  {data.activity.daily.map((row) => (
                    <tr key={row.day}>
                      <th scope="row">
                        {new Date(row.day * 1000).toISOString().slice(0, 10)}
                        {row.day === Math.floor(data.activity.asOf / 86400) * 86400
                          ? " (partial)"
                          : ""}
                      </th>
                      <td>{row.users}</td>
                      <td>{row.toys}</td>
                      <td>{row.published}</td>
                      <td>{row.creators}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </details>
        </section>
      )}

      {data && (
        <section className="admin-card">
          <h2>Creator publishing funnel</h2>
          <p>
            All-time progress based on toys still stored. Deleted toys and system accounts are
            excluded.
          </p>
          <ol className="admin-funnel">
            {(
              [
                ["creators", "Saved a toy"],
                ["publishers", "Published a toy"],
                ["repeat_publishers", "Published 2+ toys"],
              ] as const
            ).map(([key, label]) => (
              <li key={key}>
                <span>{label}</span>
                <strong>{data.activity.funnel[key].toLocaleString()}</strong>
                <span>
                  {data.activity.funnel.creators === 0
                    ? "No creators yet"
                    : `${Math.round((data.activity.funnel[key] / data.activity.funnel.creators) * 100)}% of creators`}
                </span>
              </li>
            ))}
          </ol>
        </section>
      )}

      {data && (
        <section className={`admin-card ${data.storage.warning ? "admin-storage-warning" : ""}`}>
          <h2>Storage</h2>
          <p>
            {formatBytes(data.storage.usedBytes)} of {formatBytes(data.storage.limitBytes)} used
            {data.storage.warning ? " — action required" : ""}
          </p>
          <progress
            aria-label="Storage used"
            value={data.storage.usedBytes}
            max={data.storage.limitBytes}
          />
        </section>
      )}

      {data && (
        <div className="admin-curation">
          <AdminToyPicker
            title="Toy of the Week"
            limit={1}
            selected={data.featuredToys.filter((toy) => toy.id === data.featuredToyId)}
            onSave={async (toys) => {
              await setFeaturedToy(toys[0]?.id ?? null);
              setData((current) =>
                current
                  ? {
                      ...current,
                      featuredToyId: toys[0]?.id ?? "",
                      featuredToys: [
                        ...current.featuredToys.filter(
                          (item) => !toys.some((toy) => toy.id === item.id),
                        ),
                        ...toys,
                      ],
                    }
                  : current,
              );
              setStatus("Toy of the Week saved.");
            }}
          />
          <AdminToyPicker
            title="Community highlights"
            limit={5}
            selected={data.featuredToyIds.flatMap((id) =>
              data.featuredToys.filter((toy) => toy.id === id),
            )}
            onSave={async (toys) => {
              await setFeaturedToys(toys.map((toy) => toy.id));
              setData((current) =>
                current
                  ? {
                      ...current,
                      featuredToyIds: toys.map((toy) => toy.id),
                      featuredToys: [
                        ...current.featuredToys.filter(
                          (item) => !toys.some((toy) => toy.id === item.id),
                        ),
                        ...toys,
                      ],
                    }
                  : current,
              );
              setStatus("Community highlights saved.");
            }}
          />
        </div>
      )}

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
          Recent users <span>{data?.users.length ?? 0}</span>
        </h2>
        <p>Showing the latest {data?.users.length ?? 0} users (up to 200).</p>
        <div className="admin-table-wrap">
          <table>
            <thead>
              <tr>
                <th>Handle</th>
                <th>Discord ID</th>
                <th>Status</th>
                <th>Storage</th>
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
                  <td>{formatBytes(member.storage_bytes)}</td>
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
          Recent toys <span>{data?.toys.length ?? 0}</span>
        </h2>
        <p>Showing the latest {data?.toys.length ?? 0} toys (up to 200).</p>
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
