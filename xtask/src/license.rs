use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::{Subcommand, ValueEnum};

use crate::util::{copy_file, on_path, run, workspace_members};

const PUBLISH_WORKSPACES: &[&str] = &[
    ".",
    "extras/autd3-rs-emulator",
    "extras/autd3-rs-pattern-holo-wgpu",
];

const DENY_WORKSPACES: &[&str] = &[
    ".",
    "console",
    "simulator",
    "simulator/frontend",
    "bindings/ffi",
    "bindings/python",
];

const AUDIT_ONLY_WORKSPACES: &[&str] = &[
    "xtask",
    "extras/autd3-rs-emulator",
    "extras/autd3-rs-pattern-holo-wgpu",
    "firmware/cpu/board",
];

const THIRD_PARTY: &str = "THIRD-PARTY-LICENSES.md";

const PLACEHOLDER: &str = "# Third-party licenses\n\nThis notice was not generated for this build.\nRun `cargo xtask license generate`, or pass `--license` to the packaging command,\nto produce the real notices with cargo-about.\n";

#[derive(Subcommand)]
pub enum LicenseCmd {
    #[command(about = "Generate the third-party license notices with cargo-about")]
    Generate {
        #[arg(value_enum, default_value_t = GenTarget::All, help = "Artifact to generate the notices for")]
        target: GenTarget,
    },
    #[command(about = "Check the dependency licenses of every workspace with cargo-deny")]
    Check,
    #[command(
        about = "Check every workspace for security advisories, duplicates and unknown sources"
    )]
    Audit,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum GenTarget {
    All,
    Python,
    Csharp,
    Unity,
    Console,
    Simulator,
}

pub fn run_license(root: &Path, cmd: &LicenseCmd) -> Result<()> {
    match cmd {
        LicenseCmd::Generate { target } => generate(root, *target),
        LicenseCmd::Check => check(root),
        LicenseCmd::Audit => audit(root),
    }
}

fn generate(root: &Path, target: GenTarget) -> Result<()> {
    match target {
        GenTarget::All => {
            generate_python(root, true)?;
            generate_csharp(root, true)?;
            generate_unity(root, true)?;
            generate_console(root, true)?;
        }
        GenTarget::Python => generate_python(root, true)?,
        GenTarget::Csharp => generate_csharp(root, true)?,
        GenTarget::Unity => generate_unity(root, true)?,
        GenTarget::Console => generate_console(root, true)?,
        GenTarget::Simulator => generate_simulator(root, true)?,
    }
    println!("license generate: done");
    Ok(())
}

fn check(root: &Path) -> Result<()> {
    check_bundled_license(root)?;
    deny(root, DENY_WORKSPACES.iter().copied(), &["licenses"])
}

fn audit(root: &Path) -> Result<()> {
    deny(
        root,
        DENY_WORKSPACES.iter().chain(AUDIT_ONLY_WORKSPACES).copied(),
        &["advisories", "bans", "sources"],
    )
}

fn deny<'a>(root: &Path, workspaces: impl Iterator<Item = &'a str>, checks: &[&str]) -> Result<()> {
    if !on_path("cargo-deny") {
        bail!("`cargo-deny` is required");
    }
    let config = root.join("deny.toml");
    let config = config.to_string_lossy().into_owned();
    let mut failed = Vec::new();
    for ws in workspaces {
        let dir = root.join(ws);
        println!(
            "== cargo-deny check {}: {} ==",
            checks.join(" "),
            dir.display()
        );
        let args = ["deny", "--config", config.as_str(), "check"]
            .into_iter()
            .chain(checks.iter().copied());
        if run("cargo", args, &dir).is_err() {
            failed.push(ws.to_owned());
        }
    }
    if !failed.is_empty() {
        bail!("cargo-deny failed in: {}", failed.join(", "));
    }
    Ok(())
}

fn check_bundled_license(root: &Path) -> Result<()> {
    println!("== bundled license text ==");
    let mit = std::fs::read_to_string(root.join("LICENSE"))
        .with_context(|| format!("reading {}", root.join("LICENSE").display()))?;

    let mut missing = Vec::new();
    for ws in PUBLISH_WORKSPACES {
        let ws_dir = if *ws == "." {
            root.to_path_buf()
        } else {
            root.join(ws)
        };
        for package in workspace_members(&ws_dir)?
            .iter()
            .filter(|package| package.publishable())
        {
            let path = package.dir().join("LICENSE");
            match std::fs::read_to_string(&path) {
                Ok(text) if text == mit => {}
                Ok(_) => missing.push(format!(
                    "{}: {} differs from the root text",
                    package.name(),
                    path.display()
                )),
                Err(_) => {
                    missing.push(format!("{}: {} is missing", package.name(), path.display()));
                }
            }
        }
    }
    if !missing.is_empty() {
        bail!(
            "publishable crates must bundle their license text (`cargo package` cannot reach outside the crate directory):\n  {}",
            missing.join("\n  ")
        );
    }
    println!("bundled license text: ok");
    Ok(())
}

fn ensure_about(generate: bool) -> Result<()> {
    if generate && !on_path("cargo-about") {
        bail!("`cargo-about` is required");
    }
    Ok(())
}

pub fn generate_python(root: &Path, generate: bool) -> Result<()> {
    ensure_about(generate)?;
    let mit_license = root.join("LICENSE");

    let py = root.join("bindings/python");
    for wheel in crate::py::WHEELS {
        let dir = py.join(wheel);
        about(
            root,
            &dir.join("Cargo.toml"),
            &dir.join(THIRD_PARTY),
            generate,
        )?;
        copy_file(&mit_license, &dir.join("LICENSE"))?;
    }
    Ok(())
}

pub fn generate_csharp(root: &Path, generate: bool) -> Result<()> {
    ensure_about(generate)?;

    let ffi = root.join("bindings/ffi");
    let cs_src = root.join("bindings/csharp/src");
    for pkg in crate::cs::PACKAGES {
        let krate = pkg.ffi_crate;
        let dir = cs_src.join(pkg.assembly);
        about(
            root,
            &ffi.join(krate).join("Cargo.toml"),
            &dir.join(THIRD_PARTY),
            generate,
        )?;
    }
    Ok(())
}

pub fn generate_unity(root: &Path, generate: bool) -> Result<()> {
    ensure_about(generate)?;
    let mit_license = root.join("LICENSE");

    let ffi = root.join("bindings/ffi");
    let unity = root.join("bindings/unity");
    for pkg in crate::cs::PACKAGES {
        let krate = pkg.ffi_crate;
        let dir = unity.join(pkg.unity_id);
        about(
            root,
            &ffi.join(krate).join("Cargo.toml"),
            &dir.join(THIRD_PARTY),
            generate,
        )?;
        copy_file(&mit_license, &dir.join("LICENSE.md"))?;
    }
    Ok(())
}

pub fn generate_console(root: &Path, generate: bool) -> Result<()> {
    ensure_about(generate)?;
    let console = root.join("console");
    let out = console.join(THIRD_PARTY);
    about(root, &console.join("Cargo.toml"), &out, generate)?;

    generate_simulator(root, generate)?;
    let simulator_notice = root.join("simulator").join(THIRD_PARTY);
    let simulator = std::fs::read_to_string(&simulator_notice)
        .with_context(|| format!("reading {}", simulator_notice.display()))?;

    let firmware_tmp = console.join(".third-party-firmware.md");
    about(
        root,
        &root
            .join("tools")
            .join("firmware-writer")
            .join("Cargo.toml"),
        &firmware_tmp,
        generate,
    )?;
    let firmware = std::fs::read_to_string(&firmware_tmp)
        .with_context(|| format!("reading {}", firmware_tmp.display()))?;

    let ota_tmp = console.join(".third-party-firmware-ota.md");
    about(
        root,
        &root.join("tools").join("firmware-ota").join("Cargo.toml"),
        &ota_tmp,
        generate,
    )?;
    let ota = std::fs::read_to_string(&ota_tmp)
        .with_context(|| format!("reading {}", ota_tmp.display()))?;

    let server_tmp = console.join(".third-party-server.md");
    about(
        root,
        &root.join("server").join("Cargo.toml"),
        &server_tmp,
        generate,
    )?;
    let server = std::fs::read_to_string(&server_tmp)
        .with_context(|| format!("reading {}", server_tmp.display()))?;

    let mut combined =
        std::fs::read_to_string(&out).with_context(|| format!("reading {}", out.display()))?;
    combined.push_str("\n\n---\n\n# Simulator dependencies\n\n");
    combined.push_str(&simulator);
    combined.push_str("\n\n---\n\n# Firmware tool dependencies\n\n");
    combined.push_str(&firmware);
    combined.push_str("\n\n---\n\n# Firmware OTA tool dependencies\n\n");
    combined.push_str(&ota);
    combined.push_str("\n\n---\n\n# Server dependencies\n\n");
    combined.push_str(&server);
    std::fs::write(&out, combined).with_context(|| format!("writing {}", out.display()))?;
    std::fs::remove_file(&firmware_tmp).ok();
    std::fs::remove_file(&ota_tmp).ok();
    std::fs::remove_file(&server_tmp).ok();
    Ok(())
}

pub fn generate_simulator(root: &Path, generate: bool) -> Result<()> {
    ensure_about(generate)?;
    let sim = root.join("simulator");
    let out = sim.join(THIRD_PARTY);
    about(root, &sim.join("Cargo.toml"), &out, generate)?;

    let frontend_tmp = sim.join(".third-party-frontend.md");
    about(
        root,
        &sim.join("frontend/Cargo.toml"),
        &frontend_tmp,
        generate,
    )?;

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

fn about(root: &Path, manifest: &Path, out: &Path, generate: bool) -> Result<()> {
    if !generate {
        return placeholder(manifest, out);
    }
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

fn placeholder(manifest: &Path, out: &Path) -> Result<()> {
    println!("== cargo-about skipped: {} ==", manifest.display());
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(out, PLACEHOLDER).with_context(|| format!("writing {}", out.display()))
}
