import { useEffect, useRef, useState, type ReactNode } from "react";
import { PAD } from "../studio/transport/pad";
import "./touch-controller.css";

const directions = [
  ["Up left", "↖", PAD.up | PAD.left],
  ["Up", "↑", PAD.up],
  ["Up right", "↗", PAD.up | PAD.right],
  ["Left", "←", PAD.left],
  ["Center", "", 0],
  ["Right", "→", PAD.right],
  ["Down left", "↙", PAD.down | PAD.left],
  ["Down", "↓", PAD.down],
  ["Down right", "↘", PAD.down | PAD.right],
] as const;

export function TouchController({
  onChange,
  children,
}: {
  onChange: (mask: number) => void;
  children?: ReactNode;
}) {
  const held = useRef(new Map<number | string, number>());
  const root = useRef<HTMLDivElement>(null);
  const [mask, setMask] = useState(0);
  function update(id: number | string, bit?: number) {
    if (bit !== undefined) held.current.set(id, bit);
    else held.current.delete(id);
    const next = [...held.current.values()].reduce((a, b) => a | b, 0);
    setMask(next);
    onChange(next);
  }
  useEffect(() => {
    const clear = () => {
      held.current.clear();
      setMask(0);
      onChange(0);
    };
    window.addEventListener("blur", clear);
    document.addEventListener("visibilitychange", clear);
    return () => {
      window.removeEventListener("blur", clear);
      document.removeEventListener("visibilitychange", clear);
      clear();
    };
  }, [onChange]);

  function button(label: string, text: string, bit: number, className = "") {
    return (
      <button
        key={label}
        type="button"
        className={`touch-key ${className}`}
        aria-label={label}
        aria-pressed={bit !== 0 && (mask & bit) === bit}
        data-pad={bit}
        onPointerDown={(event) => {
          if (event.button !== 0) return;
          event.preventDefault();
          event.currentTarget.setPointerCapture(event.pointerId);
          update(event.pointerId, bit);
        }}
        onPointerMove={(event) => {
          if (!held.current.has(event.pointerId)) return;
          const target = document
            .elementFromPoint(event.clientX, event.clientY)
            ?.closest<HTMLButtonElement>("[data-pad]");
          update(
            event.pointerId,
            target && root.current?.contains(target) ? Number(target.dataset.pad) : 0,
          );
        }}
        onPointerUp={(event) => update(event.pointerId)}
        onPointerCancel={(event) => update(event.pointerId)}
        onLostPointerCapture={(event) => update(event.pointerId)}
        onKeyDown={(event) => {
          if (event.key !== " " && event.key !== "Enter") return;
          event.preventDefault();
          update(label, bit);
        }}
        onKeyUp={(event) => {
          if (event.key !== " " && event.key !== "Enter") return;
          event.preventDefault();
          update(label);
        }}
        onBlur={() => update(label)}
        onContextMenu={(event) => event.preventDefault()}
      >
        {text}
      </button>
    );
  }

  return (
    <div className="touch-controller" ref={root} role="group" aria-label="Touch controller">
      <div className="touch-shoulders">
        {button("L", "L", PAD.l)}
        {children ? <div className="touch-tools">{children}</div> : <span>PPU / CONTROL DECK</span>}
        {button("R", "R", PAD.r)}
      </div>
      <div className="touch-main">
        <div className="touch-dpad" role="group" aria-label="Directional pad">
          {directions.map(([label, text, bit]) =>
            bit
              ? button(label, text, bit, label.includes(" ") ? "touch-diagonal" : "")
              : button(label, text, bit, "touch-center"),
          )}
        </div>
        <div className="touch-face" role="group" aria-label="Action buttons">
          {button("X", "X", PAD.x, "touch-x")}
          {button("Y", "Y", PAD.y, "touch-y")}
          {button("A", "A", PAD.a, "touch-a")}
          {button("B", "B", PAD.b, "touch-b")}
        </div>
      </div>
      <div className="touch-system">
        {button("Select", "SELECT", PAD.select)}
        {button("Start", "START", PAD.start)}
      </div>
    </div>
  );
}
