use std::path::Path;

use anyhow::Result;
use clap::Subcommand;

use crate::clean::{CleanArgs, Cleaner};
use crate::util::{cargo_clippy, cargo_fmt, publish_workspace, run};

#[derive(Subcommand)]
pub(crate) enum EmulatorCmd {
    #[command(about = "Build the emulator workspace")]
    Build {
        #[arg(long, help = "Enable the GPU-accelerated field computation")]
        gpu: bool,
    },
    #[command(about = "Run the emulator workspace tests")]
    Test {
        #[arg(long, help = "Enable the GPU-accelerated field computation")]
        gpu: bool,
    },
    #[command(about = "Clippy the emulator workspace")]
    Lint {
        #[arg(long, help = "Enable the GPU-accelerated field computation")]
        gpu: bool,
    },
    #[command(about = "Rustfmt the emulator workspace")]
    Format {
        #[arg(long, help = "Rewrite the files instead of only checking them")]
        fix: bool,
    },
    #[command(
        about = "Publish the emulator workspace to crates.io, skipping already-published versions"
    )]
    Publish {
        #[arg(long, help = "Run every check without uploading")]
        dry_run: bool,
    },
    #[command(about = "Build and run an emulator example")]
    Example {
        #[arg(help = "Example binary name")]
        name: String,
        #[arg(long, help = "Build the dev profile instead of release")]
        debug: bool,
        #[arg(long, help = "Do not plot the computed field")]
        no_plot: bool,
        #[arg(
            trailing_var_arg = true,
            allow_hyphen_values = true,
            help = "Arguments forwarded to the example"
        )]
        args: Vec<String>,
    },
    #[command(about = "Remove the emulator workspace build outputs")]
    Clean(CleanArgs),
}

pub(crate) fn run_emulator(root: &Path, cmd: &EmulatorCmd) -> Result<()> {
    let dir = root.join("extras").join("autd3-rs-emulator");
    match cmd {
        EmulatorCmd::Build { gpu } => build(&dir, *gpu),
        EmulatorCmd::Test { gpu } => test(&dir, *gpu),
        EmulatorCmd::Lint { gpu } => lint(&dir, *gpu),
        EmulatorCmd::Format { fix } => cargo_fmt(
            &dir,
            &[
                "-p",
                "autd3-rs-emulator",
                "-p",
                "autd3-rs-emulator-examples",
            ],
            *fix,
        ),
        EmulatorCmd::Publish { dry_run } => publish_workspace(&dir, *dry_run),
        EmulatorCmd::Example {
            name,
            debug,
            no_plot,
            args,
        } => example(&dir, name, *debug, *no_plot, args),
        EmulatorCmd::Clean(args) => crate::clean::scope(root, *args, clean),
    }
}

pub(crate) fn clean(cleaner: &mut Cleaner) -> Result<()> {
    cleaner.path("extras/autd3-rs-emulator/target")
}

fn feature_args(gpu: bool) -> Vec<&'static str> {
    if gpu {
        vec!["--features", "autd3-rs-emulator/gpu"]
    } else {
        vec![]
    }
}

fn build(dir: &Path, gpu: bool) -> Result<()> {
    let mut args = vec!["build", "--workspace", "--all-targets"];
    args.extend(feature_args(gpu));
    run("cargo", args, dir)
}

fn test(dir: &Path, gpu: bool) -> Result<()> {
    let mut args = vec!["test", "--workspace", "--all-targets"];
    args.extend(feature_args(gpu));
    run("cargo", args, dir)
}

fn lint(dir: &Path, gpu: bool) -> Result<()> {
    let mut args = vec!["--workspace", "--all-targets"];
    args.extend(feature_args(gpu));
    cargo_clippy(dir, &args)
}

fn example(dir: &Path, name: &str, debug: bool, no_plot: bool, args: &[String]) -> Result<()> {
    let mut cargo_args = vec!["run", "-p", "autd3-rs-emulator-examples", "--bin", name];
    if !debug {
        cargo_args.push("--release");
    }
    if no_plot || !args.is_empty() {
        cargo_args.push("--");
        if no_plot {
            cargo_args.push("--no-plot");
        }
        cargo_args.extend(args.iter().map(String::as_str));
    }
    run("cargo", cargo_args, dir)
}
