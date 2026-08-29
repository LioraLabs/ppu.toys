import { useEffect, useRef, useState, type FormEvent } from "react";
import { getWall, type WallCard, type WallSort } from "../api/apiClient";
import { useDocumentTitle } from "./useDocumentTitle";
import { useSession } from "../api/session";
import { ToyCard } from "../components/ToyCard";
import "../components/cards.css";
import "./wall.css";

export function Browse() {
  useDocumentTitle("Browse");
  const { user } = useSession();
  const [sort, setSort] = useState<WallSort>("recent");
  const [query, setQuery] = useState("");
  const [search, setSearch] = useState("");
  const [cards, setCards] = useState<WallCard[]>([]);
  const [nextPage, setNextPage] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const requestRef = useRef({ sort, search });
  requestRef.current = { sort, search };

  useEffect(() => {
    let live = true;
    setLoading(true);
    getWall(sort, 0, search).then((page) => {
      if (!live) return;
      setCards(page.toys);
      setNextPage(page.nextPage);
      setLoading(false);
    });
    return () => {
      live = false;
    };
  }, [sort, search]);

  function submit(event: FormEvent) {
    event.preventDefault();
    setSearch(query.trim());
  }

  async function loadMore() {
    if (nextPage === null || loadingMore) return;
    const request = { sort, search };
    setLoadingMore(true);
    try {
      const page = await getWall(sort, nextPage, search);
      if (requestRef.current.sort !== request.sort || requestRef.current.search !== request.search)
        return;
      setCards((current) => [...current, ...page.toys]);
      setNextPage(page.nextPage);
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
      {!loading && cards.length === 0 && <p className="wall-empty">No matching toys.</p>}
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
