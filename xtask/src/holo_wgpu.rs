use std::path::Path;

use anyhow::Result;
use clap::Subcommand;

use crate::util::{publish_workspace, run};

#[derive(Subcommand)]
pub enum HoloWgpuCmd {
    /// Build the wgpu holo backend
    Build,
    /// Run the wgpu holo backend tests (the GPU-dependent ones skip without an adapter)
    Test,
    /// Run the ignored measurement tests (needs a GPU and a quiet machine)
    Bench {
        /// Only run tests whose name contains this string
        filter: Option<String>,
    },
    /// Clippy the wgpu holo backend
    Lint,
    /// Rustfmt the wgpu holo backend
    Format {
        /// Rewrite the files instead of only checking them
        #[arg(long)]
        fix: bool,
    },
    /// Publish the wgpu holo backend to crates.io, skipping already-published versions
    Publish {
        /// Run every check without uploading
        #[arg(long)]
        dry_run: bool,
    },
}

pub fn run_holo_wgpu(root: &Path, cmd: &HoloWgpuCmd) -> Result<()> {
    let dir = root.join("extras").join("autd3-rs-pattern-holo-wgpu");
    match cmd {
        HoloWgpuCmd::Build => run("cargo", vec!["build", "--all-targets"], &dir),
        HoloWgpuCmd::Test => run("cargo", vec!["test", "--release"], &dir),
        HoloWgpuCmd::Bench { filter } => {
            let mut args = vec!["test", "--release"];
            if let Some(filter) = filter {
                args.push(filter);
            }
            args.extend(["--", "--ignored", "--nocapture", "--test-threads=1"]);
            run("cargo", args, &dir)
        }
        HoloWgpuCmd::Lint => run(
            "cargo",
            vec!["clippy", "--all-targets", "--", "-D", "warnings"],
            &dir,
        ),
        HoloWgpuCmd::Format { fix } => {
            let mut args = vec!["fmt"];
            if !*fix {
                args.push("--");
                args.push("--check");
            }
            run("cargo", args, &dir)
        }
        HoloWgpuCmd::Publish { dry_run } => publish_workspace(&dir, *dry_run),
    }
}
