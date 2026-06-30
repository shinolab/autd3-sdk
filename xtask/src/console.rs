use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Subcommand;

use crate::simulator::build_backend_and_frontend;
use crate::tool::build_twincat_cli;
use crate::util::run;

#[derive(Subcommand)]
pub enum ConsoleCmd {
    /// Build the console workspace.
    Build {
        #[arg(long)]
        debug: bool,
    },
    /// Clippy the console workspace.
    Lint,
    /// Rustfmt the console workspace (check by default).
    Format {
        #[arg(long)]
        fix: bool,
    },
    /// Build and run the console GUI.
    Run {
        #[arg(long)]
        debug: bool,
    },
    /// Build console + simulator (+ twincat on Windows) and produce a distributable archive.
    Bundle {
        #[arg(long)]
        debug: bool,
    },
}

pub fn run_console(root: &Path, cmd: ConsoleCmd) -> Result<()> {
    let dir = root.join("console");
    match cmd {
        ConsoleCmd::Build { debug } => {
            let mut args = vec!["build"];
            if !debug {
                args.push("--release");
            }
            run("cargo", args, &dir)
        }
        ConsoleCmd::Lint => run(
            "cargo",
            ["clippy", "--all-targets", "--", "-D", "warnings"],
            &dir,
        ),
        ConsoleCmd::Format { fix } => {
            let mut args = vec!["fmt", "--all"];
            if !fix {
                args.push("--");
                args.push("--check");
            }
            run("cargo", args, &dir)
        }
        ConsoleCmd::Run { debug } => {
            let mut args = vec!["run"];
            if !debug {
                args.push("--release");
            }
            run("cargo", args, &dir)
        }
        ConsoleCmd::Bundle { debug } => bundle(root, &dir, debug),
    }
}

fn bundle(root: &Path, console_dir: &Path, debug: bool) -> Result<()> {
    let profile = if debug { "debug" } else { "release" };

    crate::license::generate_console(root)?;

    let mut args = vec!["build"];
    if !debug {
        args.push("--release");
    }
    run("cargo", args, console_dir)?;
    let console_bin = console_dir
        .join("target")
        .join(profile)
        .join(exe_name("autd3-console"));

    let (sim_bin, sim_web) = build_backend_and_frontend(root, debug)?;

    let out_dir = console_dir.join("target").join("bundle");
    let staging = out_dir.join("autd3-console");
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    std::fs::create_dir_all(&staging)?;

    copy_file(&console_bin, &staging.join(exe_name("autd3-console")))?;

    copy_file(&root.join("LICENSE"), &staging.join("LICENSE"))?;
    copy_file(
        &console_dir.join("THIRD-PARTY-LICENSES.md"),
        &staging.join("THIRD-PARTY-LICENSES.md"),
    )?;

    let sim_dir = staging.join("simulator");
    copy_file(&sim_bin, &sim_dir.join(exe_name("autd3-rs-simulator")))?;
    copy_dir(&sim_web, &sim_dir.join("web"))?;

    let fw_bin = build_firmware_cli(root, debug)?;
    copy_file(
        &fw_bin,
        &staging.join("firmware").join(exe_name("autd3-firmware")),
    )?;

    if cfg!(target_os = "windows") {
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

fn build_firmware_cli(root: &Path, debug: bool) -> Result<PathBuf> {
    let mut args = vec!["build", "-p", "autd3-firmware"];
    if !debug {
        args.push("--release");
    }
    run("cargo", args, root)?;
    let profile = if debug { "debug" } else { "release" };
    Ok(root
        .join("target")
        .join(profile)
        .join(exe_name("autd3-firmware")))
}

fn exe_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
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

fn copy_file(src: &Path, dst: &Path) -> Result<()> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(src, dst)
        .with_context(|| format!("copying {} -> {}", src.display(), dst.display()))?;
    Ok(())
}

fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src).with_context(|| format!("reading {}", src.display()))? {
        let entry = entry?;
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir(&path, &target)?;
        } else {
            copy_file(&path, &target)?;
        }
    }
    Ok(())
}

fn zip_dir(src: &Path, archive: &Path) -> Result<()> {
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
