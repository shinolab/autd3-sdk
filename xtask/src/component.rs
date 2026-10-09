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
    pub version_table: &'static [&'static str],
}

pub const COMPONENTS: &[Component] = &[
    Component {
        name: "software",
        section: "Rust",
        tag_prefix: "v",
        include_paths: &[
            "crates/**",
            "tools/**",
            "examples/**",
            "appliance/lib/**",
            "appliance/cli/**",
            "appliance/server/**",
            "bindings/ffi/**",
        ],
        also_shipped_by: &[],
        version_file: "Cargo.toml",
        version_table: &["workspace", "package"],
    },
    Component {
        name: "python",
        section: "Python",
        tag_prefix: "py-v",
        include_paths: &["bindings/python/**"],
        also_shipped_by: &["v"],
        version_file: "bindings/python/autd3/pyproject.toml",
        version_table: &["project"],
    },
    Component {
        name: "cs",
        section: "C#",
        tag_prefix: "cs-v",
        include_paths: &["bindings/csharp/**"],
        also_shipped_by: &["v"],
        version_file: "bindings/csharp/Directory.Build.props",
        version_table: &[],
    },
    Component {
        name: "unity",
        section: "Unity",
        tag_prefix: "unity-v",
        include_paths: &["bindings/unity/**"],
        also_shipped_by: &["cs-v", "v"],
        version_file: "bindings/unity/com.shinolab.autd3-sdk/package.json",
        version_table: &[],
    },
    Component {
        name: "simulator",
        section: "Simulator",
        tag_prefix: "simulator-v",
        include_paths: &["simulator/**"],
        also_shipped_by: &["console-v"],
        version_file: "simulator/Cargo.toml",
        version_table: &["workspace", "package"],
    },
    Component {
        name: "console",
        section: "Console",
        tag_prefix: "console-v",
        include_paths: &["console/**"],
        also_shipped_by: &[],
        version_file: "console/Cargo.toml",
        version_table: &["package"],
    },
    Component {
        name: "firmware",
        section: "Firmware",
        tag_prefix: "firmware-v",
        include_paths: &["firmware/**"],
        also_shipped_by: &[],
        version_file: "firmware/cpu/fw/Cargo.toml",
        version_table: &["package"],
    },
];

pub fn find(name: &str) -> Result<&'static Component> {
    COMPONENTS
        .iter()
        .find(|c| c.name == name)
        .with_context(|| format!("missing `{name}` component"))
}

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
            _ => toml_version(&text, self.version_table),
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

fn toml_version(text: &str, table: &[&str]) -> Option<String> {
    let doc: toml_edit::DocumentMut = text.parse().ok()?;
    table
        .iter()
        .try_fold(doc.as_item(), |item, key| item.get(key))?
        .get("version")
        .and_then(toml_edit::Item::as_str)
        .map(str::to_string)
}

fn between(text: &str, open: &str, close: &str) -> Option<String> {
    let start = text.find(open)? + open.len();
    let end = text[start..].find(close)? + start;
    Some(text[start..end].trim().to_string())
}

pub(crate) fn after_quoted(text: &str, key: &str) -> Option<String> {
    let start = text.find(key)? + key.len();
    let rest = text[start..].trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

pub fn release_sections(primary: &'static Component) -> Vec<&'static Component> {
    let names: &[&str] = match primary.name {
        "software" => &["software", "python", "cs"],
        "console" => &["console", "simulator"],
        _ => return vec![primary],
    };
    names.iter().filter_map(|name| find(name).ok()).collect()
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
