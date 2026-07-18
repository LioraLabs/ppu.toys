import { Toolbar } from "./Toolbar";
import { transport } from "./transport/transport";
import { useTheme } from "./theme";
import { AddSourceButton } from "./sources/AddSourceButton";
import { WorkspaceActions } from "./cloud/WorkspaceActions";

export interface ToolbarWiredProps {
  /** Open-sketch name (from the sketch store via Studio). */
  sketchName?: string;
  /** Unsaved-changes marker (from the sketch store via Studio). */
  dirty?: boolean;
}

/** Wired container: reads the theme store and drives the transport, injecting
 *  the wired AddSourceButton / WorkspaceActions as the presentational toolbar's
 *  slots. Render-identical to the pre-split Toolbar. */
export function ToolbarWired({ sketchName, dirty }: ToolbarWiredProps) {
  const { theme, toggleTheme } = useTheme();
  return (
    <Toolbar
      sketchName={sketchName}
      dirty={dirty}
      theme={theme}
      onRun={() => transport.restart()}
      onToggleTheme={toggleTheme}
      sourceSlot={<AddSourceButton />}
      workspaceSlot={<WorkspaceActions />}
    />
  );
}
