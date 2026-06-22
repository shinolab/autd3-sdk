use std::path::Path;

use anyhow::Result;
use clap::Subcommand;

use crate::util::run;

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
        RustCmd::Build => run("cargo", ["build", "--workspace", "--all-targets"], root),
        RustCmd::Test => run("cargo", ["test", "--workspace", "--all-targets"], root),
        RustCmd::Lint => run(
            "cargo",
            [
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
            root,
        ),
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
