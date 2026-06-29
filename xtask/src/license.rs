use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::Subcommand;

use crate::util::{on_path, run};

const PY_MIT_WHEELS: &[&str] = &[
    "autd3-core",
    "autd3",
    "autd3-pattern",
    "autd3-pattern-holo",
    "autd3-modulation",
    "autd3-link-ethercrab",
    "autd3-link-remote",
    "autd3-link-twincat",
    "autd3-link-nop",
];
const PY_SOEM_WHEEL: &str = "autd3-link-soem";

// C# package directory -> the FFI crate whose cdylib that package ships.
const CS_PACKAGES: &[(&str, &str)] = &[
    ("AUTD3.Core", "autd3-ffi-core"),
    ("AUTD3", "autd3-ffi"),
    ("AUTD3.Pattern", "autd3-ffi-pattern"),
    ("AUTD3.Pattern.Holo", "autd3-ffi-pattern-holo"),
    ("AUTD3.Modulation", "autd3-ffi-modulation"),
    ("AUTD3.Link.Ethercrab", "autd3-ffi-link-ethercrab"),
    ("AUTD3.Link.Remote", "autd3-ffi-link-remote"),
    ("AUTD3.Link.Twincat", "autd3-ffi-link-twincat"),
    ("AUTD3.Link.Nop", "autd3-ffi-link-nop"),
];
const CS_SOEM_PACKAGE: &str = "AUTD3.Link.Soem";
const CS_SOEM_CRATE: &str = "autd3-ffi-link-soem";

const DENY_WORKSPACES: &[&str] = &[
    ".",
    "console",
    "simulator",
    "simulator/frontend",
    "bindings/ffi",
    "bindings/python",
];

const THIRD_PARTY: &str = "THIRD-PARTY-LICENSES.md";

#[derive(Subcommand)]
pub enum LicenseCmd {
    /// Generate THIRD-PARTY-LICENSES.md for every distributable (via cargo-about)
    /// and place the matching LICENSE / NOTICE files next to each artifact.
    Generate,
    /// Verify dependency licenses with cargo-deny: copyleft is denied everywhere
    /// except the autd3-rs-link-soem exception declared in deny.toml.
    Check,
}

pub fn run_license(root: &Path, cmd: &LicenseCmd) -> Result<()> {
    match cmd {
        LicenseCmd::Generate => generate(root),
        LicenseCmd::Check => check(root),
    }
}

fn generate(root: &Path) -> Result<()> {
    generate_python(root)?;
    generate_csharp(root)?;
    generate_console(root)?;
    generate_simulator(root)?;
    println!("license generate: done");
    Ok(())
}

fn check(root: &Path) -> Result<()> {
    if !on_path("cargo-deny") {
        bail!("`cargo-deny` is required (cargo install --locked cargo-deny@0.19.9)");
    }
    let config = root.join("deny.toml");
    let config = config.to_string_lossy().into_owned();
    for ws in DENY_WORKSPACES {
        let dir = root.join(ws);
        println!("== cargo-deny check licenses: {} ==", dir.display());
        run(
            "cargo",
            ["deny", "check", "--config", &config, "licenses"],
            &dir,
        )?;
    }
    Ok(())
}

fn ensure_about() -> Result<()> {
    if !on_path("cargo-about") {
        bail!("`cargo-about` is required (cargo install --locked cargo-about@0.9.0)");
    }
    Ok(())
}

// One THIRD-PARTY per wheel so each ships exactly its own deps (keeps GPL out of
// the MIT wheels). Called by `cargo xtask py build` so wheels are always current.
pub fn generate_python(root: &Path) -> Result<()> {
    ensure_about()?;
    let mit_license = root.join("LICENSE");
    let gpl_license = root.join("crates/autd3-rs-link-soem/COPYING");
    let soem_notice = root.join("extra-licenses/soem-notice.md");

    let py = root.join("bindings/python");
    for wheel in PY_MIT_WHEELS {
        let dir = py.join(wheel);
        about(root, &dir.join("Cargo.toml"), &dir.join(THIRD_PARTY))?;
        copy(&mit_license, &dir.join("LICENSE"))?;
    }
    let dir = py.join(PY_SOEM_WHEEL);
    about(root, &dir.join("Cargo.toml"), &dir.join(THIRD_PARTY))?;
    copy(&gpl_license, &dir.join("LICENSE"))?;
    copy(&soem_notice, &dir.join("NOTICE"))?;
    Ok(())
}

// One THIRD-PARTY per package, generated from the FFI crate whose cdylib that
// package ships. soem additionally carries GPLv3 + NOTICE.
pub fn generate_csharp(root: &Path) -> Result<()> {
    ensure_about()?;
    let gpl_license = root.join("crates/autd3-rs-link-soem/COPYING");
    let soem_notice = root.join("extra-licenses/soem-notice.md");

    let ffi = root.join("bindings/ffi");
    let cs_src = root.join("bindings/csharp/src");
    for (pkg, krate) in CS_PACKAGES {
        let dir = cs_src.join(pkg);
        about(
            root,
            &ffi.join(krate).join("Cargo.toml"),
            &dir.join(THIRD_PARTY),
        )?;
    }
    let dir = cs_src.join(CS_SOEM_PACKAGE);
    about(
        root,
        &ffi.join(CS_SOEM_CRATE).join("Cargo.toml"),
        &dir.join(THIRD_PARTY),
    )?;
    copy(&gpl_license, &dir.join("COPYING"))?;
    copy(&soem_notice, &dir.join("NOTICE"))?;
    Ok(())
}

// Single crate, no copyleft / native deps. Called by `cargo xtask console bundle`.
pub fn generate_console(root: &Path) -> Result<()> {
    ensure_about()?;
    let console = root.join("console");
    about(
        root,
        &console.join("Cargo.toml"),
        &console.join(THIRD_PARTY),
    )
}

pub fn generate_simulator(root: &Path) -> Result<()> {
    ensure_about()?;
    let sim = root.join("simulator");
    let out = sim.join(THIRD_PARTY);
    about(root, &sim.join("Cargo.toml"), &out)?;

    let frontend_tmp = sim.join(".third-party-frontend.md");
    about(root, &sim.join("frontend/Cargo.toml"), &frontend_tmp)?;

    let mut combined =
        std::fs::read_to_string(&out).with_context(|| format!("reading {}", out.display()))?;
    let frontend = std::fs::read_to_string(&frontend_tmp)
        .with_context(|| format!("reading {}", frontend_tmp.display()))?;
    combined.push_str("\n\n---\n\n# Browser frontend dependencies\n\n");
    combined.push_str(&frontend);
    std::fs::write(&out, combined).with_context(|| format!("writing {}", out.display()))?;
    std::fs::remove_file(&frontend_tmp).ok();
    Ok(())
}

fn about(root: &Path, manifest: &Path, out: &Path) -> Result<()> {
    let about_toml = root.join("about.toml");
    let template = root.join("about.hbs");
    println!("== cargo-about: {} ==", manifest.display());
    run(
        "cargo",
        [
            "about",
            "generate",
            "-c",
            &about_toml.to_string_lossy(),
            "--manifest-path",
            &manifest.to_string_lossy(),
            &template.to_string_lossy(),
            "-o",
            &out.to_string_lossy(),
        ],
        root,
    )
}

fn copy(src: &Path, dst: &Path) -> Result<()> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(src, dst)
        .with_context(|| format!("copying {} -> {}", src.display(), dst.display()))?;
    Ok(())
}
