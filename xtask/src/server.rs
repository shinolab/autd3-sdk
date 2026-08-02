use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Subcommand;

use crate::util::{
    capture, cargo_bin, cargo_build_args, ensure_rust_target, run_built_bin, run_cargo,
};

const PACKAGE: &str = "autd3-remote-server";
const APPLIANCE_TARGET: &str = "aarch64-unknown-linux-musl";

const FORBIDDEN_CRATES: &[&str] = &["autd3-rs-link-soem"];

const DIST_FILES: &[(&str, bool)] = &[
    ("remote-server.toml", false),
    ("autd3-remote-server.service", false),
    ("cmdline.txt.example", false),
    ("INSTALL.md", false),
    ("sudoers-autd3-admin", false),
    ("tune-appliance.sh", true),
    ("run-server", true),
    ("autd3-admin", true),
];

#[derive(Subcommand)]
pub enum ServerCmd {
    /// Build the appliance server
    Build {
        /// Build the dev profile instead of release
        #[arg(long)]
        debug: bool,
        /// Cross-compile for a target triple (defaults to the host)
        #[arg(long)]
        target: Option<String>,
    },
    /// Cross-compile the appliance server for the Raspberry Pi 4 (aarch64)
    Cross,
    /// Build and run the appliance server locally
    Run {
        /// Build the dev profile instead of release
        #[arg(long)]
        debug: bool,
        /// Do not wrap the run in `sudo`
        #[arg(long)]
        no_sudo: bool,
        /// Arguments forwarded to the server
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Zip the server, the systemd unit and the tuning scripts into `appliance/server/bundle/`
    Bundle {
        /// Target triple to bundle (defaults to the Raspberry Pi 4 target)
        #[arg(long)]
        target: Option<String>,
    },
    /// Fail if a GPL crate reaches the distributed appliance binary
    CheckLicense,
}

pub fn run_server(root: &Path, cmd: &ServerCmd) -> Result<()> {
    match cmd {
        ServerCmd::Build { debug, target } => build(root, target.as_deref(), *debug).map(|_| ()),
        ServerCmd::Cross => cross_build(root).map(|_| ()),
        ServerCmd::Run {
            debug,
            no_sudo,
            args,
        } => {
            let bin = build(root, None, *debug)?;
            run_built_bin(&bin, args, *no_sudo, root)
        }
        ServerCmd::Bundle { target } => bundle(root, target.as_deref()),
        ServerCmd::CheckLicense => check_license(root),
    }
}

fn build(root: &Path, target: Option<&str>, debug: bool) -> Result<PathBuf> {
    check_license(root)?;
    if let Some(target) = target {
        ensure_rust_target(target)?;
    }
    let mut args: Vec<String> = cargo_build_args(PACKAGE, target, debug)
        .into_iter()
        .map(ToOwned::to_owned)
        .collect();
    if target == Some(APPLIANCE_TARGET)
        && let Some(lld) = rust_lld(root)
    {
        args.push("--config".to_owned());
        args.push(format!(
            "target.{APPLIANCE_TARGET}.linker={:?}",
            lld.to_string_lossy(),
        ));
    }
    run_cargo(args, root)?;
    Ok(cargo_bin(root, target, debug, PACKAGE))
}

fn rust_lld(root: &Path) -> Option<PathBuf> {
    let sysroot = capture("rustc", &["--print", "sysroot"], root).ok()?;
    let host = capture("rustc", &["-vV"], root)
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("host: ").map(ToOwned::to_owned))?;
    let lld = Path::new(&sysroot)
        .join("lib/rustlib")
        .join(host)
        .join("bin")
        .join(crate::util::exe_name("rust-lld"));
    lld.is_file().then_some(lld)
}

pub fn cross_build(root: &Path) -> Result<PathBuf> {
    build(root, Some(APPLIANCE_TARGET), false)
}

fn check_license(root: &Path) -> Result<()> {
    let tree = capture(
        "cargo",
        &[
            "tree", "-p", PACKAGE, "--edges", "normal", "--prefix", "none",
        ],
        root,
    )?;
    let found: Vec<&str> = FORBIDDEN_CRATES
        .iter()
        .filter(|name| {
            tree.lines()
                .any(|line| line.split_whitespace().next() == Some(*name))
        })
        .copied()
        .collect();
    if !found.is_empty() {
        bail!(
            "{} depends on {}, whose license (GPL-3.0) would follow the distributed \
             appliance image. The appliance drives the bus with the MIT echocat link",
            PACKAGE,
            found.join(", "),
        );
    }
    Ok(())
}

fn bundle(root: &Path, target: Option<&str>) -> Result<()> {
    let target = target.unwrap_or(APPLIANCE_TARGET);
    let bin = build(root, Some(target), false)?;

    let component = crate::component::COMPONENTS
        .iter()
        .find(|c| c.name == "software")
        .context("no `software` component")?;
    let version = component.current_version(root)?;

    let dist = root.join("appliance/server/dist");
    let out = root.join("appliance/server/bundle");
    std::fs::create_dir_all(&out).with_context(|| format!("creating {}", out.display()))?;
    let archive = out.join(format!("{PACKAGE}-v{version}-{target}.zip"));

    let file = std::fs::File::create(&archive)
        .with_context(|| format!("creating {}", archive.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let plain = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);
    let executable = plain.unix_permissions(0o755);

    add(&mut zip, PACKAGE, &bin, executable)?;
    for (name, exec) in DIST_FILES {
        let options = if *exec { executable } else { plain };
        add(&mut zip, name, &dist.join(name), options)?;
    }
    zip.finish()?;

    println!("appliance bundle: {}", archive.display());
    Ok(())
}

fn add(
    zip: &mut zip::ZipWriter<std::fs::File>,
    name: &str,
    path: &Path,
    options: zip::write::SimpleFileOptions,
) -> Result<()> {
    zip.start_file(name, options)?;
    let mut src =
        std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    std::io::copy(&mut src, zip).with_context(|| format!("adding {name}"))?;
    Ok(())
}
