import { useEffect, useId, useRef, useState } from "react";
import { getWall, type WallPage, type WallSort } from "../api/apiClient";

type Toy = { id: string; title: string; author: string };

export function AdminToyPicker({
  title,
  selected,
  limit,
  onSave,
}: {
  title: string;
  selected: Toy[];
  limit: number;
  onSave: (toys: Toy[]) => Promise<void>;
}) {
  const id = useId();
  const searchToggle = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<WallSort>("recent");
  const [page, setPage] = useState(0);
  const [results, setResults] = useState<WallPage | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [retry, setRetry] = useState(0);

  useEffect(() => {
    if (!open) return;
    let live = true;
    setResults(null);
    setError("");
    const timer = setTimeout(() => {
      getWall(sort, page, query)
        .then((result) => {
          if (live) setResults(result);
        })
        .catch((error) => {
          if (live) setError(`Could not load toys: ${String(error)}`);
        });
    }, 250);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [open, query, sort, page, retry]);

  async function save(toys: Toy[]) {
    setBusy(true);
    setError("");
    try {
      await onSave(toys);
      if (limit === 1) {
        setOpen(false);
        searchToggle.current?.focus();
      }
    } catch (error) {
      setError(`Could not save selection: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="admin-card">
      <h2>
        {title}{" "}
        <span>
          {selected.length}/{limit}
        </span>
      </h2>
      <p>
        {limit === 1
          ? "The main spotlight on the home page."
          : "Up to five featured toys, shown in this order."}
      </p>
      <ol className="admin-selected">
        {selected.map((toy, index) => (
          <li key={toy.id}>
            <img src={`/blobs/thumb/${toy.id}`} alt="" loading="lazy" width="64" height="48" />
            <div className="admin-toy-label">
              <a href={`/t/${toy.id}`} target="_blank" rel="noreferrer">
                {toy.title}
              </a>
              <span>by {toy.author}</span>
            </div>
            {limit > 1 && (
              <button
                className="admin-secondary"
                disabled={busy || index === 0}
                aria-label={`Move ${toy.title} up`}
                onClick={() => {
                  const toys = [...selected];
                  [toys[index - 1], toys[index]] = [toys[index], toys[index - 1]];
                  void save(toys);
                }}
              >
                ↑
              </button>
            )}
            <button
              className="admin-secondary"
              disabled={busy}
              aria-label={`Remove ${toy.title}`}
              onClick={() => void save(selected.filter((item) => item.id !== toy.id))}
            >
              Remove
            </button>
          </li>
        ))}
      </ol>
      {selected.length === 0 && <p>No toy selected.</p>}
      <button
        className="admin-secondary"
        ref={searchToggle}
        aria-expanded={open}
        aria-controls={`${id}-search`}
        onClick={() => setOpen(!open)}
      >
        {open ? "Close search" : limit === 1 ? "Choose a toy" : "Find featured toys"}
      </button>
      {busy && <p role="status">Saving selection…</p>}
      {error && (
        <p role="alert">
          {error}{" "}
          {open && !results && (
            <button className="admin-secondary" onClick={() => setRetry(retry + 1)}>
              Retry search
            </button>
          )}
        </p>
      )}
      {open && (
        <div id={`${id}-search`} className="admin-picker">
          <div className="admin-search-toolbar">
            <label htmlFor={id}>
              Search by title or author
              <input
                id={id}
                type="search"
                value={query}
                onChange={(event) => {
                  setQuery(event.target.value);
                  setPage(0);
                  setResults(null);
                }}
              />
            </label>
            <label htmlFor={`${id}-sort`}>
              Sort
              <select
                id={`${id}-sort`}
                value={sort}
                onChange={(event) => {
                  setSort(event.target.value as WallSort);
                  setPage(0);
                  setResults(null);
                }}
              >
                <option value="recent">Newest</option>
                <option value="popular">Most loved</option>
              </select>
            </label>
          </div>
          {!results && !error && <p role="status">Searching toys…</p>}
          {results?.toys.length === 0 && (
            <p role="status">No matching toys. Try a different title or author.</p>
          )}
          <ul className="admin-search-results" aria-busy={!results && !error}>
            {results?.toys.map((toy) => {
              const chosen = selected.some((item) => item.id === toy.id);
              return (
                <li key={toy.id}>
                  <img src={toy.thumbUrl} alt="" loading="lazy" width="80" height="60" />
                  <div className="admin-toy-label">
                    <a href={`/t/${toy.id}`} target="_blank" rel="noreferrer">
                      {toy.title}
                    </a>
                    <span>
                      by {toy.author.handle} · {toy.heartCount} hearts
                    </span>
                  </div>
                  <button
                    className="admin-primary"
                    disabled={busy || chosen || (limit > 1 && selected.length >= limit)}
                    aria-label={`${chosen ? "Selected" : "Select"} ${toy.title}`}
                    onClick={() =>
                      void save([
                        ...(limit === 1 ? [] : selected),
                        { id: toy.id, title: toy.title, author: toy.author.handle },
                      ])
                    }
                  >
                    {chosen ? "Selected" : "Select"}
                  </button>
                </li>
              );
            })}
          </ul>
          {limit > 1 && selected.length >= limit && (
            <p>All five spots are filled. Remove a toy to choose another.</p>
          )}
          {results && (
            <div className="admin-pagination">
              <button
                className="admin-secondary"
                disabled={page === 0}
                onClick={() => {
                  setPage(page - 1);
                  setResults(null);
                }}
              >
                Previous
              </button>
              <span>Page {page + 1}</span>
              <button
                className="admin-secondary"
                disabled={results.nextPage === null}
                onClick={() => {
                  setPage(results.nextPage!);
                  setResults(null);
                }}
              >
                Next
              </button>
            </div>
          )}
        </div>
      )}
    </section>
  );
}
