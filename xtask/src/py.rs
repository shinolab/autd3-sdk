use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Subcommand;

use crate::util::{cargo_fmt_packages, on_path, run};

pub(crate) const WHEELS: &[&str] = &[
    "autd3-core",
    "autd3-pattern",
    "autd3-pattern-holo",
    "autd3-modulation",
    "autd3-link-echocat",
    "autd3-link-remote",
    "autd3-link-twincat",
    "autd3-link-nop",
    "autd3",
    "autd3-emulator",
];
const NATIVE_LIB_WHEELS: &[&str] = &[];

#[derive(Subcommand)]
pub enum PyCmd {
    /// Build the wheels into `bindings/python/target/wheels`
    Build {
        /// Build the dev profile instead of release
        #[arg(long)]
        debug: bool,
    },
    /// Install the wheels into the local venv in editable mode
    Develop {
        /// Build the release profile instead of dev
        #[arg(long)]
        release: bool,
    },
    /// Clippy the Python binding workspace
    Lint,
    /// Rustfmt the Python binding workspace
    Format {
        /// Rewrite the files instead of only checking them
        #[arg(long)]
        fix: bool,
    },
    /// Install the wheels into the local venv and run pytest
    Test,
    /// Run a Python example from `bindings/python/examples/`
    Example {
        /// Example script name (without `.py`)
        name: String,
        /// Build the dev profile instead of release
        #[arg(long)]
        debug: bool,
        /// Do not wrap the run in `sudo`
        #[arg(long)]
        no_sudo: bool,
        /// Arguments forwarded to the example
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

pub fn run_py(root: &Path, cmd: PyCmd) -> Result<()> {
    let dir = root.join("bindings").join("python");
    match cmd {
        PyCmd::Build { debug } => {
            crate::license::generate_python(root)?;
            let out = dir.join("target").join("wheels");
            for wheel in WHEELS {
                drop_stale_native_cdylib(&dir, wheel, !debug);
                drop_stale_extension_modules(&dir, wheel);
                let manifest = manifest(wheel);
                let mut args = vec!["build", "-m", &manifest, "-o"];
                let out = out.to_string_lossy().into_owned();
                args.push(&out);
                if !debug {
                    args.push("--release");
                }
                if NATIVE_LIB_WHEELS.contains(wheel) {
                    args.push("--auditwheel");
                    args.push("warn");
                }
                maturin(&dir, None, &args)?;
            }
            Ok(())
        }
        PyCmd::Develop { release } => {
            let venv = ensure_venv(&dir)?;
            develop(&dir, &venv, WHEELS, release)
        }
        PyCmd::Lint => {
            let mut args = vec!["clippy", "--workspace", "--all-targets"];
            args.extend(["--", "-D", "warnings"]);
            run("cargo", args, &dir)
        }
        PyCmd::Format { fix } => cargo_fmt_packages(&dir, fix),
        PyCmd::Test => {
            let venv = ensure_venv(&dir)?;
            develop(&dir, &venv, WHEELS, false)?;
            let python = venv_python(&venv);
            if dir.join("tests").is_dir() {
                pip_install(&dir, &venv, &["pytest", "numpy", "scipy", "polars"])?;
                run(&python.to_string_lossy(), ["-m", "pytest", "tests"], &dir)
            } else {
                let imports = WHEELS
                    .iter()
                    .map(|w| format!("import {}", module_name(w)))
                    .collect::<Vec<_>>()
                    .join("; ");
                run(&python.to_string_lossy(), ["-c", &imports], &dir)
            }
        }
        PyCmd::Example {
            name,
            debug,
            no_sudo,
            args,
        } => {
            let venv = ensure_venv(&dir)?;
            develop(&dir, &venv, WHEELS, !debug)?;
            pip_install(&dir, &venv, &["numpy", "scipy", "polars"])?;
            let script = dir.join("examples").join(format!("{name}.py"));
            if !script.is_file() {
                bail!("example not found: {}", script.display());
            }
            run_example(&venv_python(&venv), &script, &args, no_sudo, &dir)
        }
    }
}

fn manifest(wheel: &str) -> String {
    format!("{wheel}/Cargo.toml")
}

fn module_name(wheel: &str) -> String {
    wheel.replace('-', "_")
}

fn drop_stale_native_cdylib(dir: &Path, wheel: &str, release: bool) {
    if !cfg!(target_os = "linux") || !NATIVE_LIB_WHEELS.contains(&wheel) {
        return;
    }
    let module = module_name(wheel);
    let profile = if release { "release" } else { "debug" };
    let target = dir.join("target");
    for path in [
        target
            .join(profile)
            .join("deps")
            .join(format!("lib{module}.so")),
        target.join(profile).join(format!("lib{module}.so")),
        target.join("maturin").join(format!("lib{module}.so")),
    ] {
        let _ = std::fs::remove_file(path);
    }
}

fn drop_stale_extension_modules(dir: &Path, wheel: &str) {
    let module = module_name(wheel);
    let package = dir.join(wheel).join("python").join(&module);
    let Ok(entries) = std::fs::read_dir(package) else {
        return;
    };
    let prefix = format!("_{module}.");
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(&prefix)
            && (name.ends_with(".so") || name.ends_with(".pyd") || name.ends_with(".dylib"))
        {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

pub(crate) fn develop(dir: &Path, venv: &Path, wheels: &[&str], release: bool) -> Result<()> {
    for wheel in wheels {
        drop_stale_native_cdylib(dir, wheel, release);
        drop_stale_extension_modules(dir, wheel);
        let manifest = manifest(wheel);
        let mut args = vec!["develop", "-m", &manifest];
        if release {
            args.push("--release");
        }
        maturin(dir, Some(venv), &args)?;
    }
    Ok(())
}

pub(crate) fn pip_install(dir: &Path, venv: &Path, packages: &[&str]) -> Result<()> {
    if !on_path("uv") {
        bail!("`uv` is required for the `py` scope (https://docs.astral.sh/uv/)");
    }
    let mut cmd = Command::new("uv");
    cmd.args(["pip", "install"])
        .args(packages)
        .current_dir(dir)
        .env("VIRTUAL_ENV", venv);
    spawn(cmd, "uv")
}

pub(crate) fn ensure_venv(dir: &Path) -> Result<PathBuf> {
    if !on_path("uv") {
        bail!("`uv` is required for the `py` scope (https://docs.astral.sh/uv/)");
    }
    let venv = dir.join(".venv");
    if !venv.join("pyvenv.cfg").is_file() {
        run("uv", ["venv", &venv.to_string_lossy()], dir)?;
    }
    Ok(venv)
}

pub(crate) fn venv_python(venv: &Path) -> PathBuf {
    if cfg!(windows) {
        venv.join("Scripts").join("python.exe")
    } else {
        venv.join("bin").join("python")
    }
}

fn maturin(dir: &Path, venv: Option<&Path>, args: &[&str]) -> Result<()> {
    if !on_path("uv") {
        bail!("`uv` is required for the `py` scope (https://docs.astral.sh/uv/)");
    }
    let from = if cfg!(target_os = "linux") {
        "maturin[patchelf]>=1.14,<2.0"
    } else {
        "maturin>=1.14,<2.0"
    };
    let mut cmd = Command::new("uv");
    cmd.args(["tool", "run", "--from", from, "maturin"])
        .args(args)
        .current_dir(dir);
    if let Some(venv) = venv {
        cmd.env("VIRTUAL_ENV", venv);
    }
    spawn(cmd, "uv")
}

fn run_example(
    python: &Path,
    script: &Path,
    args: &[String],
    no_sudo: bool,
    cwd: &Path,
) -> Result<()> {
    let python = python.to_string_lossy().into_owned();
    let script = script.to_string_lossy().into_owned();
    if !no_sudo && cfg!(unix) {
        let mut sudo_args: Vec<String> = Vec::new();
        if let Ok(log) = std::env::var("RUST_LOG") {
            sudo_args.push(format!("RUST_LOG={log}"));
        }
        sudo_args.push(python);
        sudo_args.push("-B".to_owned());
        sudo_args.push(script);
        sudo_args.extend(args.iter().cloned());
        run("sudo", sudo_args.iter().map(String::as_str), cwd)
    } else {
        let mut a = vec!["-B".to_owned(), script];
        a.extend(args.iter().cloned());
        run(&python, a.iter().map(String::as_str), cwd)
    }
}

fn spawn(mut cmd: Command, program: &str) -> Result<()> {
    let status = cmd
        .status()
        .with_context(|| format!("failed to spawn `{program}`"))?;
    if !status.success() {
        bail!("`{program}` exited with {status}");
    }
    Ok(())
}
