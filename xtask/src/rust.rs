use std::path::Path;

use anyhow::Result;
use clap::Subcommand;

use crate::util::{run, run_built_bin};

/// Packages whose test binaries link a pcap runtime
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
            let args = vec!["build", "--workspace", "--all-targets"];
            run("cargo", args, root)
        }
        RustCmd::Test { no_pcap } => {
            let mut args = vec!["test", "--workspace", "--all-targets"];
            if *no_pcap {
                args.extend(PCAP_PACKAGES.iter().flat_map(|pkg| ["--exclude", *pkg]));
            }
            run("cargo", args, root)
        }
        RustCmd::Lint => {
            let mut args = vec!["clippy", "--workspace", "--all-targets"];
            args.extend(["--", "-D", "warnings"]);
            run("cargo", args, root)
        }
        RustCmd::Format { fix } => {
            let mut args = vec!["fmt", "--all"];
            if !*fix {
                args.push("--");
                args.push("--check");
            }
            run("cargo", args, root)
        }
        RustCmd::Example {
            name,
            debug,
            no_sudo,
            args,
        } => run_example(root, name, *debug, *no_sudo, args),
    }
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
