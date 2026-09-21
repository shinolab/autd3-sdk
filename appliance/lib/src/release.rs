use std::io::Read;
use std::time::Duration;

use crate::{CHECKSUM_SUFFIX, RELEASE_TAG_PREFIX, SERVER_TARGET, server_asset_name, sha256_hex};

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

const RELEASES_API: &str = "https://api.github.com/repos/shinolab/autd3-sdk/releases";
const USER_AGENT: &str = concat!("autd3-appliance/", env!("CARGO_PKG_VERSION"));
const MAX_BINARY_BYTES: u64 = 64 * 1024 * 1024;
const PER_PAGE: usize = 100;
const MAX_PAGES: usize = 5;
const NOT_FOUND: u16 = 404;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServerRelease {
    pub version: String,
    pub tag: String,
    pub url: String,
    pub checksum_url: String,
    pub size: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum ReleaseError {
    #[error("fetching {action}")]
    Http {
        action: String,
        #[source]
        source: Box<ureq::Error>,
    },
    #[error("reading {action}")]
    Read {
        action: String,
        #[source]
        source: std::io::Error,
    },
    #[error("parsing the GitHub releases response")]
    Parse(#[source] serde_json::Error),
    #[error("the GitHub releases response was not a JSON array")]
    NotAnArray,
    #[error("{RELEASE_TAG_PREFIX}{version} publishes no server binary for {SERVER_TARGET}")]
    NoVersion { version: String },
    #[error("the published checksum of {name} reads `{text}`, which is not a SHA-256")]
    MalformedChecksum { name: String, text: String },
    #[error(
        "{name} hashes to {actual}, not the published {expected}; the download was truncated, or \
         the checksum does not belong to it"
    )]
    Checksum {
        name: String,
        actual: String,
        expected: String,
    },
}

pub fn list(timeout: Duration) -> Result<Vec<ServerRelease>, ReleaseError> {
    let mut found = Vec::new();
    for page in 1..=MAX_PAGES {
        let body = get(
            &format!("{RELEASES_API}?per_page={PER_PAGE}&page={page}"),
            "application/vnd.github+json",
            "the GitHub release list",
            timeout,
        )?;
        let (mut listed, entries) = page_releases(&String::from_utf8_lossy(&body))?;
        found.append(&mut listed);
        if entries < PER_PAGE {
            break;
        }
    }
    Ok(newest_first(found))
}

pub fn find(version: &str, timeout: Duration) -> Result<ServerRelease, ReleaseError> {
    let version = normalize_version(version);
    let tag = format!("{RELEASE_TAG_PREFIX}{version}");
    let body = match get(
        &format!("{RELEASES_API}/tags/{tag}"),
        "application/vnd.github+json",
        &format!("the {tag} release"),
        timeout,
    ) {
        Ok(body) => body,
        Err(ReleaseError::Http { source, .. })
            if matches!(*source, ureq::Error::StatusCode(NOT_FOUND)) =>
        {
            return Err(ReleaseError::NoVersion { version });
        }
        Err(e) => return Err(e),
    };
    let release: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&body)).map_err(ReleaseError::Parse)?;
    server_release(&release).ok_or(ReleaseError::NoVersion { version })
}

pub fn download(release: &ServerRelease, timeout: Duration) -> Result<Vec<u8>, ReleaseError> {
    let name = server_asset_name(&release.version);
    let binary = get(&release.url, "application/octet-stream", &name, timeout)?;
    let checksum = get(
        &release.checksum_url,
        "text/plain",
        &format!("{name}{CHECKSUM_SUFFIX}"),
        timeout,
    )?;
    verify(&name, &binary, &String::from_utf8_lossy(&checksum))?;
    Ok(binary)
}

#[must_use]
pub fn normalize_version(version: &str) -> String {
    let version = version.trim();
    let version = version.strip_prefix(RELEASE_TAG_PREFIX).unwrap_or(version);
    version.strip_prefix('v').unwrap_or(version).to_owned()
}

#[must_use]
pub fn version_key(version: &str) -> (u64, u64, u64) {
    let mut parts = version
        .trim_start_matches('v')
        .split(['.', '-', '+'])
        .filter_map(|part| part.parse::<u64>().ok());
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

#[must_use]
pub fn is_newer(candidate: &str, running: &str) -> bool {
    version_key(candidate) > version_key(running)
}

fn get(url: &str, accept: &str, action: &str, timeout: Duration) -> Result<Vec<u8>, ReleaseError> {
    let config = ureq::config::Config::builder()
        .timeout_global(Some(timeout))
        .build();
    let response = ureq::Agent::new_with_config(config)
        .get(url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", accept)
        .call()
        .map_err(|source| ReleaseError::Http {
            action: action.to_owned(),
            source: Box::new(source),
        })?;
    let mut bytes = Vec::new();
    response
        .into_body()
        .into_reader()
        .take(MAX_BINARY_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|source| ReleaseError::Read {
            action: action.to_owned(),
            source,
        })?;
    Ok(bytes)
}

fn page_releases(body: &str) -> Result<(Vec<ServerRelease>, usize), ReleaseError> {
    let releases: serde_json::Value = serde_json::from_str(body).map_err(ReleaseError::Parse)?;
    let releases = releases.as_array().ok_or(ReleaseError::NotAnArray)?;

    let found = releases
        .iter()
        .filter(|release| {
            !release
                .get("draft")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        })
        .filter_map(server_release)
        .collect();
    Ok((found, releases.len()))
}

fn newest_first(mut found: Vec<ServerRelease>) -> Vec<ServerRelease> {
    found.sort_by_key(|release| std::cmp::Reverse(version_key(&release.version)));
    found.dedup_by(|a, b| a.version == b.version);
    found
}

fn server_release(release: &serde_json::Value) -> Option<ServerRelease> {
    let tag = release.get("tag_name")?.as_str()?;
    let version = tag.strip_prefix(RELEASE_TAG_PREFIX)?;
    let assets = release.get("assets")?.as_array()?;
    let binary_name = server_asset_name(version);
    let checksum_name = format!("{binary_name}{CHECKSUM_SUFFIX}");

    let binary = assets.iter().find(|asset| named(asset, &binary_name))?;
    let checksum = assets.iter().find(|asset| named(asset, &checksum_name))?;
    Some(ServerRelease {
        version: version.to_owned(),
        tag: tag.to_owned(),
        url: download_url(binary)?,
        checksum_url: download_url(checksum)?,
        size: binary
            .get("size")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
    })
}

fn named(asset: &serde_json::Value, name: &str) -> bool {
    asset.get("name").and_then(serde_json::Value::as_str) == Some(name)
}

fn download_url(asset: &serde_json::Value) -> Option<String> {
    Some(asset.get("browser_download_url")?.as_str()?.to_owned())
}

fn verify(name: &str, binary: &[u8], checksum: &str) -> Result<(), ReleaseError> {
    let expected = parse_checksum(checksum).ok_or_else(|| ReleaseError::MalformedChecksum {
        name: name.to_owned(),
        text: checksum.trim().chars().take(80).collect(),
    })?;
    let actual = sha256_hex(binary);
    if actual == expected {
        return Ok(());
    }
    Err(ReleaseError::Checksum {
        name: name.to_owned(),
        actual,
        expected,
    })
}

fn parse_checksum(text: &str) -> Option<String> {
    let token = text.split_whitespace().next()?;
    (token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| token.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str, tag: &str, size: u64) -> String {
        let url = format!("example.invalid/{tag}/{name}");
        format!("{{\"name\": \"{name}\", \"size\": {size}, \"browser_download_url\": \"{url}\"}}")
    }

    fn release_json(tag: &str, assets: &[(String, u64)]) -> String {
        let assets: Vec<String> = assets
            .iter()
            .map(|(name, size)| asset(name, tag, *size))
            .collect();
        format!(
            "{{\"tag_name\": \"{tag}\", \"draft\": false, \"assets\": [{}]}}",
            assets.join(", "),
        )
    }

    fn with_server(version: &str) -> String {
        let binary = server_asset_name(version);
        release_json(
            &format!("{RELEASE_TAG_PREFIX}{version}"),
            &[
                (binary.clone(), 6_749_168),
                (format!("{binary}{CHECKSUM_SUFFIX}"), 105),
            ],
        )
    }

    fn feed(releases: &[String]) -> String {
        format!("[{}]", releases.join(", "))
    }

    fn parse_releases(body: &str) -> Result<Vec<ServerRelease>, ReleaseError> {
        page_releases(body).map(|(found, _)| newest_first(found))
    }

    fn other(tag: &str, assets: &[&str]) -> String {
        let assets: Vec<(String, u64)> =
            assets.iter().map(|name| ((*name).to_owned(), 1)).collect();
        release_json(tag, &assets)
    }

    #[test]
    fn the_newest_server_release_comes_first() {
        let body = feed(&[
            with_server("0.10.0"),
            with_server("0.9.1"),
            with_server("0.11.0"),
        ]);
        let versions: Vec<String> = parse_releases(&body)
            .unwrap()
            .into_iter()
            .map(|release| release.version)
            .collect();
        assert_eq!(versions, ["0.11.0", "0.10.0", "0.9.1"]);
    }

    #[test]
    fn a_release_without_the_server_binary_is_not_offered() {
        let body = feed(&[
            other(
                "appliance-v0.9.0",
                &["autd3-appliance-rp4-0.9.0-20260919.img.xz", "SHA256SUMS"],
            ),
            other("v0.9.0", &[]),
            other("console-v0.9.0", &["console-installer.sh"]),
            other("firmware-v0.9.0", &["autd3-sdk-firmware-v0.9.0.zip"]),
        ]);
        assert!(parse_releases(&body).unwrap().is_empty());
    }

    #[test]
    fn a_binary_without_a_published_checksum_is_not_offered() {
        let body = feed(&[release_json(
            &format!("{RELEASE_TAG_PREFIX}0.10.0"),
            &[(server_asset_name("0.10.0"), 6_749_168)],
        )]);
        assert!(parse_releases(&body).unwrap().is_empty());
    }

    #[test]
    fn a_draft_release_is_not_offered() {
        let draft = with_server("0.10.0").replace("\"draft\": false", "\"draft\": true");
        assert!(parse_releases(&feed(&[draft])).unwrap().is_empty());
    }

    #[test]
    fn the_asset_urls_are_taken_from_the_release() {
        let release = parse_releases(&feed(&[with_server("0.10.0")]))
            .unwrap()
            .remove(0);
        assert_eq!(release.tag, "appliance-v0.10.0");
        assert_eq!(
            release.url,
            format!(
                "example.invalid/appliance-v0.10.0/{}",
                server_asset_name("0.10.0"),
            ),
        );
        assert_eq!(
            release.checksum_url,
            format!("{}{CHECKSUM_SUFFIX}", release.url)
        );
        assert_eq!(release.size, 6_749_168);
    }

    #[test]
    fn a_full_page_asks_for_the_next_one() {
        let full: Vec<String> = (0..PER_PAGE)
            .map(|i| other(&format!("console-v0.{i}.0"), &["console-installer.sh"]))
            .collect();
        let (found, entries) = page_releases(&feed(&full)).unwrap();
        assert!(found.is_empty());
        assert_eq!(
            entries, PER_PAGE,
            "the appliance releases can sit on a later page",
        );

        let (_, entries) = page_releases(&feed(&full[..3])).unwrap();
        assert!(entries < PER_PAGE, "a short page ends the walk");
    }

    #[test]
    fn a_response_that_is_not_a_release_list_is_refused() {
        assert!(matches!(
            parse_releases("{\"message\": \"API rate limit exceeded\"}"),
            Err(ReleaseError::NotAnArray),
        ));
        assert!(matches!(
            parse_releases("<html>"),
            Err(ReleaseError::Parse(_)),
        ));
    }

    #[test]
    fn the_checksum_file_may_carry_the_file_name_after_the_hash() {
        let binary = b"server";
        let hex = sha256_hex(binary);
        let name = server_asset_name("0.10.0");
        assert!(verify(&name, binary, &format!("{hex}  {name}\n")).is_ok());
        assert!(verify(&name, binary, &format!("{}\n", hex.to_uppercase())).is_ok());
    }

    #[test]
    fn a_binary_that_does_not_match_the_published_checksum_is_refused() {
        let name = server_asset_name("0.10.0");
        let hex = sha256_hex(b"server");
        for (binary, checksum) in [
            (&b"tampered"[..], hex.as_str()),
            (&b"server"[..], "not-a-checksum"),
            (&b"server"[..], ""),
            (&b"server"[..], "abc123"),
        ] {
            assert!(verify(&name, binary, checksum).is_err());
        }
    }

    #[test]
    fn sha256_matches_the_reference_digest() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        );
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        );
    }

    #[test]
    fn a_newer_version_is_recognised_across_the_minor_bump() {
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(is_newer("1.0.0", "0.10.3"));
        assert!(!is_newer("0.9.0", "0.9.0"));
        assert!(!is_newer("0.9.0", "0.10.0"));
        assert_eq!(version_key("v1.2.3"), (1, 2, 3));
    }

    #[test]
    fn a_version_is_accepted_as_a_tag_or_a_bare_number() {
        for given in ["0.10.0", "v0.10.0", "appliance-v0.10.0", " 0.10.0 "] {
            assert_eq!(normalize_version(given), "0.10.0");
        }
    }

    #[test]
    fn the_asset_name_carries_the_appliance_target() {
        assert_eq!(
            server_asset_name("0.10.0"),
            "autd3-remote-server-0.10.0-aarch64-unknown-linux-musl",
        );
    }
}
