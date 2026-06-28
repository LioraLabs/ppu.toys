const SAMPLE = `-- ppu.toys :: dusk-parallax
local SPEED = 12

function frame(t, f)
  ppu.brightness = 15
  ppu.mode = 1
  ppu.bg[1].scroll.x = t * SPEED
end`;

export function EditorPane() {
  return (
    <section className="editor">
      <div className="editor-tabs">
        <div className="etab etab--active">
          <span className="etab-dot" />
          main.lua
        </div>
        <div className="etab">mode7.lua</div>
        <div className="etab-spacer" />
        <div className="etab-status">Lua 5.4 · ok</div>
      </div>
      {/* CodeMirror 6 mounts here (U3). Static placeholder for now. */}
      <div className="editor-body" data-editor-slot>
        <pre className="editor-placeholder">{SAMPLE}</pre>
      </div>
    </section>
  );
}
