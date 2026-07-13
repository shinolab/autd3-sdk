use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::Args;
use toml_edit::{DocumentMut, Item, Value, value};

use crate::changelog::write_changelog_file;
use crate::component::{COMPONENTS, Component, detect};
use crate::cpu::gen_param;
use crate::util::capture;

const DOC_COMPONENT: &str = "doc";

#[derive(Args)]
pub struct BumpVersionCmd {
    component: Option<String>,

    version: Option<String>,

    #[arg(long)]
    no_changelog: bool,
}

pub fn run_bump_version(root: &Path, cmd: &BumpVersionCmd) -> Result<()> {
    if cmd.component.as_deref() == Some(DOC_COMPONENT) {
        return bump_doc(root, cmd.version.as_deref());
    }
    let component = resolve_component(root, cmd.component.as_deref())?;
    let raw = match cmd.version.as_deref() {
        Some(v) => v.to_string(),
        None => version_from_branch(root, component)?,
    };
    let allow_build = matches!(component.name, "python" | "cs");
    let (core, full) = parse_version(&raw, allow_build)?;
    let tag = format!("{}{full}", component.tag_prefix);

    match component.name {
        "software" => {
            bump_cargo_toml(&root.join("Cargo.toml"), &core)?;
            bump_cargo_toml(&root.join("bindings/ffi/Cargo.toml"), &core)?;
            bump_cargo_toml(&root.join("bindings/python/Cargo.toml"), &core)?;
            bump_python_pyproject(root, &core)?;
            bump_csharp_props(&root.join("bindings/csharp/Directory.Build.props"), &core)?;
            bump_cargo_toml(&root.join("emulator/Cargo.toml"), &core)?;
            bump_cargo_toml(&root.join("simulator/Cargo.toml"), &core)?;
            bump_package_version(&root.join("console/Cargo.toml"), &core)?;
            println!(
                "Updated software version -> {core} (crates, ffi, python, csharp, emulator, simulator, console)"
            );
            bump_unity_series(root, &core)?;
        }
        "python" => {
            bump_cargo_toml(&root.join("bindings/python/Cargo.toml"), &core)?;
            bump_python_pyproject(root, &full)?;
            println!("Updated python version -> pyproject {full} (crate {core})");
        }
        "cs" => {
            bump_csharp_props(&root.join("bindings/csharp/Directory.Build.props"), &full)?;
            println!("Updated C# version -> {full}");
        }
        "unity" => {
            let count = bump_unity(root, &core)?;
            println!("Updated Unity version -> {core} ({count} package.json, incl. sibling deps)");
        }
        "simulator" => {
            bump_cargo_toml(&root.join("simulator/Cargo.toml"), &core)?;
            println!("Updated simulator version -> {core}");
        }
        "console" => {
            bump_package_version(&root.join("console/Cargo.toml"), &core)?;
            println!("Updated console version -> {core}");
        }
        "firmware" => {
            bump_firmware(root, &core)?;
            println!(
                "Updated firmware version (fw/src/version.rs + params.svh, regenerated params.rs) -> {core}"
            );
        }
        other => bail!("no version-bump implementation for component `{other}`"),
    }

    if cmd.no_changelog {
        println!("Skipped CHANGELOG.md (--no-changelog).");
    } else {
        write_changelog_file(root, "CHANGELOG.md")?;
        println!("Generated CHANGELOG.md for {tag}");
    }

    println!();
    println!("Next (do these manually after reviewing the diff):");
    print_next_steps(component.name);
    if component.name == "software" {
        println!();
        println!(
            "Then, to freeze the outgoing doc version series (no-op on a patch release; see README.md > ドキュメントサイト):"
        );
        println!("  cargo xtask bump-version doc");
        println!();
    }
    println!("  git commit -m \"chore: release {tag}\"");
    Ok(())
}

fn bump_doc(root: &Path, version: Option<&str>) -> Result<()> {
    let software = COMPONENTS
        .iter()
        .find(|c| c.name == "software")
        .context("missing `software` component")?;
    let raw = match version {
        Some(v) => v.to_string(),
        None => version_from_branch(root, software)?,
    };
    let (core, _) = parse_version(&raw, false)?;
    let mut parts = core.split('.');
    let (Some(major), Some(minor), Some(patch)) = (parts.next(), parts.next(), parts.next()) else {
        bail!("invalid version `{core}`");
    };
    if patch != "0" {
        println!(
            "Skipped doc: {core} is a patch release; the {major}.{minor}.x series is still the live one."
        );
        return Ok(());
    }

    let slug = format!("{major}.{minor}.x");
    if !crate::doc::add_version(root, &slug)? {
        return Ok(());
    }

    println!();
    println!("Next (do these manually after reviewing the diff):");
    println!(
        "  git add doc/astro.config.mjs doc/.gitignore doc/src/content/versions/{slug}.json   # the guard requires a tracked config"
    );
    println!("  cargo xtask doc build                    # generates doc/src/content/docs/{slug}/");
    println!("  cargo xtask doc freeze-version {slug}    # inlines codes + tracks the snapshot");
    println!("  git add doc/");
    Ok(())
}

fn resolve_component(root: &Path, name: Option<&str>) -> Result<&'static Component> {
    if let Some(name) = name {
        return COMPONENTS.iter().find(|c| c.name == name).with_context(|| {
            let known = COMPONENTS
                .iter()
                .map(|c| c.name)
                .chain([DOC_COMPONENT])
                .collect::<Vec<_>>()
                .join(", ");
            format!("unknown component `{name}` (known: {known})")
        });
    }
    let stem = release_stem(root)?;
    detect(&stem)
        .map(|(c, _)| c)
        .with_context(|| format!("branch `release/{stem}` matches no known release component"))
}

fn version_from_branch(root: &Path, component: &Component) -> Result<String> {
    let stem = release_stem(root)?;
    stem.strip_prefix(component.tag_prefix)
        .map(str::to_string)
        .with_context(|| {
            format!(
                "branch `release/{stem}` does not start with `{}`; pass the version explicitly",
                component.tag_prefix
            )
        })
}

fn release_stem(root: &Path) -> Result<String> {
    let branch = capture("git", &["rev-parse", "--abbrev-ref", "HEAD"], root)?;
    branch
        .strip_prefix("release/")
        .map(str::to_string)
        .with_context(|| {
            format!(
                "must be on a `release/<prefix><x.y.z>` branch to infer the version (current: `{branch}`)"
            )
        })
}

fn parse_version(version: &str, allow_build: bool) -> Result<(String, String)> {
    let parts: Vec<&str> = version.split('.').collect();
    let numeric = parts
        .iter()
        .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    if !numeric {
        bail!("invalid version `{version}`: components must be non-empty and numeric");
    }
    match parts.len() {
        3 => Ok((version.to_string(), version.to_string())),
        4 if allow_build => Ok((parts[..3].join("."), version.to_string())),
        4 => bail!(
            "version `{version}` has a build component; only `python`/`cs` accept major.minor.patch.build"
        ),
        n => {
            bail!(
                "invalid version `{version}`: expected major.minor.patch[.build], got {n} components"
            )
        }
    }
}

fn print_next_steps(name: &str) {
    match name {
        "software" => {
            println!("  cargo xtask rust build         # refresh Cargo.lock");
            println!("  cargo xtask ffi build          # refresh bindings/ffi/Cargo.lock");
            println!("  cargo xtask py build           # refresh bindings/python/Cargo.lock");
            println!("  cargo xtask emulator build     # refresh emulator/Cargo.lock");
            println!("  cargo xtask simulator build    # refresh simulator/Cargo.lock");
            println!("  cargo xtask console build      # refresh console/Cargo.lock");
            println!(
                "  git add Cargo.toml Cargo.lock CHANGELOG.md bindings/ffi/Cargo.toml bindings/ffi/Cargo.lock bindings/python/Cargo.toml bindings/python/Cargo.lock 'bindings/python/*/pyproject.toml' bindings/csharp/Directory.Build.props emulator/Cargo.toml emulator/Cargo.lock simulator/Cargo.toml simulator/Cargo.lock console/Cargo.toml console/Cargo.lock 'bindings/unity/*/package.json'"
            );
        }
        "python" => {
            println!("  cargo xtask py build           # refresh bindings/python/Cargo.lock");
            println!(
                "  git add bindings/python/Cargo.toml bindings/python/Cargo.lock 'bindings/python/*/pyproject.toml' CHANGELOG.md"
            );
        }
        "cs" => {
            println!("  git add bindings/csharp/Directory.Build.props CHANGELOG.md");
        }
        "unity" => {
            println!("  git add 'bindings/unity/*/package.json' CHANGELOG.md");
        }
        "simulator" => {
            println!("  cargo xtask simulator build    # refresh simulator/Cargo.lock");
            println!("  git add simulator/Cargo.toml simulator/Cargo.lock CHANGELOG.md");
        }
        "console" => {
            println!("  cargo xtask console build      # refresh console/Cargo.lock");
            println!("  git add console/Cargo.toml console/Cargo.lock CHANGELOG.md");
        }
        "firmware" => {
            println!(
                "  git add firmware/cpu/fw/src/version.rs firmware/fpga/rtl/sources_1/new/headers/params.svh firmware/cpu/fw/src/params.rs CHANGELOG.md"
            );
        }
        _ => {}
    }
}

fn bump_unity_series(root: &Path, core: &str) -> Result<()> {
    let unity = COMPONENTS
        .iter()
        .find(|c| c.name == "unity")
        .context("missing `unity` component")?;
    let current = unity.current_version(root)?;
    let [major, minor, _] = version_parts(core)?;
    let [cur_major, cur_minor, _] = version_parts(&current)?;
    if (major, minor) == (cur_major, cur_minor) {
        println!(
            "Kept Unity version {current} ({major}.{minor} series unchanged; npm patch is self-driven)"
        );
        return Ok(());
    }
    let version = format!("{major}.{minor}.0");
    let count = bump_unity(root, &version)?;
    println!("Updated Unity version -> {version} ({count} package.json, incl. sibling deps)");
    Ok(())
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

fn bump_cargo_toml(path: &Path, version: &str) -> Result<()> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut doc: DocumentMut = text
        .parse()
        .with_context(|| format!("parsing {}", path.display()))?;

    let package = doc
        .get_mut("workspace")
        .and_then(Item::as_table_like_mut)
        .and_then(|w| w.get_mut("package"))
        .and_then(Item::as_table_like_mut)
        .with_context(|| format!("missing [workspace.package] table in {}", path.display()))?;
    package.insert("version", value(version));

    if let Some(deps) = doc
        .get_mut("workspace")
        .and_then(|w| w.get_mut("dependencies"))
        .and_then(Item::as_table_like_mut)
    {
        for (key, item) in deps.iter_mut() {
            if !key.get().starts_with("autd3-") {
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
    }

    std::fs::write(path, doc.to_string()).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn bump_package_version(path: &Path, version: &str) -> Result<()> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut doc: DocumentMut = text
        .parse()
        .with_context(|| format!("parsing {}", path.display()))?;
    let package = doc
        .get_mut("package")
        .and_then(Item::as_table_like_mut)
        .with_context(|| format!("missing [package] table in {}", path.display()))?;
    package.insert("version", value(version));
    std::fs::write(path, doc.to_string()).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn bump_python_pyproject(root: &Path, version: &str) -> Result<()> {
    let py_root = root.join("bindings/python");
    let mut dirs: Vec<_> = std::fs::read_dir(&py_root)
        .with_context(|| format!("reading {}", py_root.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    dirs.sort();

    let mut count = 0usize;
    for dir in dirs {
        let path = dir.join("pyproject.toml");
        if !path.is_file() {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let mut doc: DocumentMut = text
            .parse()
            .with_context(|| format!("parsing {}", path.display()))?;
        let project = doc
            .get_mut("project")
            .and_then(Item::as_table_like_mut)
            .with_context(|| format!("missing [project] table in {}", path.display()))?;
        project.insert("version", value(version));
        if let Some(dynamic) = project.get_mut("dynamic").and_then(Item::as_array_mut) {
            dynamic.retain(|v| v.as_str() != Some("version"));
            if dynamic.is_empty() {
                project.remove("dynamic");
            }
        }
        std::fs::write(&path, doc.to_string())
            .with_context(|| format!("writing {}", path.display()))?;
        count += 1;
    }
    if count == 0 {
        bail!("no pyproject.toml found under {}", py_root.display());
    }
    Ok(())
}

fn bump_unity(root: &Path, version: &str) -> Result<usize> {
    let unity_root = root.join("bindings/unity");
    let mut dirs: Vec<_> = std::fs::read_dir(&unity_root)
        .with_context(|| format!("reading {}", unity_root.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();

    let dep_prefix = format!("\"{}", crate::unity::PKG_PREFIX);
    let mut count = 0usize;
    for dir in dirs {
        let path = dir.join("package.json");
        if !path.is_file() {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let mut out = String::with_capacity(text.len());
        for line in text.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("\"version\":") || trimmed.starts_with(dep_prefix.as_str()) {
                out.push_str(&replace_json_string_value(line, version));
            } else {
                out.push_str(line);
            }
            out.push('\n');
        }
        std::fs::write(&path, out).with_context(|| format!("writing {}", path.display()))?;
        count += 1;
    }
    if count == 0 {
        bail!("no package.json found under {}", unity_root.display());
    }
    Ok(count)
}

fn replace_json_string_value(line: &str, new: &str) -> String {
    let Some(colon) = line.find(':') else {
        return line.to_string();
    };
    let (key, rest) = line.split_at(colon + 1);
    let Some(open) = rest.find('"') else {
        return line.to_string();
    };
    let after_open = open + 1;
    let Some(rel_close) = rest[after_open..].find('"') else {
        return line.to_string();
    };
    let close = after_open + rel_close;
    format!("{key}{}\"{new}\"{}", &rest[..open], &rest[close + 1..])
}

fn bump_csharp_props(path: &Path, version: &str) -> Result<()> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let open = "<Version>";
    let close = "</Version>";
    let start = text
        .find(open)
        .with_context(|| format!("`{open}` not found in {}", path.display()))?
        + open.len();
    let end = text[start..]
        .find(close)
        .with_context(|| format!("`{close}` not found in {}", path.display()))?
        + start;
    let new = format!("{}{version}{}", &text[..start], &text[end..]);
    std::fs::write(path, new).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

pub fn firmware_series(root: &Path) -> Result<String> {
    let fw_lib = root.join("firmware/cpu/fw/src/version.rs");
    let text = std::fs::read_to_string(&fw_lib)
        .with_context(|| format!("reading {}", fw_lib.display()))?;
    let major = read_digits_after(&text, "FW_VERSION_MAJOR: u8 = ")?;
    let minor = read_digits_after(&text, "FW_VERSION_MINOR: u8 = ")?;

    let svh = root.join("firmware/fpga/rtl/sources_1/new/headers/params.svh");
    let text =
        std::fs::read_to_string(&svh).with_context(|| format!("reading {}", svh.display()))?;
    let fpga_major = read_digits_after(&text, "VersionNumMajor = 8'd")?;
    let fpga_minor = read_digits_after(&text, "VersionNumMinor = 8'd")?;

    if (major, minor) != (fpga_major, fpga_minor) {
        bail!(
            "CPU firmware version {major}.{minor}.x (fw/src/version.rs) and FPGA firmware version \
             {fpga_major}.{fpga_minor}.x (params.svh) disagree; run `cargo xtask bump-version firmware <version>`"
        );
    }
    Ok(format!("{major}.{minor}"))
}

fn bump_firmware(root: &Path, version: &str) -> Result<()> {
    let [major, minor, patch] = version_parts(version)?;

    let fw_lib = root.join("firmware/cpu/fw/src/version.rs");
    let mut text = std::fs::read_to_string(&fw_lib)
        .with_context(|| format!("reading {}", fw_lib.display()))?;
    for (key, val) in [
        ("FW_VERSION_MAJOR: u8 = ", major),
        ("FW_VERSION_MINOR: u8 = ", minor),
        ("FW_VERSION_PATCH: u8 = ", patch),
    ] {
        text = bump_digits_after(&text, key, val)?;
    }
    std::fs::write(&fw_lib, text).with_context(|| format!("writing {}", fw_lib.display()))?;

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

    gen_param(root)?;

    let pages = crate::doc::rewrite_firmware_series(root, &format!("{major}.{minor}"))?;
    println!("Updated firmware version in {pages} doc page(s) -> {major}.{minor}.x");
    Ok(())
}

fn read_digits_after(content: &str, key: &str) -> Result<u32> {
    let pos = content
        .find(key)
        .with_context(|| format!("`{key}` not found"))?;
    let start = pos + key.len();
    let len = content[start..]
        .find(|c: char| !c.is_ascii_digit())
        .with_context(|| format!("no digits after `{key}`"))?;
    content[start..start + len]
        .parse()
        .with_context(|| format!("non-numeric value after `{key}`"))
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
