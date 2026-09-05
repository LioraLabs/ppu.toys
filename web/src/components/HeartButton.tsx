import { useRef, useState, type Ref } from "react";
import { addHeart, goToSignIn, removeHeart } from "../api/apiClient";

/** Optimistic heart toggle. Signed out it stays LIVE and routes to sign-in —
 *  hearting is the core social ask, so it must never read as a dead control.
 *  Reverts on API failure. */
export function HeartButton({
  id,
  heartCount,
  hearted,
  signedIn,
  buttonRef,
  onHeart,
}: {
  id: string;
  heartCount: number;
  hearted: boolean;
  signedIn: boolean;
  buttonRef?: Ref<HTMLButtonElement>;
  onHeart?: () => void;
}) {
  const [state, setState] = useState({ hearted, count: heartCount });

  const inFlight = useRef(false);
  const [pending, setPending] = useState(false);
  const [failed, setFailed] = useState(false);

  async function toggle() {
    if (inFlight.current) return;
    if (!signedIn) {
      goToSignIn();
      return;
    }
    inFlight.current = true;
    setPending(true);
    setFailed(false);
    const next = !state.hearted;
    const prev = state;
    setState({ hearted: next, count: state.count + (next ? 1 : -1) });
    try {
      await (next ? addHeart(id) : removeHeart(id));
      if (next) onHeart?.();
    } catch {
      setState(prev); // revert on failure
      setFailed(true);
    } finally {
      inFlight.current = false;
      setPending(false);
    }
  }

  return (
    <>
      <button
        ref={buttonRef}
        type="button"
        className={`heart${state.hearted ? " heart--on" : ""}`}
        aria-label={signedIn ? (state.hearted ? "Remove heart" : "Heart") : "Sign in to heart"}
        aria-pressed={state.hearted}
        disabled={pending}
        title={signedIn ? undefined : "Sign in with Discord to heart toys"}
        onClick={toggle}
      >
        <span className="heart-icon">{state.hearted ? "♥" : "♡"}</span>
        <span className="heart-count">{state.count}</span>
      </button>
      {failed && (
        <span className="heart-error" role="alert">
          Heart failed. Try again.
        </span>
      )}
    </>
  );
}
