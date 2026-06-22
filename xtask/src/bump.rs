use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::Args;
use toml_edit::{DocumentMut, Item, Value, value};

use crate::changelog::write_changelog_file;
use crate::component::detect;
use crate::cpu::gen_param;
use crate::util::capture;

#[derive(Args)]
pub struct BumpVersionCmd {
    #[arg(long)]
    no_changelog: bool,
}

pub fn run_bump_version(root: &Path, cmd: &BumpVersionCmd) -> Result<()> {
    let branch = capture("git", &["rev-parse", "--abbrev-ref", "HEAD"], root)?;
    let stem = branch.strip_prefix("release/").with_context(|| {
        format!(
            "must be on a `release/<prefix><x.y.z>` branch to bump version (current: `{branch}`)"
        )
    })?;
    let (component, version) = detect(stem)
        .filter(|(_, v)| is_semver(v))
        .with_context(|| format!("branch `{branch}` does not match any release component"))?;
    let tag = format!("{}{version}", component.tag_prefix);

    match component.name {
        "software" => {
            bump_cargo_toml(root, version)?;
            println!("Updated Cargo.toml: workspace version and autd3-rs* deps -> {version}");
        }
        "firmware" => {
            bump_firmware(root, version)?;
            println!(
                "Updated firmware version (app.h + params.svh, regenerated params_fpga.h) -> {version}"
            );
        }
        other => bail!("no version-bump implementation for component `{other}`"),
    }

    if cmd.no_changelog {
        println!("Skipped CHANGELOG.md (--no-changelog).");
    } else {
        write_changelog_file(root, Some(&tag), "CHANGELOG.md")?;
        println!("Generated CHANGELOG.md for {tag}");
    }

    println!();
    println!("Next (do these manually after reviewing the diff):");
    match component.name {
        "software" => {
            println!("  cargo xtask rust build         # refresh Cargo.lock");
            println!("  git add Cargo.toml Cargo.lock CHANGELOG.md");
        }
        "firmware" => {
            println!(
                "  git add firmware/cpu/src/app.h firmware/fpga/rtl/sources_1/new/headers/params.svh firmware/cpu/inc/params_fpga.h CHANGELOG.md"
            );
        }
        _ => {}
    }
    println!("  git commit -m \"chore: release {tag}\"");
    Ok(())
}

fn is_semver(v: &str) -> bool {
    let core = v.split(['-', '+']).next().unwrap_or(v);
    let parts: Vec<&str> = core.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

fn version_parts(version: &str) -> Result<[u32; 3]> {
    let core = version.split(['-', '+']).next().unwrap_or(version);
    let mut it = core.split('.');
    let mut out = [0u32; 3];
    for slot in &mut out {
        *slot = it
            .next()
            .context("missing version component")?
            .parse()
            .context("non-numeric version component")?;
    }
    Ok(out)
}

fn bump_cargo_toml(root: &Path, version: &str) -> Result<()> {
    let path = root.join("Cargo.toml");
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let mut doc: DocumentMut = text.parse().context("parsing Cargo.toml")?;

    let workspace = doc
        .get_mut("workspace")
        .and_then(Item::as_table_like_mut)
        .context("missing [workspace] table in Cargo.toml")?;

    let package = workspace
        .get_mut("package")
        .and_then(Item::as_table_like_mut)
        .context("missing [workspace.package] table")?;
    package.insert("version", value(version));

    let deps = doc
        .get_mut("workspace")
        .and_then(|w| w.get_mut("dependencies"))
        .and_then(Item::as_table_like_mut)
        .context("missing [workspace.dependencies] table")?;
    for (key, item) in deps.iter_mut() {
        if !key.get().starts_with("autd3-rs") {
            continue;
        }
        if let Some(inline) = item.as_inline_table_mut() {
            if inline.contains_key("version") {
                inline.insert("version", Value::from(version));
            }
        } else if item.as_str().is_some() {
            *item = value(version);
        }
    }

    std::fs::write(&path, doc.to_string())
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn bump_firmware(root: &Path, version: &str) -> Result<()> {
    let [major, minor, patch] = version_parts(version)?;

    let app_h = root.join("firmware/cpu/src/app.h");
    let mut text =
        std::fs::read_to_string(&app_h).with_context(|| format!("reading {}", app_h.display()))?;
    for (key, val) in [
        ("FW_VERSION_MAJOR (", major),
        ("FW_VERSION_MINOR (", minor),
        ("FW_VERSION_PATCH (", patch),
    ] {
        text = bump_digits_after(&text, key, val)?;
    }
    std::fs::write(&app_h, text).with_context(|| format!("writing {}", app_h.display()))?;

    let svh = root.join("firmware/fpga/rtl/sources_1/new/headers/params.svh");
    let mut text =
        std::fs::read_to_string(&svh).with_context(|| format!("reading {}", svh.display()))?;
    for (key, val) in [
        ("VersionNumMajor = 8'd", major),
        ("VersionNumMinor = 8'd", minor),
        ("VersionNumPatch = 8'd", patch),
    ] {
        text = bump_digits_after(&text, key, val)?;
    }
    std::fs::write(&svh, text).with_context(|| format!("writing {}", svh.display()))?;

    gen_param(root)
}

fn bump_digits_after(content: &str, key: &str, new: u32) -> Result<String> {
    let pos = content
        .find(key)
        .with_context(|| format!("`{key}` not found"))?;
    let start = pos + key.len();
    let len = content[start..]
        .find(|c: char| !c.is_ascii_digit())
        .with_context(|| format!("no digits after `{key}`"))?;
    Ok(format!(
        "{}{new}{}",
        &content[..start],
        &content[start + len..]
    ))
}
