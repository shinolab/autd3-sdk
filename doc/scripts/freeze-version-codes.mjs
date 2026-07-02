import { readFileSync, writeFileSync, readdirSync, statSync } from "node:fs";
import { join, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const DOC_ROOT = resolve(__dirname, "..");
const CODES_BASE = join(DOC_ROOT, "codes");

function escapeRe(s) {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
function dedent(lines) {
  const indents = lines
    .filter((l) => l.trim().length > 0)
    .map((l) => l.match(/^[ \t]*/)[0].length);
  const min = indents.length ? Math.min(...indents) : 0;
  return lines.map((l) => l.slice(min));
}
const anchorRe = /^\s*\/\/\s*ANCHOR(_END)?\s*:/;
const hideStartRe = /^\s*\/\/\s*HIDE\b/;
const hideEndRe = /^\s*\/\/\s*HIDE_END\b/;
const hideLineRe = /\/\/\s*\[hide\]\s*$/;
function clean(lines) {
  const out = [];
  let hiding = false;
  for (const l of lines) {
    if (hideEndRe.test(l)) {
      hiding = false;
      continue;
    }
    if (hideStartRe.test(l)) {
      hiding = true;
      continue;
    }
    if (hiding) continue;
    if (anchorRe.test(l)) continue;
    if (hideLineRe.test(l)) continue;
    out.push(l);
  }
  return dedent(out).join("\n").replace(/^\n+|\n+$/g, "");
}
function collectRegions(lines, rawName) {
  const name = escapeRe(rawName);
  const startRe = new RegExp(`^\\s*//\\s*ANCHOR\\s*:\\s*${name}\\s*$`);
  const endRe = new RegExp(`^\\s*//\\s*ANCHOR_END\\s*:\\s*${name}\\s*$`);
  const regions = [];
  let i = 0;
  while (i < lines.length) {
    if (startRe.test(lines[i])) {
      let j = i + 1;
      while (j < lines.length && !endRe.test(lines[j])) j++;
      if (j < lines.length) {
        regions.push(lines.slice(i + 1, j));
        i = j + 1;
        continue;
      }
    }
    i++;
  }
  return regions;
}
function excerpt(raw, anchor) {
  const lines = raw.split("\n");
  let regions;
  if (anchor) {
    const names = Array.isArray(anchor) ? anchor : [anchor];
    regions = names.flatMap((n) => collectRegions(lines, n));
  } else {
    regions = [lines];
  }
  return regions
    .map((r) => clean(r))
    .filter((s) => s.length > 0)
    .join("\n\n");
}

function walk(dir, out) {
  for (const entry of readdirSync(dir)) {
    const p = join(dir, entry);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (p.endsWith(".mdx") || p.endsWith(".md")) out.push(p);
  }
}

function parseAnchorArg(argRaw) {
  if (argRaw === undefined) return undefined;
  const a = argRaw.trim();
  if (a.startsWith("[")) {
    return a
      .slice(1, -1)
      .split(",")
      .map((s) => s.trim().replace(/^["']|["']$/g, ""))
      .filter(Boolean);
  }
  return a.replace(/^["']|["']$/g, "");
}

function processFile(path) {
  let text = readFileSync(path, "utf8");

  const importRe = /^import\s+(\w+)\s+from\s+"@codes\/([^"?]+)\?raw";[ \t]*\n/gm;
  const imports = {};
  let m;
  while ((m = importRe.exec(text))) {
    const [, name, rel] = m;
    imports[name] = readFileSync(join(CODES_BASE, rel), "utf8");
  }
  const names = Object.keys(imports);
  if (names.length === 0) return false;

  // 2a. rust={excerpt(NAME, ...)} -> rust={"...inlined..."}
  text = text.replace(
    /=\{\s*excerpt\(\s*(\w+)\s*(?:,\s*([^)]+?))?\s*\)\s*\}/g,
    (full, name, argRaw) => {
      if (!(name in imports)) return full;
      const code = excerpt(imports[name], parseAnchorArg(argRaw));
      return `={${JSON.stringify(code)}}`;
    },
  );

  text = text.replace(/=\{\s*(\w+)\s*\}/g, (full, name) => {
    if (!(name in imports)) return full;
    return `={${JSON.stringify(imports[name])}}`;
  });

  for (const name of names) {
    const re = new RegExp(
      `^import\\s+${name}\\s+from\\s+"@codes/[^"?]+\\?raw";[ \\t]*\\n`,
      "m",
    );
    text = text.replace(re, "");
  }
  if (!/\bexcerpt\(/.test(text)) {
    text = text.replace(
      /^import\s+\{\s*excerpt\s*\}\s+from\s+"@lib\/excerpt";[ \t]*\n/m,
      "",
    );
  }

  writeFileSync(path, text);
  return true;
}

const slug = process.argv[2];
if (!slug) {
  console.error("usage: node scripts/freeze-version-codes.mjs <slug>");
  process.exit(1);
}
const versionDir = join(DOC_ROOT, "src", "content", "docs", slug);
const files = [];
walk(versionDir, files);
let changed = 0;
for (const f of files) {
  if (processFile(f)) changed++;
}
console.log(`freeze-version-codes: inlined codes in ${changed} file(s) under ${slug}/`);
