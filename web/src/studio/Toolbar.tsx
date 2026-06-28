export function Toolbar() {
  return (
    <header className="toolbar">
      <div className="logo-mark">p</div>
      <div className="wordmark">
        ppu<span className="dot">.</span>toys
      </div>
      <div className="tb-divider" />
      <div className="project">
        <span className="project-name">mode7-floor</span>
        <span className="saved-dot" />
      </div>
      <div className="tb-spacer" />
      <div className="social">
        <span>★ 940</span>
        <span>⑂ 51</span>
      </div>
      <button className="btn-ghost">Fork</button>
      <button className="btn-solid">Publish</button>
      <div className="avatar" />
    </header>
  );
}
