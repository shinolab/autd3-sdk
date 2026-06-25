use std::path::Path;

use anyhow::Result;
use clap::Subcommand;

use crate::util::{macos_soem_excludes, run};

#[derive(Subcommand)]
pub enum RustCmd {
    Build,
    Test,
    Lint,
    Format {
        #[arg(long)]
        fix: bool,
    },
}

pub fn run_rust(root: &Path, cmd: &RustCmd) -> Result<()> {
    match cmd {
        RustCmd::Build => {
            let mut args = vec!["build", "--workspace", "--all-targets"];
            args.extend(rust_macos_excludes());
            run("cargo", args, root)
        }
        RustCmd::Test => {
            let mut args = vec!["test", "--workspace", "--all-targets"];
            args.extend(rust_macos_excludes());
            run("cargo", args, root)
        }
        RustCmd::Lint => {
            let mut args = vec!["clippy", "--workspace", "--all-targets"];
            args.extend(rust_macos_excludes());
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
    }
}

fn rust_macos_excludes() -> Vec<&'static str> {
    macos_soem_excludes(&[
        "autd3-rs-link-soem",
        "autd3-rs-perftest",
        "autd3-rs-patternsoak",
        "autd3-rs-synctune",
    ])
}
