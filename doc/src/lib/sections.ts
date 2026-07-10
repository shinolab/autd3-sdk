import versionsConfig from "virtual:starlight-versions-config";

export type SectionKey = "users" | "developer" | "theory";

export interface Section {
  key: SectionKey;
  segment: string;
  label: string;
  labelEn: string;
}

export const SECTIONS: Section[] = [
  { key: "users", segment: "", label: "ユーザーズマニュアル", labelEn: "User's Manual" },
  {
    key: "developer",
    segment: "developer",
    label: "デベロッパーズマニュアル",
    labelEn: "Developer's Manual",
  },
  { key: "theory", segment: "theory", label: "理論と考察", labelEn: "Theory" },
];

const base = import.meta.env.BASE_URL.replace(/\/$/, "");

function toSection(segment: string | undefined): SectionKey {
  return segment === "developer" || segment === "theory" ? segment : "users";
}

function leadingSegment(segments: string[], locale: string | undefined): string | undefined {
  let i = 0;
  if (locale && segments[i] === locale) i++;
  const slug = segments[i];
  if (slug && versionsConfig.versionsBySlug[slug]) i++;
  return segments[i];
}

export function sectionOfId(id: string, locale: string | undefined): SectionKey {
  return toSection(leadingSegment(id.split("/").filter(Boolean), locale));
}

export function sectionOfHref(href: string, locale: string | undefined): SectionKey | undefined {
  if (href !== `${base}/` && !href.startsWith(`${base}/`)) return undefined;
  return toSection(leadingSegment(href.slice(base.length).split("/").filter(Boolean), locale));
}

export function sectionHref(
  section: Section,
  locale: string | undefined,
  versionSlug: string | undefined,
): string {
  const segments = [locale, versionSlug, section.segment].filter(Boolean);
  return segments.length === 0 ? `${base}/` : `${base}/${segments.join("/")}/`;
}
