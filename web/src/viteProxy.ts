export function shouldBypassApiProxy(url: string): boolean {
  return /\.(?:[cm]?[jt]sx?|css|map|wasm)(?:\?|$)/.test(url);
}
