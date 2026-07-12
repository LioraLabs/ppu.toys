import { useEffect, useRef, useState } from "react";
import { getWall, type WallCard, type WallSort } from "../api/apiClient";
import { useSession } from "../api/session";
import { ToyCard } from "../components/ToyCard";
import "../components/cards.css";
import "./wall.css";

export function Wall() {
  const { user } = useSession();
  const [sort, setSort] = useState<WallSort>("recent");
  const [cards, setCards] = useState<WallCard[]>([]);
  const [nextPage, setNextPage] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  // Always holds the latest sort, so an in-flight loadMore can detect a sort
  // change that happened while it was awaiting and discard its stale page.
  const sortRef = useRef(sort);
  sortRef.current = sort;

  // Reload from page 0 whenever the sort changes.
  useEffect(() => {
    let live = true;
    setLoading(true);
    getWall(sort, 0).then((p) => {
      if (!live) return;
      setCards(p.toys);
      setNextPage(p.nextPage);
      setLoading(false);
    });
    return () => { live = false; };
  }, [sort]);

  async function loadMore() {
    if (nextPage === null || loadingMore) return; // guard concurrent/duplicate loads
    const sortAtRequest = sort;
    setLoadingMore(true);
    try {
      const p = await getWall(sortAtRequest, nextPage);
      // A sort change since this fetch started already reset the list — the
      // page-0 effect owns the new sort, so drop this stale page rather than
      // append recent-sorted toys onto a popular list.
      if (sortRef.current !== sortAtRequest) return;
      setCards((prev) => [...prev, ...p.toys]);
      setNextPage(p.nextPage);
    } finally {
      setLoadingMore(false);
    }
  }

  return (
    <div className="wall">
      <div className="wall-toolbar">
        <button
          className={`sort-tab${sort === "recent" ? " sort-tab--on" : ""}`}
          onClick={() => setSort("recent")}
        >Recent</button>
        <button
          className={`sort-tab${sort === "popular" ? " sort-tab--on" : ""}`}
          onClick={() => setSort("popular")}
        >Popular</button>
      </div>
      {!loading && cards.length === 0 && (
        <p className="wall-empty">No toys yet — be the first to publish one.</p>
      )}
      <div className="wall-grid">
        {cards.map((c) => (
          <ToyCard key={c.id} card={c} signedIn={!!user} />
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
