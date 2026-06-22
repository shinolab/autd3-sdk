use std::path::Path;

use anyhow::Result;
use clap::Args;

use crate::util::{run, run_built_bin};

#[derive(Args)]
pub struct ExampleCmd {
    name: String,
    #[arg(long)]
    debug: bool,
    #[arg(long)]
    no_sudo: bool,
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

pub fn run_example(root: &Path, cmd: &ExampleCmd) -> Result<()> {
    let mut build_args: Vec<&str> = vec![
        "build",
        "-p",
        "autd3-rs-examples",
        "--bin",
        cmd.name.as_str(),
    ];
    if !cmd.debug {
        build_args.push("--release");
    }
    run("cargo", build_args, root)?;

    let profile = if cmd.debug { "debug" } else { "release" };
    let bin = root.join("target").join(profile).join(&cmd.name);
    run_built_bin(&bin, &cmd.args, cmd.no_sudo, root)
}
