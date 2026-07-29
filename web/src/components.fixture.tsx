import { RouterStage } from "./cosmos/FixtureStage";
import { makeWallCard } from "./fixtures";
import { PlayerFrame } from "./components/ReadOnlyPlayer";
import { ToyCard } from "./components/ToyCard";
import "./components/cards.css";

export default (
  <RouterStage>
    <div style={{ display: "grid", gap: 24, padding: 24, maxWidth: 720 }}>
      <ToyCard card={makeWallCard()} signedIn />
      <PlayerFrame />
    </div>
  </RouterStage>
);
