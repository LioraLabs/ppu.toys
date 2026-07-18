import { useEffect } from "react";
import type { Story, StoryDefault } from "@ladle/react";
import { DialectToggle, PokeBar, PokeDot, RegRow } from "./chrome";
import { REG, setMathHalf, setMathOp, writesToPokes } from "./model";
import { frameResult } from "../../../fixtures";
import { makeFixtureCompositor } from "./storyCompositor";
import { clearPokes, pokeMany } from "../../pokes/pokeStore";
import "./compose.css";
import "../inspector.css";
import "../../pokes/pokes.css";

// chrome.tsx holds the small poke-aware chrome shared across Compose/Windows:
// DialectToggle (a persisted preference store, wasm-free), RegRow (a copyable
// register readout row, reading a Compositor built by makeFixtureCompositor —
// no wasm core), and PokeBar/PokeDot (read the pokes.lua-backed poke store —
// also wasm-free, since pokes are pure DSL text, never core state).
export default {
  title: "Studio/Inspector/Compose/Chrome",
} satisfies StoryDefault;

const c = makeFixtureCompositor(frameResult);

export const DialectToggleStory: Story = () => <DialectToggle />;
DialectToggleStory.storyName = "DialectToggle";

export const RegRowStory: Story = () => (
  <RegRow c={c} addr={REG.CGADSUB} name="CGADSUB" note="add" />
);
RegRowStory.storyName = "RegRow";

// PokeDot is the marker a poked control wears; it returns null when nothing is
// poked. Here the fixture compositor's pokedAt is overridden to report one live
// friendly poke on CGADSUB, so the dot renders (solid = live matches). Shown
// next to a control label, exactly how ComposeSections/WindowSections use it.
const pokedC = { ...c, pokedAt: () => writesToPokes([setMathOp("sub", 0x00)], "friendly") };
export const PokeDotStory: Story = () => (
  <div className="cmp-ctl-label">
    MATH · $2131
    <PokeDot c={pokedC} addr={REG.CGADSUB} />
  </div>
);
PokeDotStory.storyName = "PokeDot";

// PokeBar renders nothing when no poke exists (it returns null) — this story
// documents that empty state. clearPokes() on mount guarantees it, regardless
// of what an earlier story left behind.
function EmptyPokeBar() {
  useEffect(() => {
    clearPokes();
  }, []);
  return <PokeBar />;
}

export const PokeBarEmpty: Story = () => <EmptyPokeBar />;

// Seeds the poke store with two sample friendly-dialect pokes (color math op
// + half toggle) via the same writesToPokes/pokeMany path a real control
// click takes, so PokeBar renders a visibly-populated bar. Cleans up on
// unmount so the seeded pokes don't leak into other stories/sessions.
function PopulatedPokeBar() {
  useEffect(() => {
    pokeMany(
      writesToPokes(
        [setMathOp("sub", 0x00), setMathHalf(true, 0x00)],
        "friendly",
      ),
    );
    return () => clearPokes();
  }, []);
  return <PokeBar />;
}

export const PokeBarPopulated: Story = () => <PopulatedPokeBar />;
