export interface ExcerptOptions {
  anchor?: string | string[];
}

function escapeRe(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function dedent(lines: string[]): string[] {
  const indents = lines
    .filter((l) => l.trim().length > 0)
    .map((l) => l.match(/^[ \t]*/)![0].length);
  const min = indents.length ? Math.min(...indents) : 0;
  return lines.map((l) => l.slice(min));
}

const anchorRe = /^\s*(?:\/\/|#)\s*ANCHOR(_END)?\s*:/;
const hideStartRe = /^\s*(?:\/\/|#)\s*HIDE\b/;
const hideEndRe = /^\s*(?:\/\/|#)\s*HIDE_END\b/;
const hideLineRe = /(?:\/\/|#)\s*\[hide\]\s*$/;

function clean(lines: string[]): string {
  const out: string[] = [];
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

function collectRegions(lines: string[], rawName: string): string[][] {
  const name = escapeRe(rawName);
  const startRe = new RegExp(`^\\s*(?://|#)\\s*ANCHOR\\s*:\\s*${name}\\s*$`);
  const endRe = new RegExp(`^\\s*(?://|#)\\s*ANCHOR_END\\s*:\\s*${name}\\s*$`);
  const regions: string[][] = [];
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

export function excerpt(
  raw: string,
  options: string | string[] | ExcerptOptions = {},
): string {
  const opts: ExcerptOptions =
    typeof options === "string" || Array.isArray(options)
      ? { anchor: options }
      : options;
  const lines = raw.split("\n");

  let regions: string[][];
  if (opts.anchor) {
    const names = Array.isArray(opts.anchor) ? opts.anchor : [opts.anchor];
    regions = names.flatMap((name) => collectRegions(lines, name));
  } else {
    regions = [lines];
  }

  return regions
    .map((region) => clean(region))
    .filter((s) => s.length > 0)
    .join("\n\n");
}
