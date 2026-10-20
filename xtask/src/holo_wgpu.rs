use std::path::Path;

use anyhow::Result;
use clap::Subcommand;

use crate::clean::{CleanArgs, Cleaner};
use crate::util::{cargo_clippy, cargo_fmt, publish_workspace, run};

#[derive(Subcommand)]
pub(crate) enum HoloWgpuCmd {
    #[command(about = "Build the wgpu holo backend")]
    Build,
    #[command(
        about = "Run the wgpu holo backend tests (the GPU-dependent ones skip without an adapter)"
    )]
    Test,
    #[command(about = "Run the ignored measurement tests (needs a GPU and a quiet machine)")]
    Bench {
        #[arg(help = "Only run tests whose name contains this string")]
        filter: Option<String>,
    },
    #[command(about = "Clippy the wgpu holo backend")]
    Lint,
    #[command(about = "Rustfmt the wgpu holo backend")]
    Format {
        #[arg(long, help = "Rewrite the files instead of only checking them")]
        fix: bool,
    },
    #[command(
        about = "Publish the wgpu holo backend to crates.io, skipping already-published versions"
    )]
    Publish {
        #[arg(long, help = "Run every check without uploading")]
        dry_run: bool,
    },
    #[command(about = "Remove the wgpu holo backend build outputs")]
    Clean(CleanArgs),
}

pub(crate) fn run_holo_wgpu(root: &Path, cmd: &HoloWgpuCmd) -> Result<()> {
    let dir = root.join("extras").join("autd3-rs-pattern-holo-wgpu");
    match cmd {
        HoloWgpuCmd::Build => run("cargo", vec!["build", "--all-targets"], &dir),
        HoloWgpuCmd::Test => run("cargo", vec!["test", "--release"], &dir),
        HoloWgpuCmd::Bench { filter } => bench(&dir, filter.as_deref()),
        HoloWgpuCmd::Lint => cargo_clippy(&dir, &["--all-targets"]),
        HoloWgpuCmd::Format { fix } => cargo_fmt(&dir, &[], *fix),
        HoloWgpuCmd::Publish { dry_run } => publish_workspace(&dir, *dry_run),
        HoloWgpuCmd::Clean(args) => crate::clean::scope(root, *args, clean),
    }
}

fn bench(dir: &Path, filter: Option<&str>) -> Result<()> {
    let mut args = vec!["test", "--release"];
    if let Some(filter) = filter {
        args.push(filter);
    }
    args.extend(["--", "--ignored", "--nocapture", "--test-threads=1"]);
    run("cargo", args, dir)
}

pub(crate) fn clean(cleaner: &mut Cleaner) -> Result<()> {
    cleaner.path("extras/autd3-rs-pattern-holo-wgpu/target")
}
