import type { Story, StoryDefault } from "@ladle/react";
import { SpritesTab } from "./SpritesTab";
import { frameResult, makeFrameResult } from "../../fixtures";
import "./inspector.css";

// SpritesTab is a pure props component: given a FrameResult it renders the OAM
// rows and RANGE/TIME OVER badges with no wasm core on the render path.
export default {
  title: "Studio/Inspector/SpritesTab",
} satisfies StoryDefault;

export const Default: Story = () => <SpritesTab frame={frameResult} />;

export const Overflow: Story = () => (
  <SpritesTab
    frame={makeFrameResult({
      objOverflow: { rangeOver: true, timeOver: true, maxSprites: 32, maxTiles: 34 },
    })}
  />
);

export const Empty: Story = () => (
  <SpritesTab frame={makeFrameResult({ oam: frameResult.oam.map((s) => ({ ...s, on: false })) })} />
);

export const Waiting: Story = () => <SpritesTab frame={null} />;
