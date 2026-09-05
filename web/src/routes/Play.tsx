import { useEffect, useMemo, useRef, useState, type Ref } from "react";
import { Link, useNavigate, useParams, useSearchParams } from "react-router-dom";
import { getToy, getWall, type ToyFull, type WallCard } from "../api/apiClient";
import { sessionStore, useSession } from "../api/session";
import { decodeBase64 } from "../api/base64";
import { HeartButton } from "../components/HeartButton";
import { ReadOnlyPlayer } from "../components/ReadOnlyPlayer";
import { useDocumentTitle } from "./useDocumentTitle";
import "../components/cards.css";
import "./permalink.css";
import "./play.css";

export function Play() {
  const { id = "" } = useParams();
  const navigate = useNavigate();
  const [params] = useSearchParams();
  const tag = params.get("tag") ?? "";
  const author = params.get("author") ?? "";
  const { user } = useSession();
  const [toy, setToy] = useState<ToyFull | null>(null);
  const [error, setError] = useState(false);
  const [retry, setRetry] = useState(0);
  const [cards, setCards] = useState<WallCard[]>([]);
  const [nextPage, setNextPage] = useState<number | null>(0);
  const [feedBusy, setFeedBusy] = useState(true);
  const [feedError, setFeedError] = useState(false);
  const [history, setHistory] = useState<string[]>([]);
  const feedRequest = useRef(0);
  const moving = useRef(false);
  const heartButton = useRef<HTMLButtonElement>(null);
  const swipe = useRef<{ id: number; x: number; y: number; time: number; moved: boolean } | null>(
    null,
  );
  const current = toy?.id === id ? toy : null;
  useDocumentTitle(current ? `${current.title} · ${current.author.handle}` : "Play");
  const sources = useMemo(
    () =>
      current?.sources
        .filter((s) => s.payload)
        .map((s) => ({
          name: s.name,
          payload: decodeBase64(s.payload!),
        })) ?? [],
    [current],
  );

  // Play has no site shell, so resolve the session for direct links too.
  useEffect(() => {
    void sessionStore.refresh().catch(() => {});
  }, []);
  useEffect(() => {
    let live = true;
    moving.current = true;
    setError(false);
    getToy(id)
      .then((next) => {
        if (live) {
          setToy(next);
          moving.current = false;
        }
      })
      .catch(() => {
        if (live) {
          setError(true);
          moving.current = false;
        }
      });
    return () => {
      live = false;
    };
  }, [id, retry]);

  useEffect(() => {
    const request = ++feedRequest.current;
    moving.current = false;
    setCards([]);
    setHistory([]);
    setNextPage(0);
    setFeedBusy(true);
    setFeedError(false);
    getWall("recent", 0, "", { tag, author })
      .then((page) => {
        if (request !== feedRequest.current) return;
        setCards(page.toys);
        setNextPage(page.nextPage);
      })
      .catch(() => {
        if (request === feedRequest.current) setFeedError(true);
      })
      .finally(() => {
        if (request === feedRequest.current) setFeedBusy(false);
      });
    return () => {
      ++feedRequest.current;
    };
  }, [tag, author]);

  const seen = new Set([...history, id]);

  function go(next: string) {
    moving.current = true;
    navigate(`/t/${next}/play${params.size ? `?${params}` : ""}`, { replace: true });
  }
  async function advance() {
    if (moving.current || feedBusy) return;
    moving.current = true;
    setFeedError(false);
    let available = cards;
    let pageNumber = nextPage;
    const request = feedRequest.current;
    try {
      // Skip seen toys, including the initial deep link, across page boundaries.
      let next = available.find((card) => !seen.has(card.id));
      while (!next && pageNumber !== null) {
        setFeedBusy(true);
        const page = await getWall("recent", pageNumber, "", { tag, author });
        if (request !== feedRequest.current) return;
        available = [...available, ...page.toys];
        pageNumber = page.nextPage;
        setCards(available);
        setNextPage(pageNumber);
        next = available.find((card) => !seen.has(card.id));
      }
      if (next) {
        setHistory((previous) => [...previous, id]);
        go(next.id);
      }
    } catch {
      if (request === feedRequest.current) setFeedError(true);
    } finally {
      if (request === feedRequest.current) {
        setFeedBusy(false);
        moving.current = false;
      }
    }
  }
  function previous() {
    if (moving.current || feedBusy || !history.length) return;
    const last = history[history.length - 1];
    setHistory((visited) => visited.slice(0, -1));
    go(last);
  }
  const exhausted = nextPage === null && !cards.some((card) => !seen.has(card.id));

  return (
    <main className="play-page play-feed">
      {current && (
        <header className="play-credit">
          <h1>
            <Link to={`/t/${id}`}>{current.title}</Link>
          </h1>
          <Link className="play-creator" to={`/u/${current.author.handle}`}>
            @{current.author.handle}
          </Link>
        </header>
      )}
      <div
        className="play-surface"
        onPointerDown={(event) => {
          if (!event.isPrimary) {
            swipe.current = null;
            return;
          }
          const target = event.target as HTMLElement;
          if (event.button !== 0 || !target.closest(".player, canvas") || target.closest("button"))
            return;
          swipe.current = {
            id: event.pointerId,
            x: event.clientX,
            y: event.clientY,
            time: Date.now(),
            moved: false,
          };
          event.currentTarget.setPointerCapture(event.pointerId);
        }}
        onPointerMove={(event) => {
          const start = swipe.current;
          if (
            start &&
            start.id === event.pointerId &&
            Math.hypot(event.clientX - start.x, event.clientY - start.y) > 12
          )
            start.moved = true;
        }}
        onPointerUp={(event) => {
          const start = swipe.current;
          swipe.current = null;
          if (!start || start.id !== event.pointerId) return;
          const dy = event.clientY - start.y;
          const dx = event.clientX - start.x;
          if (Math.abs(dy) >= 64 && Math.abs(dy) > Math.abs(dx) * 1.5) {
            if (dy < 0) void advance();
            else previous();
          } else if (
            !start.moved &&
            Math.hypot(dx, dy) <= 12 &&
            Date.now() - start.time < 500 &&
            heartButton.current?.getAttribute("aria-pressed") !== "true"
          ) {
            heartButton.current?.click();
          }
        }}
        onPointerCancel={() => {
          swipe.current = null;
        }}
        onLostPointerCapture={() => {
          swipe.current = null;
        }}
      >
        {current ? (
          <ReadOnlyPlayer key={id} files={current.files} sources={sources} controls>
            <PlayHeart key={id} toy={current} signedIn={!!user} buttonRef={heartButton} />
          </ReadOnlyPlayer>
        ) : (
          <div className="play-load" role={error ? "alert" : "status"}>
            {error ? (
              <>
                This toy couldn’t load.{" "}
                <button className="fork-btn" onClick={() => setRetry((n) => n + 1)}>
                  Retry toy
                </button>
              </>
            ) : (
              "Loading toy…"
            )}
          </div>
        )}
        <nav className="play-accessible-nav" aria-label="Navigate toys">
          <button
            type="button"
            disabled={!history.length || feedBusy || (!current && !error)}
            onClick={previous}
          >
            Previous toy
          </button>
          <button
            type="button"
            disabled={feedBusy || (!current && !error) || (exhausted && !feedError)}
            onClick={() => void advance()}
          >
            Next toy
          </button>
        </nav>
        <span className="play-announcement" role="status">
          {feedBusy ? "Loading toys…" : exhausted ? "End of feed" : ""}
        </span>
        {feedError && (
          <div className="play-feed-error" role="alert">
            Feed couldn’t load.{" "}
            <button type="button" onClick={() => void advance()}>
              Retry feed
            </button>
          </div>
        )}
      </div>
    </main>
  );
}

function PlayHeart({
  toy,
  signedIn,
  buttonRef,
}: {
  toy: ToyFull;
  signedIn: boolean;
  buttonRef: Ref<HTMLButtonElement>;
}) {
  const [flash, setFlash] = useState(0);
  useEffect(() => {
    if (!flash) return;
    const timer = window.setTimeout(() => setFlash(0), 2000);
    return () => window.clearTimeout(timer);
  }, [flash]);
  return (
    <div className={`play-heart${flash ? " play-heart--visible" : ""}`}>
      <HeartButton
        buttonRef={buttonRef}
        id={toy.id}
        heartCount={toy.heartCount}
        hearted={toy.hearted}
        signedIn={signedIn}
        onHeart={() => setFlash((n) => n + 1)}
      />
      {!!flash && (
        <>
          <div key={flash} className="play-heart-thanks" aria-hidden="true">
            <svg viewBox="0 0 24 24">
              <path d="M12 21s-9-5.5-9-12a5 5 0 0 1 9-3 5 5 0 0 1 9 3c0 6.5-9 12-9 12Z" />
            </svg>
          </div>
          <span className="play-announcement" role="status">
            Heart added. Thank you!
          </span>
        </>
      )}
    </div>
  );
}
