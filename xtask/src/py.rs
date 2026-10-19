use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use clap::Subcommand;

use crate::clean::{CleanArgs, Cleaner};
use crate::util::{capture, cargo_clippy, cargo_fmt, on_path, run, run_cargo, run_env};

pub(crate) const WHEELS: &[&str] = &[
    "autd3-core",
    "autd3-pattern",
    "autd3-pattern-holo",
    "autd3-modulation",
    "autd3",
    "autd3-emulator",
];
const NATIVE_LIB_WHEELS: &[&str] = &[];
const FREE_THREADED_UNMET_DEPS: &[&str] = &["autd3-emulator"];
const FREE_THREADED_UNAVAILABLE: &[&str] = &["polars"];

#[derive(Subcommand)]
pub enum PyCmd {
    #[command(about = "Build the wheels into `bindings/python/target/wheels`")]
    Build {
        #[arg(long, help = "Build the dev profile instead of release")]
        debug: bool,
        #[arg(
            long,
            help = "Build with this interpreter (a uv request such as `3.15t`) instead of the default one"
        )]
        python: Option<String>,
        #[arg(
            long,
            help = "Generate the third-party license notices with cargo-about (slow; use it for release artifacts)"
        )]
        license: bool,
    },
    #[command(about = "Install the wheels into the local venv in editable mode")]
    Develop {
        #[arg(long, help = "Build the release profile instead of dev")]
        release: bool,
        #[arg(
            long,
            help = "Use `.venv-<python>` created with this interpreter (a uv request such as `3.15t`)"
        )]
        python: Option<String>,
    },
    #[command(about = "Clippy the Python binding workspace")]
    Lint,
    #[command(about = "Rustfmt the Python binding workspace")]
    Format {
        #[arg(long, help = "Rewrite the files instead of only checking them")]
        fix: bool,
    },
    #[command(about = "Install the wheels into the local venv and run pytest")]
    Test {
        #[arg(
            long,
            help = "Use `.venv-<python>` created with this interpreter (a uv request such as `3.15t`)"
        )]
        python: Option<String>,
    },
    #[command(about = "Run a Python example from `bindings/python/examples/`")]
    Example {
        #[arg(help = "Example script name (without `.py`)")]
        name: String,
        #[arg(long, help = "Build the dev profile instead of release")]
        debug: bool,
        #[arg(
            trailing_var_arg = true,
            allow_hyphen_values = true,
            help = "Arguments forwarded to the example"
        )]
        args: Vec<String>,
    },
    #[command(about = "Remove the Python binding build outputs")]
    Clean(CleanArgs),
}

pub fn run_py(root: &Path, cmd: PyCmd) -> Result<()> {
    let dir = root.join("bindings").join("python");
    match cmd {
        PyCmd::Build {
            debug,
            python,
            license,
        } => {
            crate::license::generate_python(root, license)?;
            let out = dir.join("target").join("wheels");
            let interpreter = python
                .as_deref()
                .map(|spec| venv_for(&dir, Some(spec)).map(|venv| venv_python(&venv)))
                .transpose()?
                .map(|python| python.to_string_lossy().into_owned());
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
                if let Some(interpreter) = &interpreter {
                    args.push("-i");
                    args.push(interpreter);
                }
                if NATIVE_LIB_WHEELS.contains(wheel) {
                    args.push("--auditwheel");
                    args.push("warn");
                }
                maturin(&dir, None, &args)?;
            }
            Ok(())
        }
        PyCmd::Develop { release, python } => {
            let venv = venv_for(&dir, python.as_deref())?;
            develop(&dir, &venv, WHEELS, release)
        }
        PyCmd::Lint => {
            cargo_clippy(&dir, &["--workspace", "--all-targets"])?;
            let venv = ensure_venv(&dir)?;
            develop(&dir, &venv, WHEELS, false)?;
            pip_install(&dir, &venv, &["mypy==2.4.0", "pytest", "numpy", "scipy", "polars"])?;
            let python = venv_python(&venv);
            run(&python.to_string_lossy(), ["-m", "mypy"], &dir)
        }
        PyCmd::Format { fix } => cargo_fmt(&dir, &[], fix),
        PyCmd::Test { python } => {
            run_cargo(["test", "-p", "autd3-python-cell"], &dir)?;
            let venv = venv_for(&dir, python.as_deref())?;
            develop(&dir, &venv, WHEELS, false)?;
            let free_threaded = is_free_threaded(&dir, &venv)?;
            let python = venv_python(&venv);
            if dir.join("tests").is_dir() {
                let packages: Vec<&str> = ["pytest", "numpy", "scipy", "polars"]
                    .into_iter()
                    .filter(|p| !free_threaded || !FREE_THREADED_UNAVAILABLE.contains(p))
                    .collect();
                pip_install(&dir, &venv, &packages)?;
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
        PyCmd::Example { name, debug, args } => {
            let venv = ensure_venv(&dir)?;
            develop(&dir, &venv, WHEELS, !debug)?;
            pip_install(&dir, &venv, &["numpy", "scipy", "polars"])?;
            let script = dir.join("examples").join(format!("{name}.py"));
            if !script.is_file() {
                bail!("example not found: {}", script.display());
            }
            let python = venv_python(&venv).to_string_lossy().into_owned();
            let script = script.to_string_lossy().into_owned();
            let script_args = ["-B", &script]
                .into_iter()
                .chain(args.iter().map(String::as_str));
            run(&python, script_args, &dir)
        }
        PyCmd::Clean(args) => crate::clean::scope(root, args, clean),
    }
}

pub fn clean(cleaner: &mut Cleaner) -> Result<()> {
    cleaner.paths(&[
        "bindings/python/target",
        "bindings/python/.pytest_cache",
        "bindings/python/.mypy_cache",
    ])?;
    let python = cleaner.root().join("bindings").join("python");
    for wheel in WHEELS {
        let dir = python.join(wheel);
        for name in ["LICENSE", "THIRD-PARTY-LICENSES.md"] {
            let path = dir.join(name);
            cleaner.remove(&path)?;
        }
        for path in stale_extension_modules(&dir, wheel) {
            cleaner.remove(&path)?;
        }
    }
    cleaner.nested("bindings/python", &["__pycache__"])?;
    cleaner.deps("bindings/python/.venv")?;
    let mut extra_venvs: Vec<String> = std::fs::read_dir(&python)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.starts_with(".venv-"))
        .collect();
    extra_venvs.sort();
    for name in extra_venvs {
        cleaner.deps(&format!("bindings/python/{name}"))?;
    }
    Ok(())
}

fn stale_extension_modules(dir: &Path, wheel: &str) -> Vec<PathBuf> {
    let module = module_name(wheel);
    let prefix = format!("_{module}.");
    let Ok(entries) = std::fs::read_dir(dir.join("python").join(&module)) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with(&prefix)
                && (name.ends_with(".so") || name.ends_with(".pyd") || name.ends_with(".dylib"))
        })
        .map(|entry| entry.path())
        .collect();
    found.sort();
    found
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
    for path in stale_extension_modules(&dir.join(wheel), wheel) {
        let _ = std::fs::remove_file(path);
    }
}

pub(crate) fn develop(dir: &Path, venv: &Path, wheels: &[&str], release: bool) -> Result<()> {
    let free_threaded = is_free_threaded(dir, venv)?;
    for wheel in wheels {
        drop_stale_native_cdylib(dir, wheel, release);
        drop_stale_extension_modules(dir, wheel);
        if free_threaded && FREE_THREADED_UNMET_DEPS.contains(wheel) {
            install_without_deps(dir, venv, wheel, release)?;
            continue;
        }
        let manifest = manifest(wheel);
        let mut args = vec!["develop", "-m", &manifest];
        if release {
            args.push("--release");
        }
        maturin(dir, Some(venv), &args)?;
    }
    Ok(())
}

fn install_without_deps(dir: &Path, venv: &Path, wheel: &str, release: bool) -> Result<()> {
    let out = dir.join("target").join("no-deps-wheels").join(wheel);
    if out.exists() {
        std::fs::remove_dir_all(&out)?;
    }
    let manifest = manifest(wheel);
    let python = venv_python(venv).to_string_lossy().into_owned();
    let out_arg = out.to_string_lossy().into_owned();
    let mut args = vec!["build", "-m", &manifest, "-i", &python, "-o", &out_arg];
    if release {
        args.push("--release");
    }
    maturin(dir, None, &args)?;
    let built = std::fs::read_dir(&out)?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext == "whl"))
        .ok_or_else(|| anyhow::anyhow!("maturin produced no wheel in {}", out.display()))?;
    ensure_uv()?;
    run_env(
        "uv",
        [
            OsStr::new("pip"),
            OsStr::new("install"),
            OsStr::new("--no-deps"),
            OsStr::new("--reinstall"),
            built.as_os_str(),
        ],
        dir,
        &[("VIRTUAL_ENV", venv.as_os_str())],
    )
}

fn is_free_threaded(dir: &Path, venv: &Path) -> Result<bool> {
    let python = venv_python(venv);
    let flag = capture(
        &python.to_string_lossy(),
        &[
            "-c",
            "import sysconfig; print(sysconfig.get_config_var('Py_GIL_DISABLED') or 0)",
        ],
        dir,
    )?;
    Ok(flag == "1")
}

fn ensure_uv() -> Result<()> {
    if !on_path("uv") {
        bail!("`uv` is required for the `py` scope (https://docs.astral.sh/uv/)");
    }
    Ok(())
}

pub(crate) fn pip_install(dir: &Path, venv: &Path, packages: &[&str]) -> Result<()> {
    ensure_uv()?;
    run_env(
        "uv",
        ["pip", "install"].iter().chain(packages),
        dir,
        &[("VIRTUAL_ENV", venv.as_os_str())],
    )
}

pub(crate) fn ensure_venv(dir: &Path) -> Result<PathBuf> {
    venv_for(dir, None)
}

fn venv_for(dir: &Path, python: Option<&str>) -> Result<PathBuf> {
    ensure_uv()?;
    if let Some(spec) = python
        && !spec
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'))
    {
        bail!("`--python` takes a uv version request such as `3.15t`, not a path: {spec}");
    }
    let venv = match python {
        Some(spec) => dir.join(format!(".venv-{spec}")),
        None => dir.join(".venv"),
    };
    if !venv.join("pyvenv.cfg").is_file() {
        let path = venv.to_string_lossy();
        let mut args = vec!["venv", &*path];
        if let Some(spec) = python {
            args.extend(["--python", spec]);
        }
        run("uv", args, dir)?;
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
    ensure_uv()?;
    let from = if cfg!(target_os = "linux") {
        "maturin[patchelf]>=1.14,<2.0"
    } else {
        "maturin>=1.14,<2.0"
    };
    let env: Vec<(&str, &OsStr)> = venv
        .map(|venv| ("VIRTUAL_ENV", venv.as_os_str()))
        .into_iter()
        .collect();
    run_env(
        "uv",
        ["tool", "run", "--from", from, "maturin"]
            .iter()
            .chain(args),
        dir,
        &env,
    )
}
