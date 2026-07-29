import { useState } from "react";
import { addHeart, goToSignIn, removeHeart } from "../api/apiClient";

/** Optimistic heart toggle. Signed out it stays LIVE and routes to sign-in —
 *  hearting is the core social ask, so it must never read as a dead control.
 *  Reverts on API failure. */
export function HeartButton({
  id,
  heartCount,
  hearted,
  signedIn,
}: {
  id: string;
  heartCount: number;
  hearted: boolean;
  signedIn: boolean;
}) {
  const [state, setState] = useState({ hearted, count: heartCount });

  async function toggle() {
    if (!signedIn) {
      goToSignIn();
      return;
    }
    const next = !state.hearted;
    const prev = state;
    setState({ hearted: next, count: state.count + (next ? 1 : -1) });
    try {
      await (next ? addHeart(id) : removeHeart(id));
    } catch {
      setState(prev); // revert on failure
    }
  }

  return (
    <button
      type="button"
      className={`heart${state.hearted ? " heart--on" : ""}`}
      aria-label={signedIn ? (state.hearted ? "Remove heart" : "Heart") : "Sign in to heart"}
      aria-pressed={state.hearted}
      title={signedIn ? undefined : "Sign in with Discord to heart toys"}
      onClick={toggle}
    >
      <span className="heart-icon">{state.hearted ? "♥" : "♡"}</span>
      <span className="heart-count">{state.count}</span>
    </button>
  );
}
