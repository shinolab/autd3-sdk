use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::Args;
use toml_edit::{DocumentMut, Item, Value, value};

use crate::changelog::write_changelog_file;
use crate::component::{COMPONENTS, Component, detect};
use crate::cpu::gen_param;
use crate::util::capture;

#[derive(Args)]
pub struct BumpVersionCmd {
    component: Option<String>,

    version: Option<String>,

    #[arg(long)]
    no_changelog: bool,
}

pub fn run_bump_version(root: &Path, cmd: &BumpVersionCmd) -> Result<()> {
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
            println!("Updated software version -> {core} (crates, ffi, python, csharp, emulator)");
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
                "Updated firmware version (app.h + params.svh, regenerated params_fpga.h) -> {core}"
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
    print_next_steps(component.name);
    println!("  git commit -m \"chore: release {tag}\"");
    Ok(())
}

fn resolve_component(root: &Path, name: Option<&str>) -> Result<&'static Component> {
    if let Some(name) = name {
        return COMPONENTS.iter().find(|c| c.name == name).with_context(|| {
            let known = COMPONENTS
                .iter()
                .map(|c| c.name)
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
            println!(
                "  git add Cargo.toml Cargo.lock CHANGELOG.md bindings/ffi/Cargo.toml bindings/ffi/Cargo.lock bindings/python/Cargo.toml bindings/python/Cargo.lock 'bindings/python/*/pyproject.toml' bindings/csharp/Directory.Build.props emulator/Cargo.toml emulator/Cargo.lock"
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
                "  git add firmware/cpu/src/app.h firmware/fpga/rtl/sources_1/new/headers/params.svh firmware/cpu/inc/params_fpga.h CHANGELOG.md"
            );
        }
        _ => {}
    }
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
