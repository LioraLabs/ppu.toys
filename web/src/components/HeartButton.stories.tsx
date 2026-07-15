import type { Story, StoryDefault } from "@ladle/react";
import { HeartButton } from "./HeartButton";
import "./cards.css";

// HeartButton is a pure props component. Its only import is addHeart/removeHeart
// from apiClient; the Ladle global MSW worker already answers those (204), so the
// optimistic toggle works in the catalog without any backend.
export default {
  title: "Components/HeartButton",
} satisfies StoryDefault;

export const SignedIn: Story = () => (
  <HeartButton id="abc123" heartCount={3} hearted={false} signedIn />
);

export const Hearted: Story = () => (
  <HeartButton id="abc123" heartCount={4} hearted signedIn />
);

export const SignedOut: Story = () => (
  <HeartButton id="abc123" heartCount={3} hearted={false} signedIn={false} />
);

export const ZeroCount: Story = () => (
  <HeartButton id="abc123" heartCount={0} hearted={false} signedIn />
);

export const HighCount: Story = () => (
  <HeartButton id="abc123" heartCount={12345} hearted signedIn />
);
