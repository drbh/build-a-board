// Static assets answer every request they have a file for (the playground, the
// book, footprints, the small compiler files). The worker only sees the rest:
// the compiler files too large for static assets, which live in R2 under their
// sha256, named by `compiler/r2.json`.

const types = { ".wasm": "application/wasm" };

export default {
  async fetch(request, env) {
    if (request.method !== "GET" && request.method !== "HEAD") {
      return new Response("method not allowed", { status: 405 });
    }
    const url = new URL(request.url);
    const name = url.pathname.match(/^\/compiler\/([^/]+)$/)?.[1];
    if (!name) return new Response("not found", { status: 404 });

    // `/compiler/<sha256>` answers HEAD by key, so a deploy can skip uploads R2 already has.
    let key = /^[0-9a-f]{64}$/.test(name) ? name : null;
    if (!key) {
      const manifest = await env.ASSETS.fetch(new URL("/compiler/r2.json", url));
      if (!manifest.ok) return new Response("not found", { status: 404 });
      key = (await manifest.json())[name];
      if (!key) return new Response("not found", { status: 404 });
    }

    const object =
      request.method === "HEAD" ? await env.COMPILER.head(key) : await env.COMPILER.get(key, { onlyIf: request.headers });
    if (!object) return new Response("not found", { status: 404 });

    const headers = new Headers();
    object.writeHttpMetadata(headers);
    headers.set("etag", object.httpEtag);
    headers.set("content-type", types[name.slice(name.lastIndexOf("."))] ?? "application/octet-stream");
    // The names are stable across compiler updates, so revalidate (cheap: a 304) rather than cache forever.
    headers.set("cache-control", "public, max-age=3600, must-revalidate");
    const body = request.method === "GET" && "body" in object ? object.body : null;
    const status = request.method === "GET" && !body ? 304 : 200;
    return new Response(body, { status, headers });
  },
};
