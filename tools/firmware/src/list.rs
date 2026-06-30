use std::io::Read;

use anyhow::{Context, Result};

const RELEASES_URL: &str = "https://api.github.com/repos/shinolab/autd3-sdk/releases?per_page=100";

pub fn print_available_versions() -> Result<()> {
    for version in fetch_versions()? {
        println!("{version}");
    }
    Ok(())
}

fn fetch_versions() -> Result<Vec<String>> {
    let resp = ureq::get(RELEASES_URL)
        .header("User-Agent", "autd3-firmware")
        .header("Accept", "application/vnd.github+json")
        .call()
        .context("querying GitHub releases")?;
    let mut body = String::new();
    resp.into_body()
        .into_reader()
        .read_to_string(&mut body)
        .context("reading GitHub releases response")?;

    let releases: serde_json::Value =
        serde_json::from_str(&body).context("parsing GitHub releases JSON")?;
    let releases = releases
        .as_array()
        .context("unexpected GitHub releases response (expected a JSON array)")?;

    let mut versions: Vec<String> = releases
        .iter()
        .filter_map(|r| r.get("tag_name").and_then(serde_json::Value::as_str))
        .filter_map(|tag| tag.strip_prefix("firmware-v"))
        .map(ToString::to_string)
        .collect();
    versions.sort_by_key(|v| std::cmp::Reverse(version_key(v)));
    versions.dedup();
    Ok(versions)
}

fn version_key(version: &str) -> (u64, u64, u64) {
    let mut parts = version
        .split(['.', '-', '+'])
        .filter_map(|p| p.parse::<u64>().ok());
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}
