import { useEffect, useRef, useState, type FormEvent } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { getWall, type WallCard, type WallSort } from "../api/apiClient";
import { useDocumentTitle } from "./useDocumentTitle";
import { useSession } from "../api/session";
import { ToyCard } from "../components/ToyCard";
import "../components/cards.css";
import "./wall.css";

export function Browse() {
  useDocumentTitle("Browse");
  const { user } = useSession();
  const [params, setParams] = useSearchParams();
  const tag = params.get("tag") ?? "";
  const [failed, setFailed] = useState(false);
  const [retry, setRetry] = useState(0);
  const [sort, setSort] = useState<WallSort>("recent");
  const [query, setQuery] = useState("");
  const [search, setSearch] = useState("");
  const [cards, setCards] = useState<WallCard[]>([]);
  const [nextPage, setNextPage] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const requestRef = useRef({ sort, search, tag });
  requestRef.current = { sort, search, tag };

  useEffect(() => {
    let live = true;
    setLoading(true);
    setFailed(false);
    setCards([]);
    setNextPage(null);
    getWall(sort, 0, search, { tag })
      .then((page) => {
        if (!live) return;
        setCards(page.toys);
        setNextPage(page.nextPage);
        setLoading(false);
      })
      .catch(() => {
        if (live) {
          setFailed(true);
          setLoading(false);
        }
      });
    return () => {
      live = false;
    };
  }, [sort, search, tag, retry]);

  function submit(event: FormEvent) {
    event.preventDefault();
    setSearch(query.trim());
  }

  async function loadMore() {
    if (nextPage === null || loadingMore) return;
    const request = { sort, search, tag };
    setLoadingMore(true);
    try {
      const page = await getWall(sort, nextPage, search, { tag });
      if (
        requestRef.current.sort !== request.sort ||
        requestRef.current.search !== request.search ||
        requestRef.current.tag !== request.tag
      )
        return;
      setCards((current) => [...current, ...page.toys]);
      setNextPage(page.nextPage);
    } catch {
      setFailed(true);
    } finally {
      setLoadingMore(false);
    }
  }

  return (
    <div className="browse-page">
      <header className="browse-heading">
        <div>
          <span className="wall-section-label">Explore the archive</span>
          <h1>Browse Toys</h1>
        </div>
        <form className="browse-search" role="search" onSubmit={submit}>
          <label htmlFor="toy-search">Search by title or author</label>
          <div>
            <input
              id="toy-search"
              type="search"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
            <button type="submit">Search</button>
          </div>
        </form>
      </header>
      <div className="browse-toolbar">
        <label className="browse-tag-filter">
          Tag
          <select
            aria-label="Filter by tag"
            value={tag}
            onChange={(event) => setParams(event.target.value ? { tag: event.target.value } : {})}
          >
            <option value="">All tags</option>
            <option value="playable">Playable</option>
            {tag && tag !== "playable" && <option value={tag}>#{tag}</option>}
          </select>
        </label>
        {cards[0] && (
          <Link
            className="fork-btn"
            to={`/t/${cards[0].id}/play${tag ? `?tag=${encodeURIComponent(tag)}` : ""}`}
          >
            Play feed
          </Link>
        )}
        <span>{search ? `Results for “${search}”` : "All published toys"}</span>
        <div className="sort-tabs" aria-label="Sort toys">
          <button
            className={`sort-tab${sort === "recent" ? " sort-tab--on" : ""}`}
            aria-pressed={sort === "recent"}
            onClick={() => setSort("recent")}
          >
            Newest
          </button>
          <button
            className={`sort-tab${sort === "popular" ? " sort-tab--on" : ""}`}
            aria-pressed={sort === "popular"}
            onClick={() => setSort("popular")}
          >
            Popular
          </button>
        </div>
      </div>
      {failed && (
        <p className="wall-empty" role="alert">
          Toys couldn’t load. <button onClick={() => setRetry((n) => n + 1)}>Retry</button>
        </p>
      )}
      {!failed && !loading && cards.length === 0 && <p className="wall-empty">No matching toys.</p>}
      {loading && (
        <div className="wall-loading" aria-label="Loading toys">
          <span />
          <span />
          <span />
        </div>
      )}
      <div className="wall-grid">
        {cards.map((card) => (
          <ToyCard key={card.id} card={card} signedIn={!!user} />
        ))}
      </div>
      {nextPage !== null && (
        <div className="wall-more">
          <button onClick={() => void loadMore()} disabled={loadingMore}>
            {loadingMore ? "Loading…" : "Load more"}
          </button>
        </div>
      )}
    </div>
  );
}
