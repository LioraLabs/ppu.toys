import { WIDTH, HEIGHT } from "../../ppu/core";
import { transport } from "../transport/transport";

export interface RecordedClip { clip: Blob; thumb: Blob }
export interface RecordOptions { durationMs?: number; fps?: number }

export function isRecordingSupported(): boolean {
  return typeof globalThis.MediaRecorder !== "undefined" &&
    typeof HTMLCanvasElement !== "undefined" &&
    typeof HTMLCanvasElement.prototype.captureStream === "function";
}

/** Record the live loop: paint each transport frame into an offscreen 256x224
 *  2D canvas, capture it as a WebM stream, and grab one frame as a PNG thumb.
 *  Rewinds to t=0 so the clip starts at the loop head. */
export async function recordClip(opts: RecordOptions = {}): Promise<RecordedClip> {
  if (!isRecordingSupported()) throw new Error("Loop recording isn't supported in this browser.");
  const durationMs = opts.durationMs ?? 5000;
  const fps = opts.fps ?? 30;
  const canvas = document.createElement("canvas");
  canvas.width = WIDTH; canvas.height = HEIGHT;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("Loop recording needs a 2D canvas.");

  const paint = () => {
    const fb = transport.getSnapshot().frame.framebuffer;
    ctx.putImageData(new ImageData(new Uint8ClampedArray(fb), WIDTH, HEIGHT), 0, 0);
  };

  transport.restart();          // rewind clock to t=0, resume playback
  paint();
  const thumb = await new Promise<Blob>((res, rej) =>
    canvas.toBlob((b) => (b ? res(b) : rej(new Error("thumb encode failed"))), "image/png"));

  const stream = canvas.captureStream(fps);
  const mime = MediaRecorder.isTypeSupported("video/webm;codecs=vp9") ? "video/webm;codecs=vp9" : "video/webm";
  const rec = new MediaRecorder(stream, { mimeType: mime, videoBitsPerSecond: 1_000_000 });
  const chunks: BlobPart[] = [];
  rec.ondataavailable = (e) => { if (e.data.size) chunks.push(e.data); };

  const unsub = transport.subscribe(paint);   // repaint as the shared loop advances
  const done = new Promise<Blob>((resolve) => {
    rec.onstop = () => resolve(new Blob(chunks, { type: "video/webm" }));
  });
  rec.start();
  await new Promise((r) => setTimeout(r, durationMs));
  rec.stop();
  unsub();
  const clip = await done;
  return { clip, thumb };
}
