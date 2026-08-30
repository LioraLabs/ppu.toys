import "./avatar.css";

/** Discord CDN avatar URL, or null when the user has no custom avatar. */
export function discordAvatarUrl(id: string, hash: string | null, size = 64): string | null {
  return hash ? `https://cdn.discordapp.com/avatars/${id}/${hash}.png?size=${size}` : null;
}

/** Deterministic hue from a handle, for the letter-tile fallback. */
export function hueOf(handle: string): number {
  let h = 0;
  for (const c of handle) h = (h * 31 + c.charCodeAt(0)) % 360;
  return h;
}

export interface AvatarProps {
  handle: string;
  /** Discord user id — needed to build the CDN URL. */
  id?: string | null;
  /** Discord avatar hash (null = default avatar → letter tile). */
  avatar?: string | null;
  /** Rendered box in px (default 24). */
  size?: number;
}

/** User avatar: Discord image when id+hash are known, letter tile otherwise.
 *  Pure presentational — pages pass whatever identity fields their API
 *  payload carries. */
export function Avatar({ handle, id, avatar, size = 24 }: AvatarProps) {
  const url = id ? discordAvatarUrl(id, avatar ?? null, size <= 32 ? 64 : 128) : null;
  const style = { width: size, height: size, fontSize: Math.round(size * 0.42) };
  if (url) {
    return <img className="avatar-img" style={style} src={url} alt="" loading="lazy" />;
  }
  return (
    <span
      className="avatar-tile"
      style={{ ...style, background: `hsl(${hueOf(handle)} 42% 38%)` }}
      aria-hidden="true"
    >
      {handle.slice(0, 1).toUpperCase()}
    </span>
  );
}
