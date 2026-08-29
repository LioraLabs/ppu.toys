import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { getWall, type WallCard, type WallSort } from "../api/apiClient";
import { useDocumentTitle } from "./useDocumentTitle";
import { useSession } from "../api/session";
import { ToyCard } from "../components/ToyCard";
import "../components/cards.css";
import "./wall.css";

// Lazy: the 3D hero owns the only three.js import — its chunk loads on / and
// nowhere else. While it loads (or whenever it can't run) the CSS art stands in.
const HeroTV = lazy(() => import("./hero/HeroTV"));

/** The original CSS synthwave hero art — now the hero's fallback for chunk
 *  loading, reduced motion, missing WebGL, and an empty wall. */
function HeroArt() {
  return (
    <div className="wall-hero-art" aria-hidden="true">
      <div className="hero-screen">
        <span className="hero-sun" />
        <span className="hero-mountain hero-mountain--one" />
        <span className="hero-mountain hero-mountain--two" />
        <span className="hero-grid" />
      </div>
      <span className="hero-status">
        <i /> PPU ONLINE
      </span>
    </div>
  );
}

export function Wall() {
  useDocumentTitle();
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
    return () => {
      live = false;
    };
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
      <div className="wall-hero">
        <div className="wall-hero-copy">
          <span className="wall-kicker">A tiny playground for a legendary chip</span>
          <h1>
            Make pictures like it&rsquo;s <span>1991.</span>
          </h1>
          <p>
            Build live graphics toys with Lua and an authentic SNES PPU. Remix an experiment or
            start with a blank screen.
          </p>
          <div className="wall-hero-actions">
            <Link className="wall-primary" to="/studio">
              Open the Studio <span aria-hidden="true">→</span>
            </Link>
            <a className="wall-secondary" href="#toy-wall">
              Explore toys
            </a>
          </div>
        </div>
        <div className="wall-hero-scene">
          <Suspense fallback={<HeroArt />}>
            <HeroTV fallback={<HeroArt />} />
          </Suspense>
        </div>
      </div>
      <section className="wall-gallery" id="toy-wall">
        <div className="wall-toolbar">
          <div>
            <span className="wall-section-label">Community signal</span>
            <h2>Fresh from the PPU</h2>
          </div>
          <div className="sort-tabs" aria-label="Sort toys">
            <button
              className={`sort-tab${sort === "recent" ? " sort-tab--on" : ""}`}
              aria-pressed={sort === "recent"}
              onClick={() => setSort("recent")}
            >
              Recent
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
        {!loading && cards.length === 0 && (
          <p className="wall-empty">No toys yet — be the first to publish one.</p>
        )}
        {loading && (
          <div className="wall-loading" aria-label="Loading toys">
            <span />
            <span />
            <span />
          </div>
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
      </section>
    </div>
  );
}
