use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use clap::Subcommand;

use crate::util::{cargo_fmt_packages, macos_soem_excludes, on_path, run};

const MIT_WHEELS: &[&str] = &[
    "autd3-core",
    "autd3-pattern",
    "autd3-pattern-holo",
    "autd3-modulation",
    "autd3-link-ethercrab",
    "autd3-link-remote",
    "autd3-link-twincat",
    "autd3",
];
const SOEM_WHEEL: &str = "autd3-link-soem";
const SOEM_CRATE: &str = "autd3-python-link-soem";

#[derive(Subcommand)]
pub enum PyCmd {
    Build {
        #[arg(long)]
        debug: bool,
        #[arg(long)]
        soem: bool,
    },
    Develop {
        #[arg(long)]
        release: bool,
        #[arg(long)]
        soem: bool,
    },
    Lint,
    Format {
        #[arg(long)]
        fix: bool,
    },
    Test {
        #[arg(long)]
        soem: bool,
    },
    Example {
        name: String,
        #[arg(long)]
        debug: bool,
        #[arg(long)]
        no_sudo: bool,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

pub fn run_py(root: &Path, cmd: PyCmd) -> Result<()> {
    let dir = root.join("bindings").join("python");
    match cmd {
        PyCmd::Build { debug, soem } => {
            let out = dir.join("target").join("wheels");
            for wheel in wheels(soem) {
                let manifest = manifest(wheel);
                let mut args = vec!["build", "-m", &manifest, "-o"];
                let out = out.to_string_lossy().into_owned();
                args.push(&out);
                if !debug {
                    args.push("--release");
                }
                maturin(&dir, None, &args)?;
            }
            Ok(())
        }
        PyCmd::Develop { release, soem } => {
            let venv = ensure_venv(&dir)?;
            develop(&dir, &venv, wheels(soem), release)
        }
        PyCmd::Lint => {
            let mut args = vec!["clippy", "--workspace", "--all-targets"];
            args.extend(macos_soem_excludes(&[SOEM_CRATE]));
            args.extend(["--", "-D", "warnings"]);
            run("cargo", args, &dir)
        }
        PyCmd::Format { fix } => cargo_fmt_packages(&dir, fix),
        PyCmd::Test { soem } => {
            let venv = ensure_venv(&dir)?;
            develop(&dir, &venv, wheels(soem), false)?;
            let python = venv_python(&venv);
            if dir.join("tests").is_dir() {
                pip_install(&dir, &venv, &["pytest", "numpy"])?;
                run(&python.to_string_lossy(), ["-m", "pytest", "tests"], &dir)
            } else {
                let imports = wheels(soem)
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
            develop(&dir, &venv, MIT_WHEELS, !debug)?;
            pip_install(&dir, &venv, &["numpy"])?;
            let script = dir.join("examples").join(format!("{name}.py"));
            if !script.is_file() {
                bail!("example not found: {}", script.display());
            }
            run_example(&venv_python(&venv), &script, &args, no_sudo, &dir)
        }
    }
}

fn wheels(soem: bool) -> &'static [&'static str] {
    if soem && !cfg!(target_os = "macos") {
        const ALL: &[&str] = &[
            "autd3-core",
            "autd3-pattern",
            "autd3-pattern-holo",
            "autd3-modulation",
            "autd3-link-ethercrab",
            "autd3-link-remote",
            "autd3-link-twincat",
            "autd3",
            SOEM_WHEEL,
        ];
        ALL
    } else {
        MIT_WHEELS
    }
}

fn manifest(wheel: &str) -> String {
    format!("{wheel}/Cargo.toml")
}

fn module_name(wheel: &str) -> String {
    wheel.replace('-', "_")
}

fn develop(dir: &Path, venv: &Path, wheels: &[&str], release: bool) -> Result<()> {
    for wheel in wheels {
        let manifest = manifest(wheel);
        let mut args = vec!["develop", "-m", &manifest];
        if release {
            args.push("--release");
        }
        maturin(dir, Some(venv), &args)?;
    }
    Ok(())
}

fn pip_install(dir: &Path, venv: &Path, packages: &[&str]) -> Result<()> {
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

fn ensure_venv(dir: &Path) -> Result<PathBuf> {
    if !on_path("uv") {
        bail!("`uv` is required for the `py` scope (https://docs.astral.sh/uv/)");
    }
    let venv = dir.join(".venv");
    if !venv.join("pyvenv.cfg").is_file() {
        run("uv", ["venv", &venv.to_string_lossy()], dir)?;
    }
    Ok(venv)
}

fn venv_python(venv: &Path) -> PathBuf {
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
    let mut cmd = Command::new("uv");
    cmd.args(["tool", "run", "--from", "maturin>=1.14,<2.0", "maturin"])
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
    if !no_sudo && cfg!(target_os = "linux") {
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
