import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import "./copyToast.css";

/** House click-to-copy (handoff prototype: clipboard write + "Copied …" toast
 *  that fades after ~1.5 s). One hook instance per surface (tab / overlay / row). */
export function useCopyToast(): { toast: ReactNode; copy: (label: string) => void } {
  const [msg, setMsg] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);
  const copy = useCallback((label: string) => {
    try {
      void navigator.clipboard?.writeText(label).catch(() => {});
    } catch {
      /* clipboard unavailable (permissions/tests) — the toast still confirms intent */
    }
    setMsg(`Copied ${label}`);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => setMsg(null), 1500);
  }, []);
  return { toast: msg ? <div className="tm-toast">{msg}</div> : null, copy };
}

/** Click-to-copy text: shows `label`, copies `label` (dotted-underline accent). */
export function Copyable({
  label,
  onCopy,
  cyan,
}: {
  label: string;
  onCopy: (label: string) => void;
  cyan?: boolean;
}) {
  return (
    <button type="button" className={"tm-copy" + (cyan ? " tm-copy--cyan" : "")} onClick={() => onCopy(label)}>
      {label}
    </button>
  );
}
