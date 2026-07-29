import { Toolbar } from "./Toolbar";
import { transport } from "./transport/transport";
import { useTheme } from "./theme";
import { useSession, sessionStore } from "../api/session";
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
  // session refresh is owned by WorkspaceActions (the studio's session seam);
  // this just mirrors the resolved user into the account menu.
  const { user } = useSession();
  return (
    <Toolbar
      sketchName={sketchName}
      dirty={dirty}
      theme={theme}
      user={user && { id: user.id, handle: user.handle, avatar: user.avatar }}
      onRun={() => transport.restart()}
      onToggleTheme={toggleTheme}
      onSignOut={() => void sessionStore.signOut()}
      sourceSlot={<AddSourceButton />}
      workspaceSlot={<WorkspaceActions />}
    />
  );
}
