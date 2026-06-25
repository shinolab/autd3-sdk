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
        RustCmd::Build => {
            let args = vec!["build", "--workspace", "--all-targets"];
            run("cargo", args, root)
        }
        RustCmd::Test => {
            let mut args = vec!["test", "--workspace", "--all-targets"];
            args.extend(windows_test_excludes());
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
    }
}

fn windows_test_excludes() -> Vec<&'static str> {
    if cfg!(target_os = "windows") {
        [
            "autd3-rs-perftest",
            "autd3-rs-patternsoak",
            "autd3-rs-synctune",
            "autd3-rs-examples",
        ]
        .iter()
        .flat_map(|pkg| ["--exclude", *pkg])
        .collect()
    } else {
        Vec::new()
    }
}
