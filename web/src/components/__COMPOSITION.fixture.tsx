import { RouterStage } from "../cosmos/FixtureStage";
import { makeWallCard } from "../fixtures";
import { PlayerFrame } from "./ReadOnlyPlayer";
import { ToyCard } from "./ToyCard";
import "./cards.css";

export default (
  <RouterStage>
    <div style={{ display: "grid", gap: 24, padding: 24, maxWidth: 720 }}>
      <ToyCard card={makeWallCard()} signedIn />
      <PlayerFrame />
    </div>
  </RouterStage>
);
