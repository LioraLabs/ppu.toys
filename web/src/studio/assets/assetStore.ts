/** A user-uploaded image. `id` is the string referenced from Lua as
 *  bg[n].source / obj.sheet. */
export interface Asset {
  id: string;
  name: string;
  width: number;
  height: number;
  preview: string; // data URL thumbnail
}

/** A decoded image ready to register. */
export interface DecodedImage {
  name: string;
  imageData: ImageData;
  preview: string;
}

/** Slugify a filename into a Lua-safe asset id, deduping against taken ids. */
export function assetId(filename: string, taken: Iterable<string>): string {
  const base =
    filename
      .replace(/\.[^.]+$/, "")
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "_")
      .replace(/^_+|_+$/g, "") || "asset";
  const used = new Set(taken);
  if (!used.has(base)) return base;
  let n = 2;
  while (used.has(`${base}_${n}`)) n++;
  return `${base}_${n}`;
}
