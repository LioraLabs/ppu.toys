import { SpritesTab } from "./SpritesTab";
import { frameResult, makeFrameResult } from "../../fixtures";
import "./inspector.css";

// SpritesTab is a pure props component: given a FrameResult it renders the OAM
// rows and RANGE/TIME OVER badges with no wasm core on the render path.
const Default = () => <SpritesTab frame={frameResult} />;

const Overflow = () => (
  <SpritesTab
    frame={makeFrameResult({
      objOverflow: { rangeOver: true, timeOver: true, maxSprites: 32, maxTiles: 34 },
    })}
  />
);

const Empty = () => (
  <SpritesTab frame={makeFrameResult({ oam: frameResult.oam.map((s) => ({ ...s, on: false })) })} />
);

const Waiting = () => <SpritesTab frame={null} />;

export default {
  Default,
  Overflow,
  Empty,
  Waiting,
};
