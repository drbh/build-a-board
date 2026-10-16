// The example boards, read from ../examples at build time. Only single-file
// boards are offered: the playground compiles one file, so a crate with
// `mod name;` modules (balance-bot) cannot load yet.
const sources = import.meta.glob("../../../examples/*/src/lib.rs", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

const outOfLine = /^\s*(pub(\([^)]*\))?\s+)?mod\s+\w+\s*;/m;

export const examples: { name: string; source: string }[] = Object.entries(sources)
  .filter(([, source]) => !outOfLine.test(source))
  .map(([path, source]) => ({ name: path.split("/").at(-3)!, source }))
  .sort((a, b) => a.name.localeCompare(b.name));
