import type { DecodedImage } from "./assetStore";

/** Decode a PNG blob to ImageData plus a preview data URL, using browser image
 *  decoding + a 2D canvas. Throws if the blob cannot be decoded. */
export async function decodeImageBlob(blob: Blob, name: string): Promise<DecodedImage> {
  const bitmap = await createImageBitmap(blob);
  try {
    const canvas = document.createElement("canvas");
    canvas.width = bitmap.width;
    canvas.height = bitmap.height;
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("2D canvas context unavailable");
    ctx.drawImage(bitmap, 0, 0);
    const imageData = ctx.getImageData(0, 0, bitmap.width, bitmap.height);
    return { name, imageData, preview: canvas.toDataURL("image/png") };
  } finally {
    bitmap.close();
  }
}

/** Decode a PNG File (drag-drop upload path). */
export async function decodeImageFile(file: File): Promise<DecodedImage> {
  return decodeImageBlob(file, file.name);
}

/** Keep only files that look like PNGs. */
export function pngFiles(files: Iterable<File>): File[] {
  return Array.from(files).filter((f) => f.type === "image/png" || /\.png$/i.test(f.name));
}
