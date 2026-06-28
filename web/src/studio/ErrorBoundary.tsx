import { Component, type ErrorInfo, type ReactNode } from "react";

/** Contains render/runtime crashes within a subtree so one panel failing can't
 *  blank the whole Studio. Shows a terse in-place message; the rest of the app
 *  keeps running. Defense-in-depth behind component-level error handling. */
export class ErrorBoundary extends Component<
  { label: string; children: ReactNode },
  { error: Error | null }
> {
  state: { error: Error | null } = { error: null };

  static getDerivedStateFromError(error: Error): { error: Error } {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error(`[ppu.toys] ${this.props.label} crashed:`, error, info.componentStack);
  }

  render(): ReactNode {
    if (this.state.error) {
      return (
        <div className="panel-error" role="alert">
          {this.props.label} failed to render — see the console for details.
        </div>
      );
    }
    return this.props.children;
  }
}
