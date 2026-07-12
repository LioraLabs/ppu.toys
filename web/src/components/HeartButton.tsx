import { useState } from "react";
import { addHeart, removeHeart } from "../api/apiClient";

/** Optimistic heart toggle. Signed-out users see it disabled (the server would
 *  401 the mutation anyway). Reverts on API failure. */
export function HeartButton({
  id, heartCount, hearted, signedIn,
}: { id: string; heartCount: number; hearted: boolean; signedIn: boolean }) {
  const [state, setState] = useState({ hearted, count: heartCount });

  async function toggle() {
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
      aria-label={state.hearted ? "Remove heart" : "Heart"}
      aria-pressed={state.hearted}
      disabled={!signedIn}
      onClick={toggle}
    >
      <span className="heart-icon">{state.hearted ? "♥" : "♡"}</span>
      <span className="heart-count">{state.count}</span>
    </button>
  );
}
