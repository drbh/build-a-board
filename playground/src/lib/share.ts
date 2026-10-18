// The address bar is the share link: which example, and (once edited) the
// board's source itself, deflated and base64url-encoded in `?src=`, so a board
// someone is looking at can be handed to someone else as a URL.

async function pipe(bytes: Uint8Array<ArrayBuffer>, stream: CompressionStream | DecompressionStream): Promise<Uint8Array> {
  const out = new Blob([bytes]).stream().pipeThrough(stream);
  return new Uint8Array(await new Response(out).arrayBuffer());
}

export async function encodeSource(text: string): Promise<string> {
  const bytes = await pipe(new TextEncoder().encode(text), new CompressionStream("deflate-raw"));
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

/// Null for anything that is not a source this encoded.
export async function decodeSource(encoded: string): Promise<string | null> {
  try {
    const binary = atob(encoded.replace(/-/g, "+").replace(/_/g, "/"));
    const bytes = await pipe(Uint8Array.from(binary, (c) => c.charCodeAt(0)), new DecompressionStream("deflate-raw"));
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    return null;
  }
}

export interface Shared {
  example: string | null;
  /// The tour's step, from 0.
  step: number | null;
  source: string | null;
}

export async function readShare(search: string): Promise<Shared> {
  const params = new URLSearchParams(search);
  const src = params.get("src");
  const step = Number(params.get("step"));
  return {
    example: params.get("example"),
    step: Number.isInteger(step) && step >= 1 ? step - 1 : null,
    source: src ? await decodeSource(src) : null,
  };
}

/// The query string for the current state, keeping any other parameters
/// (`?selftest`). The source is carried only when it differs from the example
/// it started as, which keeps an unedited link short.
export async function shareQuery(
  search: string,
  state: { example: string; step?: number; source: string; original: string },
): Promise<string> {
  const params = new URLSearchParams(search);
  params.set("example", state.example);
  if (state.step !== undefined) params.set("step", String(state.step + 1));
  else params.delete("step");
  if (state.source !== state.original) params.set("src", await encodeSource(state.source));
  else params.delete("src");
  return `?${params}`;
}
