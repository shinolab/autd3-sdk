use std::path::Path;

use anyhow::{Result, bail};
use clap::Subcommand;

use crate::clean::{CleanArgs, Cleaner};
use crate::simulator::build_backend_and_frontend;
use crate::util::{
    cargo_bin, cargo_build_args, cargo_clippy, cargo_fmt, copy_file, dist_target,
    ensure_rust_target, exe_name, package_version, run, run_cargo,
};

#[derive(Subcommand)]
pub enum ConsoleCmd {
    #[command(about = "Build the console workspace")]
    Build {
        #[arg(long, help = "Build the dev profile instead of release")]
        debug: bool,
    },
    #[command(about = "Test the console workspace")]
    Test,
    #[command(about = "Clippy the console workspace")]
    Lint,
    #[command(about = "Rustfmt the console workspace")]
    Format {
        #[arg(long, help = "Rewrite the files instead of only checking them")]
        fix: bool,
    },
    #[command(about = "Build and run the console GUI")]
    Run {
        #[arg(long, help = "Build the dev profile instead of release")]
        debug: bool,
    },
    #[command(
        about = "Build every distributed binary into `console/target/distrib` (used by `dist`)"
    )]
    Stage {
        #[arg(long, help = "Build the dev profile instead of release")]
        debug: bool,
        #[arg(
            long,
            help = "Generate the third-party license notices with cargo-about (slow; use it for release artifacts)"
        )]
        license: bool,
    },
    #[command(about = "Remove the console build outputs")]
    Clean(CleanArgs),
}

const BINARIES: &[&str] = &[
    "autd3-console",
    "autd3-simulator",
    "autd3-firmware-writer",
    "autd3-firmware-ota",
    "autd3-server",
];

pub fn run_console(root: &Path, cmd: &ConsoleCmd) -> Result<()> {
    let dir = root.join("console");
    match cmd {
        ConsoleCmd::Build { debug } => {
            let mut args = vec!["build"];
            if !*debug {
                args.push("--release");
            }
            run("cargo", args, &dir)
        }
        ConsoleCmd::Test => run("cargo", ["test"], &dir),
        ConsoleCmd::Lint => cargo_clippy(&dir, &["--all-targets"]),
        ConsoleCmd::Format { fix } => cargo_fmt(&dir, &["-p", "autd3-console"], *fix),
        ConsoleCmd::Run { debug } => {
            let mut args = vec!["run"];
            if !*debug {
                args.push("--release");
            }
            run("cargo", args, &dir)
        }
        ConsoleCmd::Stage { debug, license } => stage(root, &dir, *debug, *license),
        ConsoleCmd::Clean(args) => crate::clean::scope(root, *args, clean),
    }
}

pub fn clean(cleaner: &mut Cleaner) -> Result<()> {
    cleaner.paths(&[
        "console/target",
        "console/THIRD-PARTY-LICENSES.md",
        "console/.third-party-firmware.md",
    ])
}

fn stage(root: &Path, console_dir: &Path, debug: bool, license: bool) -> Result<()> {
    check_versions_match(console_dir)?;

    let target = dist_target();
    if let Some(target) = &target {
        ensure_rust_target(target)?;
    }
    let target = target.as_deref();

    crate::license::generate_console(root, license)?;

    run_cargo(
        cargo_build_args("autd3-console", target, debug),
        console_dir,
    )?;
    let console_bin = cargo_bin(console_dir, target, debug, "autd3-console");

    let (sim_bin, _) = build_backend_and_frontend(root, debug, target)?;

    run_cargo(
        cargo_build_args("autd3-firmware-writer", target, debug),
        root,
    )?;
    let fw_bin = cargo_bin(root, target, debug, "autd3-firmware-writer");

    run_cargo(
        cargo_build_args("autd3-rs-firmware-ota", target, debug),
        root,
    )?;
    let ota_bin = cargo_bin(root, target, debug, "autd3-firmware-ota");

    run_cargo(cargo_build_args("autd3-rs-server", target, debug), root)?;
    let server_bin = cargo_bin(root, target, debug, "autd3-server");

    let out_dir = console_dir.join("target").join("distrib");
    if out_dir.exists() {
        std::fs::remove_dir_all(&out_dir)?;
    }
    std::fs::create_dir_all(&out_dir)?;
    for (bin, name) in [&console_bin, &sim_bin, &fw_bin, &ota_bin, &server_bin]
        .into_iter()
        .zip(BINARIES)
    {
        copy_file(bin, &out_dir.join(exe_name(name)))?;
    }
    copy_file(&root.join("LICENSE"), &out_dir.join("LICENSE"))?;
    copy_file(
        &console_dir.join("THIRD-PARTY-LICENSES.md"),
        &out_dir.join("THIRD-PARTY-LICENSES.md"),
    )?;
    println!(
        "staged {} binaries in {}",
        BINARIES.len(),
        out_dir.display()
    );
    Ok(())
}

fn check_versions_match(console_dir: &Path) -> Result<()> {
    let cargo = package_version(&console_dir.join("Cargo.toml"))?;
    let dist = package_version(&console_dir.join("dist.toml"))?;
    if cargo != dist {
        bail!(
            "console/Cargo.toml is {cargo} but console/dist.toml is {dist}; \
             run `cargo xtask bump-version console <version>`"
        );
    }
    Ok(())
}
