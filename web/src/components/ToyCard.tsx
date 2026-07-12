import { Link } from "react-router-dom";
import type { WallCard } from "../api/apiClient";
import { HeartButton } from "./HeartButton";
import "./cards.css";

/** One clip on the Wall / a profile grid. The clip autoplays muted+looped
 *  (native-res, cheap) with the thumbnail as its poster so a card paints
 *  instantly before the video loads. */
export function ToyCard({ card, signedIn }: { card: WallCard; signedIn: boolean }) {
  return (
    <div className="toy-card">
      <Link to={`/t/${card.id}`} className="toy-card-clip" tabIndex={-1} aria-hidden="true">
        <video
          className="toy-card-video"
          src={card.clipUrl}
          poster={card.thumbUrl}
          muted
          loop
          autoPlay
          playsInline
          preload="none"
        />
      </Link>
      <div className="toy-card-meta">
        <div className="toy-card-text">
          <Link to={`/t/${card.id}`} className="toy-card-title">{card.title}</Link>
          <Link to={`/u/${card.author.handle}`} className="toy-card-author">
            {card.author.handle}
          </Link>
        </div>
        <HeartButton
          id={card.id}
          heartCount={card.heartCount}
          hearted={card.hearted}
          signedIn={signedIn}
        />
      </div>
    </div>
  );
}
