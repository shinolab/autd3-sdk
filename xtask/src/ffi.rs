use std::path::Path;

use anyhow::Result;
use clap::Subcommand;

use crate::util::run;

const SOEM_CRATE: &str = "autd3-ffi-link-soem";

#[derive(Subcommand)]
pub enum FfiCmd {
    Build {
        #[arg(long)]
        debug: bool,
        #[arg(long)]
        soem: bool,
    },
    Lint,
    Format {
        #[arg(long)]
        fix: bool,
    },
}

pub fn run_ffi(root: &Path, cmd: FfiCmd) -> Result<()> {
    let dir = root.join("bindings").join("ffi");
    match cmd {
        FfiCmd::Build { debug, soem } => {
            let mut args = vec!["build", "--workspace"];
            if !soem {
                args.push("--exclude");
                args.push(SOEM_CRATE);
            }
            if !debug {
                args.push("--release");
            }
            run("cargo", args, &dir)
        }
        FfiCmd::Lint => run(
            "cargo",
            [
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
            &dir,
        ),
        FfiCmd::Format { fix } => {
            let mut args = vec!["fmt", "--all"];
            if !fix {
                args.push("--");
                args.push("--check");
            }
            run("cargo", args, &dir)
        }
    }
}
