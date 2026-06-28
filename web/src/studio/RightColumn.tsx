import { OutputCanvas } from "./output/OutputCanvas";
import { Inspector } from "./inspector/Inspector";

export function RightColumn() {
  return (
    <aside className="right">
      <OutputCanvas />
      <Inspector />
    </aside>
  );
}
