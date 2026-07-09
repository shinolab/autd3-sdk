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

export function versionHref(
  version: Version,
  locale: string | undefined,
): string {
  const base = import.meta.env.BASE_URL.replace(/\/$/, "");
  return `${base}${locale ? `/${locale}` : ""}/${version.slug}/`;
}
