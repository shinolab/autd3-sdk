import { getCollection } from "astro:content";
import versionsConfig from "virtual:starlight-versions-config";

type Version = (typeof versionsConfig.versions)[number];

export function pageVersion(
  id: string,
  locale: string | undefined,
): Version | undefined {
  const segments = id.split("/");
  const first = locale && segments[0] === locale ? segments[1] : segments[0];
  return first ? versionsConfig.versionsBySlug[first] : undefined;
}

export function latestVersion(): Version {
  const latest = versionsConfig.versions[0];
  if (!latest) throw new Error("no frozen documentation version is declared");
  return latest;
}

let routePaths: Set<string> | undefined;

function routePath(id: string): string {
  const segments = id.split("/").filter((segment) => segment.length > 0);
  if (segments.at(-1) === "index") segments.pop();
  return segments.join("/");
}

async function knownRoutePaths(): Promise<Set<string>> {
  routePaths ??= new Set(
    (await getCollection("docs")).map((entry) => routePath(entry.id)),
  );
  return routePaths;
}

function versionedPath(
  id: string,
  locale: string | undefined,
  slug: string | undefined,
): string {
  const segments = routePath(id)
    .split("/")
    .filter((s) => s.length > 0);
  const localized = locale !== undefined && segments[0] === locale;
  const rest = localized ? segments.slice(1) : segments;
  const body =
    rest[0] && rest[0] in versionsConfig.versionsBySlug ? rest.slice(1) : rest;
  return [
    ...(localized ? [locale] : []),
    ...(slug ? [slug] : []),
    ...body,
  ].join("/");
}

export async function versionHref(
  version: Version | undefined,
  id: string,
  locale: string | undefined,
): Promise<string> {
  const base = import.meta.env.BASE_URL.replace(/\/$/, "");
  const root = [
    ...(locale ? [locale] : []),
    ...(version ? [version.slug] : []),
  ].join("/");
  const target = versionedPath(id, locale, version?.slug);
  const path = (await knownRoutePaths()).has(target) ? target : root;
  return path.length > 0 ? `${base}/${path}/` : `${base}/`;
}
