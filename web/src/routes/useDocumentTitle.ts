import { useEffect } from "react";

const BASE = "ppu.toys";

/** Per-page document.title: "<part> — ppu.toys", or just the base when no part
 *  is ready yet (loading states pass undefined). Restores nothing on unmount —
 *  every page sets its own title, so the last navigation always wins. */
export function useDocumentTitle(part?: string) {
  useEffect(() => {
    document.title = part ? `${part} — ${BASE}` : BASE;
  }, [part]);
}
