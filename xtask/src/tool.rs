use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::Subcommand;

use crate::util::{cargo_bin, cargo_build_args, run};

#[derive(Subcommand)]
pub enum ToolCmd {
    /// Measure the communication performance
    Perftest {
        /// Build the dev profile instead of release
        #[arg(long)]
        debug: bool,
        /// Do not wrap the run in `sudo`
        #[arg(long)]
        no_sudo: bool,
        /// Enable the `mem-profile` feature (allocation size histogram)
        #[arg(long)]
        mem_profile: bool,
        /// Arguments forwarded to the tool
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Run the interactive firmware acceptance tests against a real device
    FirmwareTest {
        /// Build the dev profile instead of release
        #[arg(long)]
        debug: bool,
        /// Arguments forwarded to the tool
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Update the CPU / FPGA firmware over UDP (no J-Link / Vivado) and reboot the devices
    FirmwareOta {
        /// Build the dev profile instead of release
        #[arg(long)]
        debug: bool,
        /// Arguments forwarded to the tool (the flash image path or `--version X.Y.Z` first)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

pub fn run_tool(root: &Path, cmd: ToolCmd) -> Result<()> {
    match cmd {
        ToolCmd::Perftest {
            debug,
            no_sudo,
            mem_profile,
            args,
        } => {
            let features: &[&str] = if mem_profile { &["mem-profile"] } else { &[] };
            let bin = build_bin(root, "autd3-rs-perftest", debug, features)?;
            run_privileged(&bin, &args, no_sudo, root)
        }
        ToolCmd::FirmwareTest { debug, args } => {
            let bin = build_bin(root, "autd3-rs-firmware-test", debug, &[])?;
            run(&bin.to_string_lossy(), &args, root)
        }
        ToolCmd::FirmwareOta { debug, args } => {
            let bin = build_bin(root, "autd3-rs-firmware-ota", debug, &[])?;
            run(&bin.to_string_lossy(), &args, root)
        }
    }
}

fn build_bin(root: &Path, pkg: &str, debug: bool, features: &[&str]) -> Result<PathBuf> {
    let mut build_args = cargo_build_args(pkg, None, debug);
    let features_arg = features.join(",");
    if !features.is_empty() {
        build_args.push("--features");
        build_args.push(&features_arg);
    }
    run("cargo", build_args, root)?;
    Ok(cargo_bin(root, None, debug, pkg))
}

#[cfg(target_os = "linux")]
fn setcap_program() -> Option<String> {
    if crate::util::on_path("setcap") {
        return Some("setcap".to_owned());
    }
    ["/usr/bin/setcap", "/usr/sbin/setcap", "/sbin/setcap"]
        .into_iter()
        .find(|path| Path::new(path).is_file())
        .map(str::to_owned)
}

const RUN_CAPABILITIES: &str = "cap_sys_nice+ep";

#[cfg(target_os = "linux")]
fn grant_capabilities(bin: &Path) -> bool {
    let Some(setcap) = setcap_program() else {
        return false;
    };
    std::process::Command::new("sudo")
        .args(["-n", &setcap, RUN_CAPABILITIES])
        .arg(bin)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(not(target_os = "linux"))]
fn grant_capabilities(_bin: &Path) -> bool {
    false
}

fn run_privileged(bin: &Path, args: &[String], no_sudo: bool, cwd: &Path) -> Result<()> {
    let bin_str = bin.to_string_lossy().into_owned();
    if no_sudo || !cfg!(unix) {
        return run(&bin_str, args.iter().map(String::as_str), cwd);
    }
    if grant_capabilities(bin) {
        println!("granted {RUN_CAPABILITIES} to {bin_str}; running without sudo");
        return run(&bin_str, args.iter().map(String::as_str), cwd);
    }
    let mut sudo_args: Vec<String> = Vec::with_capacity(args.len() + 2);
    if let Ok(log) = std::env::var("RUST_LOG") {
        sudo_args.push(format!("RUST_LOG={log}"));
    }
    sudo_args.push(bin_str);
    sudo_args.extend(args.iter().cloned());
    run("sudo", sudo_args.iter().map(String::as_str), cwd)
}
