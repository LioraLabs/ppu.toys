/** Wired landing hero: fetches the featured toy (top of the popular wall),
 *  pushes it into the SHARED transport/core exactly like ReadOnlyPlayer, and
 *  hands live framebuffers to the 3D stage. Renders `fallback` (the CSS hero
 *  art) when there's nothing to show: reduced motion, WebGL missing, fetch
 *  failure, or an empty wall. Default export so Wall can React.lazy this
 *  chunk — three.js stays out of every other bundle. */
import { useCallback, useEffect, useState, type ReactNode } from "react";
import { Link } from "react-router-dom";
import { getFeaturedToy, getToy, type ToyFull } from "../../api/apiClient";
import { decodeBase64 } from "../../api/base64";
import { ppuCore } from "../../ppu/instance";
import { transport } from "../../studio/transport/transport";
import { HeroStage } from "./HeroStage";
import "./hero.css";

export default function HeroTV({ fallback }: { fallback: ReactNode }) {
  const [toy, setToy] = useState<ToyFull | null>(null);
  const [failed, setFailed] = useState(
    () => window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false,
  );

  useEffect(() => {
    let live = true;
    getFeaturedToy()
      .then((featured) =>
        featured.id ? getToy(featured.id) : Promise.reject(new Error("no featured toy")),
      )
      .then((t) => live && setToy(t))
      .catch(() => live && setFailed(true));
    return () => {
      live = false;
    };
  }, []);

  // Push the featured program into the shared core — sources BEFORE files, so
  // setSources' setup-stage dma() placements find them registered. The (no-op)
  // subscription keeps the transport's rAF clock running while the hero is on
  // screen.
  useEffect(() => {
    if (!toy || !ppuCore) return;
    for (const s of toy.sources) {
      if (s.payload) transport.addSource(s.name, decodeBase64(s.payload));
    }
    transport.setSources(toy.files);
    return transport.subscribe(() => {});
  }, [toy]);

  const ready = !!toy && !!ppuCore;
  const getFrame = useCallback(
    () => (ready ? transport.getSnapshot().frame.framebuffer : null),
    [ready],
  );

  if (failed) return <>{fallback}</>;

  const stage = <HeroStage getFrame={getFrame} onFail={() => setFailed(true)} />;
  return (
    <div className="hero3d">
      {toy ? (
        <Link className="hero3d-link" to={`/t/${toy.id}`} title={`Watch ${toy.title}`}>
          {stage}
        </Link>
      ) : (
        <div className="hero3d-link">{stage}</div>
      )}
      <span className="hero3d-caption">
        <i />
        {toy ? (
          <>
            NOW PLAYING&ensp;{toy.title} — {toy.author.handle}
          </>
        ) : (
          "TUNING IN…"
        )}
      </span>
    </div>
  );
}
