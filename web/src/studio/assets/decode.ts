import type { DecodedImage } from "./assetStore";

/** Decode a PNG File to ImageData plus a preview data URL, using browser image
 *  decoding + a 2D canvas. Throws if the file cannot be decoded. */
export async function decodeImageFile(file: File): Promise<DecodedImage> {
  const bitmap = await createImageBitmap(file);
  try {
    const canvas = document.createElement("canvas");
    canvas.width = bitmap.width;
    canvas.height = bitmap.height;
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("2D canvas context unavailable");
    ctx.drawImage(bitmap, 0, 0);
    const imageData = ctx.getImageData(0, 0, bitmap.width, bitmap.height);
    return { name: file.name, imageData, preview: canvas.toDataURL("image/png") };
  } finally {
    bitmap.close();
  }
}

/** Keep only files that look like PNGs. */
export function pngFiles(files: Iterable<File>): File[] {
  return Array.from(files).filter((f) => f.type === "image/png" || /\.png$/i.test(f.name));
}
