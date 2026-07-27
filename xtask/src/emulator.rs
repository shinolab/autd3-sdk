use std::path::Path;

use anyhow::Result;
use clap::Subcommand;

use crate::util::{publish_workspace, run};

#[derive(Subcommand)]
pub enum EmulatorCmd {
    /// Build the emulator workspace
    Build {
        /// Enable the GPU-accelerated field computation
        #[arg(long)]
        gpu: bool,
    },
    /// Run the emulator workspace tests
    Test {
        /// Enable the GPU-accelerated field computation
        #[arg(long)]
        gpu: bool,
    },
    /// Clippy the emulator workspace
    Lint {
        /// Enable the GPU-accelerated field computation
        #[arg(long)]
        gpu: bool,
    },
    /// Rustfmt the emulator workspace
    Format {
        /// Rewrite the files instead of only checking them
        #[arg(long)]
        fix: bool,
    },
    /// Publish the emulator workspace to crates.io, skipping already-published versions
    Publish {
        /// Run every check without uploading
        #[arg(long)]
        dry_run: bool,
    },
    /// Build and run an emulator example
    Example {
        /// Example binary name
        name: String,
        /// Build the dev profile instead of release
        #[arg(long)]
        debug: bool,
        /// Do not plot the computed field
        #[arg(long)]
        no_plot: bool,
        /// Arguments forwarded to the example
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

pub fn run_emulator(root: &Path, cmd: &EmulatorCmd) -> Result<()> {
    let dir = root.join("emulator");
    let feature_args = |gpu: bool| -> Vec<&str> {
        if gpu {
            vec!["--features", "autd3-rs-emulator/gpu"]
        } else {
            vec![]
        }
    };
    match cmd {
        EmulatorCmd::Build { gpu } => {
            let mut args = vec!["build", "--workspace", "--all-targets"];
            args.extend(feature_args(*gpu));
            run("cargo", args, &dir)
        }
        EmulatorCmd::Test { gpu } => {
            let mut args = vec!["test", "--workspace", "--all-targets"];
            args.extend(feature_args(*gpu));
            run("cargo", args, &dir)
        }
        EmulatorCmd::Lint { gpu } => {
            let mut args = vec!["clippy", "--workspace", "--all-targets"];
            args.extend(feature_args(*gpu));
            args.extend(["--", "-D", "warnings"]);
            run("cargo", args, &dir)
        }
        EmulatorCmd::Format { fix } => {
            let mut args = vec![
                "fmt",
                "-p",
                "autd3-rs-emulator",
                "-p",
                "autd3-rs-emulator-examples",
            ];
            if !*fix {
                args.push("--");
                args.push("--check");
            }
            run("cargo", args, &dir)
        }
        EmulatorCmd::Publish { dry_run } => publish_workspace(&dir, *dry_run),
        EmulatorCmd::Example {
            name,
            debug,
            no_plot,
            args,
        } => {
            let mut cargo_args = vec![
                "run",
                "-p",
                "autd3-rs-emulator-examples",
                "--bin",
                name.as_str(),
            ];
            if !*debug {
                cargo_args.push("--release");
            }
            if *no_plot || !args.is_empty() {
                cargo_args.push("--");
                if *no_plot {
                    cargo_args.push("--no-plot");
                }
                cargo_args.extend(args.iter().map(String::as_str));
            }
            run("cargo", cargo_args, &dir)
        }
    }
}
