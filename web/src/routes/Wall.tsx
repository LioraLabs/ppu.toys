import { lazy, Suspense, useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { getHighlights, getWall, type WallCard } from "../api/apiClient";
import { useDocumentTitle } from "./useDocumentTitle";
import { useSession } from "../api/session";
import { ToyCard } from "../components/ToyCard";
import "../components/cards.css";
import "./wall.css";

const HeroTV = lazy(() => import("./hero/HeroTV"));
const KOFI_SCRIPT = "https://storage.ko-fi.com/cdn/scripts/overlay-widget.js";

declare global {
  interface Window {
    kofiWidgetOverlay?: {
      draw: (id: string, config: Record<string, string>, containerId?: string) => void;
    };
  }
}

function KofiWidget() {
  useEffect(() => {
    let live = true;
    const draw = () => {
      if (!live || !window.kofiWidgetOverlay) return;
      window.kofiWidgetOverlay.draw(
        "X8X21XWLH3",
        {
          type: "floating-chat",
          "floating-chat.donateButton.text": "Support ppu toys on Ko-fi",
          "floating-chat.donateButton.background-color": "#eba121",
          "floating-chat.donateButton.text-color": "#fff",
        },
        "kofi-widget",
      );
    };
    const existing = document.querySelector<HTMLScriptElement>(`script[src="${KOFI_SCRIPT}"]`);
    const script =
      existing ?? Object.assign(document.createElement("script"), { src: KOFI_SCRIPT });
    script.addEventListener("load", draw);
    if (existing) draw();
    else document.head.appendChild(script);
    return () => {
      live = false;
      script.removeEventListener("load", draw);
    };
  }, []);
  return (
    <div className="kofi-trigger">
      <span>Support on Ko-fi</span>
      <div id="kofi-widget" />
    </div>
  );
}

export function Wall() {
  useDocumentTitle();
  const { user } = useSession();
  const [highlights, setHighlights] = useState<WallCard[]>([]);
  const [latest, setLatest] = useState<WallCard[]>([]);

  useEffect(() => {
    void getHighlights().then(({ toys }) => setHighlights(toys));
    void getWall("recent", 0).then(({ toys }) => setLatest(toys.slice(0, 5)));
  }, []);

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
            <Link className="wall-secondary" to="/browse">
              Explore toys
            </Link>
            <KofiWidget />
          </div>
        </div>
        <div className="wall-hero-scene">
          <Suspense>
            <HeroTV />
          </Suspense>
        </div>
      </div>

      <section className="wall-gallery">
        <div className="wall-toolbar">
          <div>
            <span className="wall-section-label">Community highlights</span>
            <h2>Featured PPU Toys</h2>
          </div>
        </div>
        <div className="wall-grid">
          {highlights.map((card) => (
            <ToyCard key={card.id} card={card} signedIn={!!user} />
          ))}
        </div>
      </section>

      <section className="wall-latest">
        <div>
          <span className="wall-section-label">Latest contributions</span>
          <h2>New from the Community</h2>
        </div>
        <ol>
          {latest.map((card) => (
            <li key={card.id}>
              <Link to={`/t/${card.id}`}>{card.title}</Link>
              <span> by </span>
              <Link to={`/u/${card.author.handle}`}>{card.author.handle}</Link>
            </li>
          ))}
        </ol>
        <Link className="wall-browse-all" to="/browse">
          Browse all toys <span aria-hidden="true">→</span>
        </Link>
      </section>
    </div>
  );
}
