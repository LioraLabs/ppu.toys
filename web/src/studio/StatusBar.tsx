export function StatusBar() {
  return (
    <footer className="statusbar">
      <span className="sb-item">
        <span className="sb-dot" />
        lua
      </span>
      <span className="sb-item">ln 9, col 24</span>
      <span className="sb-item">spaces: 2</span>
      <span className="tb-spacer" />
      <span className="sb-item">60 fps</span>
      <span className="sb-item">256×224</span>
      <span className="sb-item sb-item--dim">wasm-ppu v0.3.1</span>
    </footer>
  );
}
