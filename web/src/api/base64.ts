/** Decode a base64 string (the API's `payload` field) to bytes for
 *  transport.addSource. Uses the platform atob — no dependency. */
export function decodeBase64(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}
