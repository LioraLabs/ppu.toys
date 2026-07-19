import { makeWallCard } from "../fixtures";
import { ToyCard } from "./ToyCard";
import { RouterStage } from "../cosmos/FixtureStage";

const Default = () => <RouterStage><ToyCard card={makeWallCard()} signedIn /></RouterStage>;

const SignedOut = () => <RouterStage><ToyCard card={makeWallCard()} signedIn={false} /></RouterStage>;

const LongTitle = () => (
  <RouterStage>
    <ToyCard
      card={makeWallCard({
        title:
          "An extremely long clip title that goes on and on and should overflow the card and ellipsize instead of wrapping the layout",
      })}
      signedIn
    />
  </RouterStage>
);

const HighHeartCount = () => (
  <RouterStage><ToyCard card={makeWallCard({ heartCount: 12345, hearted: true })} signedIn /></RouterStage>
);

export default {
  Default,
  SignedOut,
  LongTitle,
  HighHeartCount,
};
