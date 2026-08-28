use std::path::Path;

use anyhow::Result;
use clap::Subcommand;

use crate::util::{cargo_fmt_packages, run};

#[derive(Subcommand)]
pub enum FfiCmd {
    /// Build the C ABI cdylibs
    Build {
        /// Build the dev profile instead of release
        #[arg(long)]
        debug: bool,
    },
    /// Test the FFI workspace
    Test,
    /// Clippy the FFI workspace
    Lint,
    /// Rustfmt the FFI workspace
    Format {
        /// Rewrite the files instead of only checking them
        #[arg(long)]
        fix: bool,
    },
}

pub fn run_ffi(root: &Path, cmd: &FfiCmd) -> Result<()> {
    let dir = root.join("bindings").join("ffi");
    match cmd {
        FfiCmd::Build { debug } => {
            let mut args = vec!["build", "--workspace"];
            if !*debug {
                args.push("--release");
            }
            run("cargo", args, &dir)
        }
        FfiCmd::Test => run("cargo", vec!["test", "--workspace"], &dir),
        FfiCmd::Lint => {
            let mut args = vec!["clippy", "--workspace", "--all-targets"];
            args.extend(["--", "-D", "warnings"]);
            run("cargo", args, &dir)
        }
        FfiCmd::Format { fix } => cargo_fmt_packages(&dir, *fix),
    }
}
