import { Mode7Panel } from "./Mode7Panel";
import { makeM7ViewData } from "../../fixtures";
import "../../styles/tokens.css";

// The M7 editor over synthetic plane + fan data (real transform, no wasm).
const noop = () => undefined;
const data = makeM7ViewData();

const Editor = () => (
  <div style={{ width: 1000, padding: 16 }}>
    <Mode7Panel data={data} onPoke={noop} onClearPokes={noop} />
  </div>
);

// Feed with no mode-7 rows: the fan disappears, the hint text explains why.
const NotMode7 = () => (
  <div style={{ width: 1000, padding: 16 }}>
    <Mode7Panel
      data={{ map: data.map, segments: new Float32Array(224 * 4).fill(NaN) }}
      onPoke={noop}
      onClearPokes={noop}
    />
  </div>
);

export default { Editor, NotMode7 };
