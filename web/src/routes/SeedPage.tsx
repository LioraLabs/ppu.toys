import { useRef, useState } from "react";
import { DEMOS } from "../studio/demos/demos";
import { openSketchStore } from "../studio/sketches/openSketch";
import { serializeWorkspace } from "../studio/cloud/serialize";
import { recordClip } from "../studio/cloud/clip";
import { createToy, publishToy, getMe, getProfile } from "../api/apiClient";
import { Studio } from "../studio/Studio";

/** One-line blurbs published with each official demo toy. */
const BLURBS: Record<string, string> = {
  "dusk-parallax": "Parallax BG scroll with a CGRAM colour-cycle sunset and a sprite.",
  "mode7-floor": "The namesake: a per-scanline affine Mode 7 floor.",
  "offset-per-tile": "Mode 2 offset-per-tile — BG3's table drives per-column scroll.",
  "mode3-gradient": "Mode 3's 8bpp 256-colour BG in one smooth gradient.",
  translucency: "Half-add colour math: a glass panel over a scrolling BG.",
  spotlight: "Window masking as a moving spotlight iris.",
  glow: "Additive colour math bloom.",
  "sprite-storm": "128 sprites vs the real per-line limits — authentic flicker.",
  mosaic: "The $2106 mosaic effect, per-BG.",
  "mode7-extbg": "Mode 7 EXTBG: per-pixel priority splits the plane in two.",
  "direct-color": "8bpp direct colour — BGR233 straight from the pixel byte.",
  "tilesheet-cavern":
    "A camera streaming a Tiled-authored map out of a tilesheet, with animated lava and water.",
};

/** DEV-ONLY (route is gated in AppRoutes): publishes every bundled demo as the
 *  signed-in account by driving the real Studio underneath — open demo, let it
 *  render, serialize, record the loop clip, publish. Idempotent by title: demos
 *  already published by this account are skipped. Runbook: web/docs/seeding.md. */
export function SeedPage() {
  const [lines, setLines] = useState<string[]>([]);
  const [running, setRunning] = useState(false);
  const stopRef = useRef(false);

  const log = (m: string) => setLines((ls) => [...ls, m]);

  async function run() {
    if (running) return;
    setRunning(true);
    stopRef.current = false;
    try {
      const me = await getMe();
      if (!me) {
        log("NOT signed in — start dev with PPU_API_COOKIE (see web/docs/seeding.md)");
        return;
      }
      log(`signed in as ${me.handle}`);
      const existing = new Set(
        (await getProfile(me.handle).catch(() => null))?.toys.map((t) => t.title) ?? [],
      );
      for (const d of DEMOS) {
        if (stopRef.current) return void log("stopped");
        if (existing.has(d.label)) {
          log(`skip ${d.id} (already published)`);
          continue;
        }
        log(`open ${d.id}…`);
        await openSketchStore.openDemo(d.id);
        // let EditorPane's restore convert assets + push the program
        await new Promise((r) => setTimeout(r, 2000));
        const { files, sources } = serializeWorkspace(openSketchStore.state());
        const description = BLURBS[d.id] ?? "";
        const { id } = await createToy({ title: d.label, description, files, sources });
        log(`recording clip for ${d.id}…`);
        const { clip, thumb } = await recordClip();
        await publishToy(id, { title: d.label, description }, clip, thumb);
        log(`published ${d.label} → /t/${id}`);
      }
      log("done");
    } catch (e) {
      log(`FAILED: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setRunning(false);
    }
  }

  return (
    <div style={{ position: "relative", height: "100%" }}>
      <Studio />
      <div
        style={{
          position: "fixed",
          top: 60,
          right: 12,
          zIndex: 100,
          width: 340,
          background: "var(--panel)",
          border: "1px solid var(--line)",
          borderRadius: 8,
          padding: 12,
          fontFamily: "var(--font-mono)",
          fontSize: 11,
          color: "var(--txt2)",
        }}
      >
        <div style={{ display: "flex", gap: 8, marginBottom: 8 }}>
          <strong style={{ flex: 1 }}>demo seeder</strong>
          <button type="button" className="btn-solid" disabled={running} onClick={() => void run()}>
            {running ? "Seeding…" : "Seed all demos"}
          </button>
          {running && (
            <button type="button" className="btn-ghost" onClick={() => (stopRef.current = true)}>
              Stop
            </button>
          )}
        </div>
        <div style={{ maxHeight: 260, overflowY: "auto", whiteSpace: "pre-wrap" }}>
          {lines.join("\n") || "publishes every bundled demo as the signed-in account"}
        </div>
      </div>
    </div>
  );
}
