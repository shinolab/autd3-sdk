use std::path::Path;

use anyhow::Result;
use clap::Subcommand;

use crate::util::run;

#[derive(Subcommand)]
pub enum EmulatorCmd {
    Build,
    Test,
    Lint,
    Format {
        #[arg(long)]
        fix: bool,
    },
}

pub fn run_emulator(root: &Path, cmd: &EmulatorCmd) -> Result<()> {
    let dir = root.join("emulator");
    match cmd {
        EmulatorCmd::Build => run("cargo", ["build", "--workspace", "--all-targets"], &dir),
        EmulatorCmd::Test => run("cargo", ["test", "--workspace", "--all-targets"], &dir),
        EmulatorCmd::Lint => run(
            "cargo",
            ["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
            &dir,
        ),
        EmulatorCmd::Format { fix } => {
            let mut args = vec!["fmt", "--all"];
            if !*fix {
                args.push("--");
                args.push("--check");
            }
            run("cargo", args, &dir)
        }
    }
}
