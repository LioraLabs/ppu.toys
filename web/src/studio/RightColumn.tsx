import { OutputCanvas } from "./output/OutputCanvas";
import { Inspector } from "./inspector/Inspector";
import { ErrorBoundary } from "./ErrorBoundary";

export function RightColumn() {
  return (
    <aside className="right">
      <ErrorBoundary label="Output">
        <OutputCanvas />
      </ErrorBoundary>
      <ErrorBoundary label="Inspector">
        <Inspector />
      </ErrorBoundary>
    </aside>
  );
}
