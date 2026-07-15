use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::util::capture;

pub struct Component {
    pub name: &'static str,
    pub section: &'static str,
    pub tag_prefix: &'static str,
    pub include_paths: &'static [&'static str],
    pub also_shipped_by: &'static [&'static str],
    pub version_file: &'static str,
}

pub const COMPONENTS: &[Component] = &[
    Component {
        name: "software",
        section: "Rust",
        tag_prefix: "v",
        include_paths: &["crates/**", "tools/**", "examples/**", "bindings/ffi/**"],
        also_shipped_by: &[],
        version_file: "Cargo.toml",
    },
    Component {
        name: "python",
        section: "Python",
        tag_prefix: "py-v",
        include_paths: &["bindings/python/**"],
        also_shipped_by: &["v"],
        version_file: "bindings/python/autd3/pyproject.toml",
    },
    Component {
        name: "cs",
        section: "C#",
        tag_prefix: "cs-v",
        include_paths: &["bindings/csharp/**"],
        also_shipped_by: &["v"],
        version_file: "bindings/csharp/Directory.Build.props",
    },
    Component {
        name: "unity",
        section: "Unity",
        tag_prefix: "unity-v",
        include_paths: &["bindings/unity/**"],
        also_shipped_by: &["cs-v", "v"],
        version_file: "bindings/unity/com.shinolab.autd3-sdk/package.json",
    },
    Component {
        name: "simulator",
        section: "Simulator",
        tag_prefix: "simulator-v",
        include_paths: &["simulator/**"],
        also_shipped_by: &["console-v"],
        version_file: "simulator/Cargo.toml",
    },
    Component {
        name: "console",
        section: "Console",
        tag_prefix: "console-v",
        include_paths: &["console/**"],
        also_shipped_by: &[],
        version_file: "console/Cargo.toml",
    },
    Component {
        name: "firmware",
        section: "Firmware",
        tag_prefix: "firmware-v",
        include_paths: &["firmware/**"],
        also_shipped_by: &[],
        version_file: "firmware/cpu/fw/src/version.rs",
    },
];

impl Component {
    pub fn tag_pattern(&self) -> String {
        format!("^({})[0-9]", self.tag_prefixes().join("|"))
    }

    pub fn tag_prefixes(&self) -> Vec<&'static str> {
        let mut prefixes = vec![self.tag_prefix];
        prefixes.extend_from_slice(self.also_shipped_by);
        prefixes.sort_by_key(|p| std::cmp::Reverse(p.len()));
        prefixes
    }

    pub fn current_version(&self, root: &Path) -> Result<String> {
        let file = root.join(self.version_file);
        let text = std::fs::read_to_string(&file)
            .with_context(|| format!("reading {}", file.display()))?;
        let version = match self.name {
            "cs" => between(&text, "<Version>", "</Version>"),
            "unity" => after_quoted(&text, "\"version\":"),
            "firmware" => {
                let major = digits_after(&text, "FW_VERSION_MAJOR: u8 = ");
                let minor = digits_after(&text, "FW_VERSION_MINOR: u8 = ");
                let patch = digits_after(&text, "FW_VERSION_PATCH: u8 = ");
                match (major, minor, patch) {
                    (Some(a), Some(b), Some(c)) => Some(format!("{a}.{b}.{c}")),
                    _ => None,
                }
            }
            _ => toml_version(&text),
        };
        version.with_context(|| format!("no version found in {}", file.display()))
    }

    pub fn is_released(&self, root: &Path, version: &str) -> Result<bool> {
        let tags = capture("git", &["tag", "--list"], root)?;
        Ok(tags.lines().any(|tag| {
            self.tag_prefixes()
                .iter()
                .any(|prefix| tag.strip_prefix(prefix) == Some(version))
        }))
    }

    pub fn pending_tag(&self, root: &Path) -> Result<Option<String>> {
        let version = self.current_version(root)?;
        if version.is_empty() {
            bail!("empty version in {}", self.version_file);
        }
        if self.is_released(root, &version)? {
            return Ok(None);
        }
        Ok(Some(format!("{}{version}", self.tag_prefix)))
    }
}

fn toml_version(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| line.starts_with("version") && line[7..].trim_start().starts_with('='))
        .and_then(|line| after_quoted(line, "="))
}

fn between(text: &str, open: &str, close: &str) -> Option<String> {
    let start = text.find(open)? + open.len();
    let end = text[start..].find(close)? + start;
    Some(text[start..end].trim().to_string())
}

fn after_quoted(text: &str, key: &str) -> Option<String> {
    let start = text.find(key)? + key.len();
    let rest = text[start..].trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn digits_after(text: &str, key: &str) -> Option<String> {
    let start = text.find(key)? + key.len();
    let digits: String = text[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    (!digits.is_empty()).then_some(digits)
}

pub fn release_sections(primary: &'static Component) -> Vec<&'static Component> {
    let names: &[&str] = match primary.name {
        "software" => &["software", "python", "cs"],
        "console" => &["console", "simulator"],
        _ => return vec![primary],
    };
    names
        .iter()
        .filter_map(|name| COMPONENTS.iter().find(|c| c.name == *name))
        .collect()
}

pub fn detect<'a>(versioned: &'a str) -> Option<(&'static Component, &'a str)> {
    let mut best: Option<(&'static Component, &'a str)> = None;
    for c in COMPONENTS {
        if let Some(rest) = versioned.strip_prefix(c.tag_prefix)
            && best.is_none_or(|(b, _)| c.tag_prefix.len() > b.tag_prefix.len())
        {
            best = Some((c, rest));
        }
    }
    best
}
