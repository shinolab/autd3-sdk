import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { sidebar } from "../src/sidebar.mjs";

const slug = process.argv[2];
if (!slug) {
  console.error("usage: node scripts/version-config.mjs <slug>");
  process.exit(1);
}

const DOC_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const dir = join(DOC_ROOT, "src", "content", "versions");
mkdirSync(dir, { recursive: true });
const file = join(dir, `${slug}.json`);

// Byte-for-byte what starlight-versions' `makeVersionConfig` would write, so a
// later `astro build` regenerating it leaves no diff.
writeFileSync(file, JSON.stringify({ sidebar }, null, 2));
console.log(`version-config: wrote ${file}`);
