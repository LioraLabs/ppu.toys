/** Decode a base64 string (the API's `payload` field) to bytes for
 *  transport.addSource. Uses the platform atob — no dependency. */
export function decodeBase64(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

/** Encode bytes to base64 for the API `payload` field. Chunked so a large
 *  payload can't overflow the call stack via String.fromCharCode(...spread). */
export function encodeBase64(bytes: Uint8Array): string {
  let bin = "";
  const CHUNK = 0x8000;
  for (let i = 0; i < bytes.length; i += CHUNK) {
    bin += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
  }
  return btoa(bin);
}
