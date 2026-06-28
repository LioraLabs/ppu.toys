import { useState } from "react";
import { useInspectorFrame } from "./useInspectorFrame";
import { RegistersTab } from "./RegistersTab";
import { SpritesTab } from "./SpritesTab";
import { VramTab } from "./VramTab";
import "./inspector.css";

type Tab = "registers" | "sprites" | "vram";
const TABS: { id: Tab; label: string }[] = [
  { id: "registers", label: "REGISTERS" },
  { id: "sprites", label: "SPRITES" },
  { id: "vram", label: "VRAM" },
];

export function Inspector() {
  const [tab, setTab] = useState<Tab>("registers");
  const frame = useInspectorFrame();
  return (
    <div className="inspector">
      <div className="insp-tabs">
        {TABS.map((t) => (
          <div
            key={t.id}
            className={"insp-tab" + (tab === t.id ? " insp-tab--active" : "")}
            onClick={() => setTab(t.id)}
          >
            {t.label}
          </div>
        ))}
      </div>
      {tab === "registers" && <RegistersTab frame={frame} />}
      {tab === "sprites" && <SpritesTab frame={frame} />}
      {tab === "vram" && <VramTab />}
    </div>
  );
}
