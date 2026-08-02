use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Subcommand;

use crate::simulator::build_backend_and_frontend;
use crate::tool::build_twincat_cli;
use crate::util::{
    cargo_bin, cargo_build_args, copy_dir, copy_file, dist_target, ensure_rust_target, exe_name,
    run, run_cargo,
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
    /// Stage the binaries and produce a self-contained archive
    Bundle {
        /// Build the dev profile instead of release
        #[arg(long)]
        debug: bool,
        /// Build twincat-cli and include it in the archive (Windows only, requires TwinCAT XAE)
        #[arg(long)]
        twincat: bool,
    },
}

const BINARIES: &[&str] = &[
    "autd3-console",
    "autd3-rs-simulator",
    "autd3-firmware",
    "autd3-appliance",
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
        ConsoleCmd::Stage { debug } => stage(root, &dir, *debug).map(|_| ()),
        ConsoleCmd::Bundle { debug, twincat } => bundle(root, &dir, *debug, *twincat),
    }
}

fn stage(root: &Path, console_dir: &Path, debug: bool) -> Result<PathBuf> {
    check_versions_match(console_dir)?;

    let target = dist_target();
    if let Some(target) = &target {
        ensure_rust_target(target)?;
    }
    let target = target.as_deref();

    crate::license::generate_console(root)?;

    run_cargo(
        cargo_build_args("autd3-console", target, debug),
        console_dir,
    )?;
    let console_bin = cargo_bin(console_dir, target, debug, "autd3-console");

    let (sim_bin, _) = build_backend_and_frontend(root, debug, target)?;

    run_cargo(cargo_build_args("autd3-firmware", target, debug), root)?;
    let fw_bin = cargo_bin(root, target, debug, "autd3-firmware");

    run_cargo(cargo_build_args("autd3-appliance", target, debug), root)?;
    let appliance_bin = cargo_bin(root, target, debug, "autd3-appliance");

    let out_dir = console_dir.join("target").join("distrib");
    if out_dir.exists() {
        std::fs::remove_dir_all(&out_dir)?;
    }
    std::fs::create_dir_all(&out_dir)?;
    for (bin, name) in [&console_bin, &sim_bin, &fw_bin, &appliance_bin]
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
    Ok(out_dir)
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

fn bundle(root: &Path, console_dir: &Path, debug: bool, twincat: bool) -> Result<()> {
    if twincat && !cfg!(target_os = "windows") {
        bail!(
            "`--twincat` is Windows-only: twincat-cli targets .NET Framework 4.8 and \
             drives the TwinCAT XAE Shell through the DTE COM API"
        );
    }

    let distrib = stage(root, console_dir, debug)?;

    let out_dir = console_dir.join("target").join("bundle");
    let staging = out_dir.join("autd3-console");
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    copy_dir(&distrib, &staging)?;

    if twincat {
        let exe = build_twincat_cli(root, debug)?;
        let dist = exe
            .parent()
            .context("twincat-cli.exe has no parent directory")?;
        copy_dir(dist, &staging.join("twincat"))?;
    }

    let archive = if cfg!(target_os = "windows") {
        let archive = out_dir.join(format!("autd3-console-{}.zip", bundle_os()));
        zip_dir(&staging, &archive)?;
        archive
    } else {
        let archive = out_dir.join(format!("autd3-console-{}.tar.gz", bundle_os()));
        run(
            "tar",
            [
                "czf",
                &archive.to_string_lossy(),
                "-C",
                &out_dir.to_string_lossy(),
                "autd3-console",
            ],
            &out_dir,
        )?;
        archive
    };
    println!("created {}", archive.display());
    Ok(())
}

fn bundle_os() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows-x64"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux-x64"
    }
}

fn zip_dir(src: &Path, archive: &Path) -> Result<()> {
    if let Some(parent) = archive.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::File::create(archive)
        .with_context(|| format!("creating {}", archive.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let root_name = src
        .file_name()
        .context("staging dir has no name")?
        .to_string_lossy()
        .into_owned();
    add_to_zip(&mut zip, src, &root_name, options)?;
    zip.finish()?;
    Ok(())
}

fn add_to_zip(
    zip: &mut zip::ZipWriter<std::fs::File>,
    dir: &Path,
    prefix: &str,
    options: zip::write::SimpleFileOptions,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = format!("{prefix}/{}", entry.file_name().to_string_lossy());
        if path.is_dir() {
            add_to_zip(zip, &path, &name, options)?;
        } else {
            zip.start_file(name, options)?;
            let mut f = std::fs::File::open(&path)?;
            std::io::copy(&mut f, zip)?;
        }
    }
    Ok(())
}
