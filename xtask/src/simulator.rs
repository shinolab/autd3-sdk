use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::Subcommand;

use crate::clean::{CleanArgs, Cleaner};
use crate::util::{cargo_bin, cargo_build_args, cargo_clippy, cargo_fmt, run, run_cargo};

pub(crate) fn build_simulator(root: &Path, debug: bool, target: Option<&str>) -> Result<PathBuf> {
    let sim = root.join("simulator");
    run_cargo(cargo_build_args("autd3-rs-simulator", target, debug), &sim)?;
    Ok(cargo_bin(&sim, target, debug, "autd3-simulator"))
}

#[derive(Subcommand)]
pub(crate) enum SimulatorCmd {
    #[command(about = "Build the simulator")]
    Build {
        #[arg(long, help = "Build the dev profile instead of release")]
        debug: bool,
        #[arg(
            long,
            help = "Generate the third-party license notices with cargo-about (slow; use it for release artifacts)"
        )]
        license: bool,
    },
    #[command(about = "Run the simulator tests")]
    Test,
    #[command(about = "Clippy the simulator")]
    Lint,
    #[command(about = "Rustfmt the simulator")]
    Format {
        #[arg(long, help = "Rewrite the files instead of only checking them")]
        fix: bool,
    },
    #[command(about = "Build and run the simulator")]
    Run {
        #[arg(long, help = "Build the dev profile instead of release")]
        debug: bool,
        #[arg(
            long,
            default_value_t = 44336,
            help = "UDP port the emulated devices listen on"
        )]
        port: u16,
    },
    #[command(about = "Remove the simulator build outputs")]
    Clean(CleanArgs),
}

pub(crate) fn run_simulator(root: &Path, cmd: &SimulatorCmd) -> Result<()> {
    let sim = root.join("simulator");
    match cmd {
        SimulatorCmd::Build { debug, license } => {
            crate::license::generate_simulator(root, *license)?;
            build_simulator(root, *debug, None).map(|_| ())
        }
        SimulatorCmd::Test => run("cargo", ["test", "--bins", "--tests"], &sim),
        SimulatorCmd::Lint => cargo_clippy(&sim, &["--all-targets"]),
        SimulatorCmd::Format { fix } => cargo_fmt(&sim, &[], *fix),
        SimulatorCmd::Run { debug, port } => {
            let bin = build_simulator(root, *debug, None)?;
            run(
                &bin.to_string_lossy(),
                ["--port".to_string(), port.to_string()],
                &sim,
            )
        }
        SimulatorCmd::Clean(args) => crate::clean::scope(root, *args, clean),
    }
}

pub(crate) fn clean(cleaner: &mut Cleaner) -> Result<()> {
    cleaner.paths(&["simulator/target", "simulator/THIRD-PARTY-LICENSES.md"])
}
