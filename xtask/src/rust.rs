use std::path::Path;

use anyhow::{Result, bail};
use clap::Subcommand;

use crate::util::{on_path, publish_workspace, publishable_members, run, run_built_bin};

const PCAP_PACKAGES: &[&str] = &[
    "autd3-rs-perftest",
    "autd3-rs-synctune",
    "autd3-rs-examples",
    "autd3-rs-firmware-test",
];

#[derive(Subcommand)]
pub enum RustCmd {
    /// Build the `crates/` workspace
    Build,
    /// Run the `crates/` workspace tests
    Test {
        /// Skip the packages that need a pcap runtime (Npcap/WinPcap, libpcap)
        #[arg(long)]
        no_pcap: bool,
    },
    /// Clippy the `crates/` workspace
    Lint,
    /// Rustfmt the `crates/` workspace
    Format {
        /// Rewrite the files instead of only checking them
        #[arg(long)]
        fix: bool,
    },
    /// Check the `crates/` workspace API for SemVer violations with cargo-semver-checks
    Semver {
        /// Released version to compare against (defaults to the latest one on crates.io)
        #[arg(long)]
        baseline: Option<String>,
    },
    /// Publish the `crates/` workspace to crates.io, skipping already-published versions
    Publish {
        /// Run every check without uploading
        #[arg(long)]
        dry_run: bool,
    },
    /// Build and run an example from `examples/`
    Example {
        /// Example binary name (one binary per feature; see `examples/`)
        name: String,
        /// Build the dev profile instead of release
        #[arg(long)]
        debug: bool,
        /// Do not wrap the run in `sudo`
        #[arg(long)]
        no_sudo: bool,
        /// Arguments forwarded to the example
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

pub fn run_rust(root: &Path, cmd: &RustCmd) -> Result<()> {
    match cmd {
        RustCmd::Build => {
            let args = vec![
                "build",
                "--workspace",
                "--all-targets",
                "--features",
                "autd3-rs/legacy",
            ];
            run("cargo", args, root)
        }
        RustCmd::Test { no_pcap } => {
            let mut args = vec![
                "test",
                "--workspace",
                "--lib",
                "--bins",
                "--tests",
                "--features",
                "autd3-rs/legacy",
            ];
            if *no_pcap {
                args.extend(PCAP_PACKAGES.iter().flat_map(|pkg| ["--exclude", *pkg]));
            }
            run("cargo", args, root)
        }
        RustCmd::Lint => {
            let mut args = vec![
                "clippy",
                "--workspace",
                "--all-targets",
                "--features",
                "autd3-rs/legacy",
            ];
            args.extend(["--", "-D", "warnings"]);
            run("cargo", args, root)?;

            let default_feature_args = vec![
                "clippy",
                "-p",
                "autd3-rs",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ];
            run("cargo", default_feature_args, root)?;

            let no_discovery_args = vec![
                "clippy",
                "-p",
                "autd3-rs-link-remote",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ];
            run("cargo", no_discovery_args, root)?;

            let no_parallel_args = vec![
                "clippy",
                "-p",
                "autd3-rs-pattern-holo",
                "--no-default-features",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ];
            run("cargo", no_parallel_args, root)
        }
        RustCmd::Format { fix } => {
            let mut args = vec!["fmt", "--all"];
            if !*fix {
                args.push("--");
                args.push("--check");
            }
            run("cargo", args, root)
        }
        RustCmd::Semver { baseline } => run_semver(root, baseline.as_deref()),
        RustCmd::Publish { dry_run } => publish_workspace(root, *dry_run),
        RustCmd::Example {
            name,
            debug,
            no_sudo,
            args,
        } => run_example(root, name, *debug, *no_sudo, args),
    }
}

fn run_semver(root: &Path, baseline: Option<&str>) -> Result<()> {
    if !on_path("cargo-semver-checks") {
        bail!("`cargo-semver-checks` is required");
    }
    let mut args = vec!["semver-checks".to_string()];
    for package in publishable_members(root)? {
        args.push("--package".to_string());
        args.push(package.name().to_string());
    }
    if let Some(baseline) = baseline {
        args.push("--baseline-version".to_string());
        args.push(baseline.to_string());
    }
    run("cargo", args, root)
}

fn run_example(root: &Path, name: &str, debug: bool, no_sudo: bool, args: &[String]) -> Result<()> {
    let mut build_args: Vec<&str> = vec!["build", "-p", "autd3-rs-examples", "--bin", name];
    if !debug {
        build_args.push("--release");
    }
    run("cargo", build_args, root)?;

    let profile = if debug { "debug" } else { "release" };
    let bin = root.join("target").join(profile).join(name);
    run_built_bin(&bin, args, no_sudo, root)
}
