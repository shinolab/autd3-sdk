use std::io::Read;

use anyhow::{Context, Result};

use autd3_firmware_writer::bundle::{RELEASES_API, TAG_PREFIX};

pub(crate) fn print_available_versions() -> Result<()> {
    let resp = ureq::get(RELEASES_API)
        .header("User-Agent", "autd3-firmware-writer")
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
        .filter_map(|tag| tag.strip_prefix(TAG_PREFIX))
        .map(ToString::to_string)
        .collect();
    versions.sort_by_key(|v| {
        let mut parts = v
            .split(['.', '-', '+'])
            .filter_map(|p| p.parse::<u64>().ok());
        std::cmp::Reverse((
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
        ))
    });
    versions.dedup();
    for version in versions {
        println!("{version}");
    }
    Ok(())
}
