import type { Story, StoryDefault } from "@ladle/react";
import { withRouter } from "../../.ladle/decorators";
import { makeWallCard } from "../fixtures";
import { ToyCard } from "./ToyCard";

export default {
  title: "Components/ToyCard",
  decorators: [withRouter],
} satisfies StoryDefault;

export const Default: Story = () => <ToyCard card={makeWallCard()} signedIn />;

export const SignedOut: Story = () => <ToyCard card={makeWallCard()} signedIn={false} />;

export const LongTitle: Story = () => (
  <ToyCard
    card={makeWallCard({
      title:
        "An extremely long clip title that goes on and on and should overflow the card and ellipsize instead of wrapping the layout",
    })}
    signedIn
  />
);

export const HighHeartCount: Story = () => (
  <ToyCard card={makeWallCard({ heartCount: 12345, hearted: true })} signedIn />
);
