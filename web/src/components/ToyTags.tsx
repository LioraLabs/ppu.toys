import { Link } from "react-router-dom";
import "./tags.css";

export function ToyTags({ tags = [] }: { tags?: string[] }) {
  if (!tags.length) return null;
  return (
    <div className="toy-tags" aria-label="Tags">
      {tags.map((tag) => (
        <Link key={tag} to={`/browse?tag=${encodeURIComponent(tag)}`} className="toy-tag">
          #{tag}
        </Link>
      ))}
    </div>
  );
}
