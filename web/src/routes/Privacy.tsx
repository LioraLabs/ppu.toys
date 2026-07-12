export function Privacy() {
  return (
    <article className="doc-page">
      <h1>Privacy</h1>
      <p>
        Sign-in uses Discord with the <code>identify</code> scope only. We store
        your Discord id, username, and avatar hash — no email, no message access.
      </p>
      <p>
        We keep a session cookie so you stay signed in, and the toys you publish.
        We don’t sell data or run third-party ad trackers.
      </p>
      <p>
        To delete your account and toys, email
        <a href="mailto:takedown@ppu.toys"> takedown@ppu.toys</a>.
      </p>
    </article>
  );
}
