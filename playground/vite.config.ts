// The playground app. Besides the page, the dev and preview servers serve
// KiCad's footprint libraries as `footprints/<Library>.pretty/<Name>.kicad_mod`
// (KICAD_FOOTPRINT_DIR overrides where they are found) and take the
// `?selftest` session's report at POST /selftest.
import { appendFileSync, createReadStream, existsSync, statSync } from "node:fs";
import { join, normalize, sep } from "node:path";
import { fileURLToPath } from "node:url";
import type { Connect, Plugin } from "vite";
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

const here = fileURLToPath(new URL(".", import.meta.url));
const cache = join(here, ".cache");

const footprints = [
  process.env.KICAD_FOOTPRINT_DIR,
  "/Applications/KiCad/KiCad.app/Contents/SharedSupport/footprints",
  "/usr/share/kicad/footprints",
].find((dir) => dir && existsSync(dir));

/// `root` joined with a URL path, or null if the path escapes it.
function within(root: string, path: string): string | null {
  const full = normalize(join(root, decodeURIComponent(path)));
  return full.startsWith(root.endsWith(sep) ? root : root + sep) ? full : null;
}

const serveLibraries: Connect.NextHandleFunction = (req, res, next) => {
  const path = (req.url ?? "/").split("?")[0];
  if (req.method === "POST" && path === "/selftest") {
    let body = "";
    req.on("data", (chunk) => (body += chunk));
    req.on("end", () => {
      appendFileSync(join(cache, "selftest.jsonl"), body + "\n");
      res.end("ok");
    });
    return;
  }
  if (!path.startsWith("/footprints/")) return next();
  const file = footprints && within(footprints, path.slice("/footprints/".length));
  if (!file || !existsSync(file) || !statSync(file).isFile()) {
    res.statusCode = 404;
    return res.end("not found");
  }
  res.setHeader("Content-Type", "text/plain; charset=utf-8");
  createReadStream(file).pipe(res);
};

function libraries(): Plugin {
  return {
    name: "bb-footprints",
    configureServer(server) {
      if (!footprints) server.config.logger.warn("no KiCad footprint libraries found; set KICAD_FOOTPRINT_DIR");
      server.middlewares.use(serveLibraries);
    },
    configurePreviewServer(server) {
      server.middlewares.use(serveLibraries);
    },
  };
}

/// Fail early, and say how to fix it, when tools/assets.sh has not run.
function assets(): Plugin {
  return {
    name: "bb-assets",
    buildStart() {
      for (const file of ["core/bb_core_wasm.js", "public/compiler/rustc.wasm", "public/compiler/libbb_sdk.rlib"]) {
        if (!existsSync(join(cache, file))) {
          this.error(`.cache/${file} is missing; run \`npm run assets\` (tools/assets.sh) first`);
        }
      }
    },
  };
}

export default defineConfig({
  plugins: [assets(), svelte(), libraries()],
  // The compiler, its standard library, and the prebuilt SDK (tools/assets.sh).
  publicDir: join(cache, "public"),
  resolve: { alias: { $core: join(cache, "core/bb_core_wasm.js") } },
  // Relative URLs, so dist/ works from any path.
  base: "./",
  build: { target: "esnext", chunkSizeWarningLimit: 2000 },
  worker: { format: "es" },
  // The example boards are read straight from ../examples.
  server: { port: 7820, strictPort: true, fs: { allow: [join(here, "..")] } },
  preview: { port: 7820, strictPort: true },
});
