use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::Subcommand;

use crate::clean::{CleanArgs, Cleaner};
use crate::simulator::build_backend_and_frontend;
use crate::tool::build_twincat_cli;
use crate::util::{
    cargo_bin, cargo_build_args, copy_file, dist_target, ensure_rust_target, exe_name, run,
    run_cargo,
};

#[derive(Subcommand)]
pub enum ConsoleCmd {
    /// Build the console workspace
    Build {
        /// Build the dev profile instead of release
        #[arg(long)]
        debug: bool,
    },
    /// Test the console workspace
    Test,
    /// Clippy the console workspace
    Lint,
    /// Rustfmt the console workspace
    Format {
        /// Rewrite the files instead of only checking them
        #[arg(long)]
        fix: bool,
    },
    /// Build and run the console GUI
    Run {
        /// Build the dev profile instead of release
        #[arg(long)]
        debug: bool,
    },
    /// Build every distributed binary into `console/target/distrib` (used by `dist`)
    Stage {
        /// Build the dev profile instead of release
        #[arg(long)]
        debug: bool,
    },
    #[command(about = "Remove the console build outputs")]
    Clean(CleanArgs),
}

const BINARIES: &[&str] = &[
    "autd3-console",
    "autd3-rs-simulator",
    "autd3-firmware-writer",
    "autd3-rs-firmware-ota",
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
        ConsoleCmd::Lint => run(
            "cargo",
            ["clippy", "--all-targets", "--", "-D", "warnings"],
            &dir,
        ),
        ConsoleCmd::Format { fix } => {
            let mut args = vec!["fmt", "-p", "autd3-console"];
            if !*fix {
                args.push("--");
                args.push("--check");
            }
            run("cargo", args, &dir)
        }
        ConsoleCmd::Run { debug } => {
            let mut args = vec!["run"];
            if !*debug {
                args.push("--release");
            }
            run("cargo", args, &dir)
        }
        ConsoleCmd::Stage { debug } => stage(root, &dir, *debug),
        ConsoleCmd::Clean(args) => crate::clean::scope(root, *args, clean),
    }
}

pub fn clean(cleaner: &mut Cleaner) -> Result<()> {
    cleaner.paths(&[
        "console/target",
        "console/THIRD-PARTY-LICENSES.md",
        "console/.third-party-firmware.md",
    ])?;
    cleaner.children("console/twincat", &[".gitkeep"])
}

fn stage(root: &Path, console_dir: &Path, debug: bool) -> Result<()> {
    check_versions_match(console_dir)?;

    let target = dist_target();
    if let Some(target) = &target {
        ensure_rust_target(target)?;
    }
    let target = target.as_deref();

    crate::license::generate_console(root)?;

    stage_twincat(root, console_dir, debug)?;

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
    let ota_bin = cargo_bin(root, target, debug, "autd3-rs-firmware-ota");

    let out_dir = console_dir.join("target").join("distrib");
    if out_dir.exists() {
        std::fs::remove_dir_all(&out_dir)?;
    }
    std::fs::create_dir_all(&out_dir)?;
    for (bin, name) in [&console_bin, &sim_bin, &fw_bin, &ota_bin]
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

fn stage_twincat(root: &Path, console_dir: &Path, debug: bool) -> Result<()> {
    let dst = console_dir.join("twincat");
    std::fs::create_dir_all(&dst)?;
    for entry in std::fs::read_dir(&dst).with_context(|| format!("reading {}", dst.display()))? {
        let entry = entry?;
        if entry.file_name() == ".gitkeep" {
            continue;
        }
        std::fs::remove_file(entry.path())?;
    }
    if !cfg!(target_os = "windows") {
        return Ok(());
    }

    let exe = build_twincat_cli(root, debug)?;
    let src = exe
        .parent()
        .context("twincat-cli.exe has no parent directory")?;
    for entry in std::fs::read_dir(src).with_context(|| format!("reading {}", src.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            copy_file(&path, &dst.join(entry.file_name()))?;
        }
    }
    let staged = dst.join(exe_name("twincat-cli"));
    if !staged.is_file() {
        bail!(
            "twincat-cli was not staged to {}; autd3-console would be built without it",
            staged.display()
        );
    }
    println!("staged twincat-cli in {}", dst.display());
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

fn package_version(manifest: &Path) -> Result<String> {
    let text = std::fs::read_to_string(manifest)
        .with_context(|| format!("reading {}", manifest.display()))?;
    let doc: toml_edit::DocumentMut = text
        .parse()
        .with_context(|| format!("parsing {}", manifest.display()))?;
    doc.get("package")
        .and_then(|package| package.get("version"))
        .and_then(toml_edit::Item::as_str)
        .map(str::to_string)
        .with_context(|| format!("no [package] version in {}", manifest.display()))
}
