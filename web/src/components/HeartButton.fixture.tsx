import { HeartButton } from "./HeartButton";
import "./cards.css";

// HeartButton is a pure props component. Its only import is addHeart/removeHeart
// from apiClient; the Cosmos global MSW worker already answers those (204), so the
// optimistic toggle works in the catalog without any backend.
const SignedIn = () => (
  <HeartButton id="abc123" heartCount={3} hearted={false} signedIn />
);

const Hearted = () => (
  <HeartButton id="abc123" heartCount={4} hearted signedIn />
);

const SignedOut = () => (
  <HeartButton id="abc123" heartCount={3} hearted={false} signedIn={false} />
);

const ZeroCount = () => (
  <HeartButton id="abc123" heartCount={0} hearted={false} signedIn />
);

const HighCount = () => (
  <HeartButton id="abc123" heartCount={12345} hearted signedIn />
);

export default {
  SignedIn,
  Hearted,
  SignedOut,
  ZeroCount,
  HighCount,
};
