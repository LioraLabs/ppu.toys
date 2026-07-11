import { useState } from "react";
import { AddSourceDialog } from "./AddSourceDialog";

export function AddSourceButton() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" className="btn-ghost" onClick={() => setOpen(true)}>+ Source</button>
      {open && <AddSourceDialog onClose={() => setOpen(false)} />}
    </>
  );
}
