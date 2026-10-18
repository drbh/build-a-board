// Render the book (../book/**/*.md) to HTML under dist/book/, for the hosted
// site. Links between book pages become links between the HTML pages; links
// to anything else in the repository (examples, the SDK, wit/) go to GitHub.
import { mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, extname, join, posix, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { Marked } from "marked";

const here = fileURLToPath(new URL("..", import.meta.url));
const repo = join(here, "..");
const book = join(repo, "book");
const out = join(here, "dist", "book");
const github = "https://github.com/drbh/build-a-board";

function pages(dir) {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return pages(path);
    return extname(name) === ".md" ? [relative(book, path).split("\\").join("/")] : [];
  });
}

/// `page.md` -> `page.html`, `README.md` -> `index.html`.
const html = (page) => page.replace(/(^|\/)README\.md$/, "$1index.html").replace(/\.md$/, ".html");

/// Where a link written on `page` should point on the site.
function rewrite(page, href) {
  if (/^[a-z]+:|^#|^\//i.test(href)) return href;
  const [path, hash = ""] = href.split("#");
  const target = posix.normalize(posix.join(posix.dirname(page), path));
  if (!target.startsWith("../") && target.endsWith(".md")) {
    return posix.relative(posix.dirname(page), html(target)) + (hash && `#${hash}`);
  }
  const file = posix.normalize(posix.join("book", target));
  const kind = extname(file) ? "blob" : "tree";
  return `${github}/${kind}/main/${file}${hash && `#${hash}`}`;
}

function render(page) {
  const source = readFileSync(join(book, page), "utf8");
  const marked = new Marked({
    walkTokens(token) {
      if (token.type === "link") token.href = rewrite(page, token.href);
    },
  });
  const body = marked.parse(source);
  const title = source.match(/^#\s+(.+)$/m)?.[1] ?? "bb";
  const root = posix.relative(posix.dirname(page), ".") || ".";
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${title} · bb</title>
<link rel="stylesheet" href="${root}/book.css">
</head>
<body>
<nav><a href="${root}/../">playground</a><a href="${root}/">book</a><a href="${github}">github</a></nav>
<main>
${body}</main>
</body>
</html>
`;
}

const css = `:root { color-scheme: light dark; --fg: #1d1d1f; --muted: #6e6e73; --bg: #fff; --code: #f3f3f5; --line: #e2e2e6; --link: #0b63c4; }
@media (prefers-color-scheme: dark) { :root { --fg: #e8e8ea; --muted: #9a9aa0; --bg: #16161a; --code: #222228; --line: #303038; --link: #6aa9ff; } }
body { margin: 0; background: var(--bg); color: var(--fg); font: 16px/1.6 system-ui, sans-serif; }
nav { display: flex; gap: 18px; padding: 12px 16px; border-bottom: 1px solid var(--line); font-size: 14px; }
main { max-width: 760px; margin: 0 auto; padding: 16px 16px 64px; }
a { color: var(--link); text-decoration: none; }
a:hover { text-decoration: underline; }
nav a { color: var(--muted); }
h1, h2, h3 { line-height: 1.25; margin: 1.6em 0 0.5em; }
h1 { margin-top: 0.6em; }
code { font: 0.9em ui-monospace, SFMono-Regular, Menlo, monospace; background: var(--code); padding: 0.1em 0.3em; border-radius: 4px; }
pre { background: var(--code); padding: 12px 14px; border-radius: 6px; overflow-x: auto; }
pre code { padding: 0; background: none; }
table { border-collapse: collapse; display: block; overflow-x: auto; }
th, td { border: 1px solid var(--line); padding: 6px 10px; text-align: left; vertical-align: top; }
`;

for (const page of pages(book)) {
  const file = join(out, html(page));
  mkdirSync(dirname(file), { recursive: true });
  writeFileSync(file, render(page));
}
writeFileSync(join(out, "book.css"), css);
console.log(`Book rendered to ${relative(repo, out)}`);
