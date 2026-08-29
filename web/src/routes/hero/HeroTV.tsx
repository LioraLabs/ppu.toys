/** Wired landing hero: always renders the CRT scene, then tunes it to the
 *  featured toy when one is available. */
import { useCallback, useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { getFeaturedToy, getToy, type ToyFull } from "../../api/apiClient";
import { decodeBase64 } from "../../api/base64";
import { ppuCore } from "../../ppu/instance";
import { transport } from "../../studio/transport/transport";
import { HeroStage } from "./HeroStage";
import "./hero.css";

export default function HeroTV() {
  const [toy, setToy] = useState<ToyFull | null>(null);

  useEffect(() => {
    let live = true;
    getFeaturedToy()
      .then((featured) => (featured.id ? getToy(featured.id) : null))
      .then((featured) => featured && live && setToy(featured))
      .catch(() => {});
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

  const stage = <HeroStage getFrame={getFrame} onFail={() => {}} />;
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
          "NO SIGNAL"
        )}
      </span>
    </div>
  );
}
