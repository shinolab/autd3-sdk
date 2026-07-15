use std::path::Path;

use anyhow::Result;
use clap::Subcommand;

use crate::util::{cargo_fmt_packages, run};

const SOEM_CRATE: &str = "autd3-ffi-link-soem";

#[derive(Subcommand)]
pub enum FfiCmd {
    /// Build the C ABI cdylibs
    Build {
        /// Build the dev profile instead of release
        #[arg(long)]
        debug: bool,
        /// Also build the SOEM cdylib (opt-in: it is GPL-3.0-only)
        #[arg(long)]
        soem: bool,
    },
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
        FfiCmd::Build { debug, soem } => {
            let mut args = vec!["build", "--workspace"];
            if !*soem {
                args.push("--exclude");
                args.push(SOEM_CRATE);
            }
            if !*debug {
                args.push("--release");
            }
            run("cargo", args, &dir)
        }
        FfiCmd::Lint => {
            let mut args = vec!["clippy", "--workspace", "--all-targets"];
            args.extend(["--", "-D", "warnings"]);
            run("cargo", args, &dir)
        }
        FfiCmd::Format { fix } => cargo_fmt_packages(&dir, *fix),
    }
}
