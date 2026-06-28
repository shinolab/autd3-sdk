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
    Example {
        name: String,
        #[arg(long)]
        debug: bool,
        #[arg(long)]
        no_plot: bool,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
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
