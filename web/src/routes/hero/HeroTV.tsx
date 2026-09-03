/** Wired landing hero: always renders the CRT scene, then tunes it to the
 *  featured toy when one is available. */
import { useCallback, useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { getFeaturedToy, getToy, type ToyFull } from "../../api/apiClient";
import { decodeBase64 } from "../../api/base64";
import { ppuCore } from "../../ppu/instance";
import { transport } from "../../studio/transport/transport";
import { padKeyHandlers, PAD_HINT } from "../../studio/transport/pad";
import { discordAvatarUrl } from "../../components/Avatar";
import { HeroStage } from "./HeroStage";
import "./hero.css";

export default function HeroTV() {
  const [toy, setToy] = useState<ToyFull | null>(null);
  const [pad] = useState(() => padKeyHandlers(transport.setPad));
  // Cheap tell for "this toy takes input": it mentions the pad global.
  const playable = !!toy && toy.files.some((f) => /\bpad\b/.test(f.source));

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

  // Same /blobs/thumb/{id} convention the server uses for og:image.
  const stage = (
    <HeroStage
      getFrame={getFrame}
      onFail={() => {}}
      pad={pad}
      cart={
        toy && {
          thumbUrl: `/blobs/thumb/${toy.id}`,
          avatarUrl: discordAvatarUrl(toy.author.id, toy.author.avatar, 128),
          handle: toy.author.handle,
        }
      }
    />
  );
  return (
    <div className="hero3d">
      {/* The TV itself is the play surface (click to focus, then keys); the
          caption carries the permalink so the toy is still one click away. */}
      <div className="hero3d-link" title={playable ? `Click to play · ${PAD_HINT}` : undefined}>
        {stage}
      </div>
      <span className="hero3d-caption">
        <i />
        {toy ? (
          <Link to={`/t/${toy.id}`} title={`Watch ${toy.title}`}>
            NOW PLAYING&ensp;{toy.title} — {toy.author.handle}
          </Link>
        ) : (
          "NO SIGNAL"
        )}
      </span>
      {playable && <span className="hero3d-hint">▶ CLICK THE TV TO PLAY · {PAD_HINT}</span>}
    </div>
  );
}
