use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::{Subcommand, ValueEnum};

use crate::util::{on_path, publishable_members, run};

const PY_WHEELS: &[&str] = &[
    "autd3-core",
    "autd3",
    "autd3-pattern",
    "autd3-pattern-holo",
    "autd3-modulation",
    "autd3-link-echocat",
    "autd3-link-ethercrab",
    "autd3-link-remote",
    "autd3-link-twincat",
    "autd3-link-nop",
    "autd3-emulator",
];

const CS_PACKAGES: &[(&str, &str)] = &[
    ("AUTD3.Core", "autd3-ffi-core"),
    ("AUTD3", "autd3-ffi"),
    ("AUTD3.Pattern", "autd3-ffi-pattern"),
    ("AUTD3.Pattern.Holo", "autd3-ffi-pattern-holo"),
    ("AUTD3.Modulation", "autd3-ffi-modulation"),
    ("AUTD3.Link.Echocat", "autd3-ffi-link-echocat"),
    ("AUTD3.Link.Ethercrab", "autd3-ffi-link-ethercrab"),
    ("AUTD3.Link.Remote", "autd3-ffi-link-remote"),
    ("AUTD3.Link.TwinCAT", "autd3-ffi-link-twincat"),
    ("AUTD3.Link.Nop", "autd3-ffi-link-nop"),
];

const UNITY_PACKAGES: &[(&str, &str)] = &[
    ("com.shinolab.autd3-sdk.core", "autd3-ffi-core"),
    ("com.shinolab.autd3-sdk", "autd3-ffi"),
    ("com.shinolab.autd3-sdk.pattern", "autd3-ffi-pattern"),
    (
        "com.shinolab.autd3-sdk.pattern.holo",
        "autd3-ffi-pattern-holo",
    ),
    ("com.shinolab.autd3-sdk.modulation", "autd3-ffi-modulation"),
    (
        "com.shinolab.autd3-sdk.link.echocat",
        "autd3-ffi-link-echocat",
    ),
    (
        "com.shinolab.autd3-sdk.link.ethercrab",
        "autd3-ffi-link-ethercrab",
    ),
    (
        "com.shinolab.autd3-sdk.link.remote",
        "autd3-ffi-link-remote",
    ),
    (
        "com.shinolab.autd3-sdk.link.twincat",
        "autd3-ffi-link-twincat",
    ),
    ("com.shinolab.autd3-sdk.link.nop", "autd3-ffi-link-nop"),
];

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

const THIRD_PARTY: &str = "THIRD-PARTY-LICENSES.md";

#[derive(Subcommand)]
pub enum LicenseCmd {
    /// Generate the third-party license notices with cargo-about
    Generate {
        /// Artifact to generate the notices for
        #[arg(value_enum, default_value_t = GenTarget::All)]
        target: GenTarget,
    },
    /// Check the dependency licenses of every workspace with cargo-deny
    Check,
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
    }
}

fn generate(root: &Path, target: GenTarget) -> Result<()> {
    match target {
        GenTarget::All => {
            generate_python(root)?;
            generate_csharp(root)?;
            generate_unity(root)?;
            generate_console(root)?;
        }
        GenTarget::Python => generate_python(root)?,
        GenTarget::Csharp => generate_csharp(root)?,
        GenTarget::Unity => generate_unity(root)?,
        GenTarget::Console => generate_console(root)?,
        GenTarget::Simulator => generate_simulator(root)?,
    }
    println!("license generate: done");
    Ok(())
}

fn check(root: &Path) -> Result<()> {
    check_bundled_license(root)?;
    if !on_path("cargo-deny") {
        bail!("`cargo-deny` is required");
    }
    let config = root.join("deny.toml");
    let config = config.to_string_lossy().into_owned();
    for ws in DENY_WORKSPACES {
        let dir = root.join(ws);
        println!("== cargo-deny check licenses: {} ==", dir.display());
        run(
            "cargo",
            ["deny", "--config", &config, "check", "licenses"],
            &dir,
        )?;
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
        for package in publishable_members(&ws_dir)? {
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

fn ensure_about() -> Result<()> {
    if !on_path("cargo-about") {
        bail!("`cargo-about` is required");
    }
    Ok(())
}

pub fn generate_python(root: &Path) -> Result<()> {
    ensure_about()?;
    let mit_license = root.join("LICENSE");

    let py = root.join("bindings/python");
    for wheel in PY_WHEELS {
        let dir = py.join(wheel);
        about(root, &dir.join("Cargo.toml"), &dir.join(THIRD_PARTY))?;
        copy(&mit_license, &dir.join("LICENSE"))?;
    }
    Ok(())
}

pub fn generate_csharp(root: &Path) -> Result<()> {
    ensure_about()?;

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
    Ok(())
}

pub fn generate_unity(root: &Path) -> Result<()> {
    ensure_about()?;
    let mit_license = root.join("LICENSE");

    let ffi = root.join("bindings/ffi");
    let unity = root.join("bindings/unity");
    for (pkg, krate) in UNITY_PACKAGES {
        let dir = unity.join(pkg);
        about(
            root,
            &ffi.join(krate).join("Cargo.toml"),
            &dir.join(THIRD_PARTY),
        )?;
        copy(&mit_license, &dir.join("LICENSE.md"))?;
    }
    Ok(())
}

pub fn generate_console(root: &Path) -> Result<()> {
    ensure_about()?;
    let console = root.join("console");
    let out = console.join(THIRD_PARTY);
    about(root, &console.join("Cargo.toml"), &out)?;

    generate_simulator(root)?;
    let simulator_notice = root.join("simulator").join(THIRD_PARTY);
    let simulator = std::fs::read_to_string(&simulator_notice)
        .with_context(|| format!("reading {}", simulator_notice.display()))?;

    let firmware_tmp = console.join(".third-party-firmware.md");
    about(
        root,
        &root.join("tools").join("firmware").join("Cargo.toml"),
        &firmware_tmp,
    )?;
    let firmware = std::fs::read_to_string(&firmware_tmp)
        .with_context(|| format!("reading {}", firmware_tmp.display()))?;

    let appliance_tmp = console.join(".third-party-appliance.md");
    about(
        root,
        &root.join("appliance").join("cli").join("Cargo.toml"),
        &appliance_tmp,
    )?;
    let appliance = std::fs::read_to_string(&appliance_tmp)
        .with_context(|| format!("reading {}", appliance_tmp.display()))?;

    let mut combined =
        std::fs::read_to_string(&out).with_context(|| format!("reading {}", out.display()))?;
    combined.push_str("\n\n---\n\n# Simulator dependencies\n\n");
    combined.push_str(&simulator);
    combined.push_str("\n\n---\n\n# Firmware tool dependencies\n\n");
    combined.push_str(&firmware);
    combined.push_str("\n\n---\n\n# Appliance CLI dependencies\n\n");
    combined.push_str(&appliance);
    std::fs::write(&out, combined).with_context(|| format!("writing {}", out.display()))?;
    std::fs::remove_file(&firmware_tmp).ok();
    std::fs::remove_file(&appliance_tmp).ok();
    Ok(())
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
