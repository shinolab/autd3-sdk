use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Subcommand;

use crate::clean::{CleanArgs, Cleaner};
use crate::util::{run, run_env};

const SOLUTION: &str = "AUTD3.slnx";

pub(crate) struct BindingPkg {
    pub assembly: &'static str,
    pub unity_id: &'static str,
    pub ffi_crate: &'static str,
    pub lib: &'static str,
}

pub(crate) const PACKAGES: &[BindingPkg] = &[
    BindingPkg {
        assembly: "AUTD3.Core",
        unity_id: "com.shinolab.autd3-sdk.core",
        ffi_crate: "autd3-ffi-core",
        lib: "autd3_core",
    },
    BindingPkg {
        assembly: "AUTD3",
        unity_id: "com.shinolab.autd3-sdk",
        ffi_crate: "autd3-ffi",
        lib: "autd3capi",
    },
    BindingPkg {
        assembly: "AUTD3.Pattern",
        unity_id: "com.shinolab.autd3-sdk.pattern",
        ffi_crate: "autd3-ffi-pattern",
        lib: "autd3_pattern",
    },
    BindingPkg {
        assembly: "AUTD3.Pattern.Holo",
        unity_id: "com.shinolab.autd3-sdk.pattern.holo",
        ffi_crate: "autd3-ffi-pattern-holo",
        lib: "autd3_pattern_holo",
    },
    BindingPkg {
        assembly: "AUTD3.Modulation",
        unity_id: "com.shinolab.autd3-sdk.modulation",
        ffi_crate: "autd3-ffi-modulation",
        lib: "autd3_modulation",
    },
];

pub(crate) const RIDS: &[&str] = &["win-x64", "linux-x64", "osx-arm64"];

#[derive(Subcommand)]
pub enum CsCmd {
    /// Build the C# solution
    Build {
        /// Build the Debug configuration instead of Release
        #[arg(long)]
        debug: bool,
    },
    /// Pack the NuGet packages (native libs from `--native-dir`, host RID otherwise)
    Pack {
        /// Directory holding the per-RID native libraries (defaults to `bindings/ffi/target/native`)
        #[arg(long)]
        native_dir: Option<PathBuf>,
        /// Directory to write the `.nupkg` files to
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Build the FFI cdylibs and run the C# tests against them
    Test,
    /// `dotnet format` the C# solution
    Format {
        /// Rewrite the files instead of only checking them
        #[arg(long)]
        fix: bool,
    },
    /// Build and run a C# example from `bindings/csharp/examples/`
    Example {
        /// Example project name
        name: String,
        /// Build the Debug configuration instead of Release
        #[arg(long)]
        debug: bool,
        /// Arguments forwarded to the example
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    #[command(about = "Remove the C# binding build outputs")]
    Clean(CleanArgs),
}

pub fn run_cs(root: &Path, cmd: CsCmd) -> Result<()> {
    let dir = root.join("bindings").join("csharp");
    match cmd {
        CsCmd::Build { debug } => {
            let config = if debug { "Debug" } else { "Release" };
            run("dotnet", ["build", SOLUTION, "-c", config], &dir)
        }
        CsCmd::Pack { native_dir, out } => pack(root, native_dir, out),
        CsCmd::Test => {
            let native = build_ffi(root)?;
            if cfg!(target_os = "windows") {
                run("dotnet", ["build", SOLUTION, "-c", "Debug"], &dir)?;
                stage_native_libs(&native, &dir)?;
                run(
                    "dotnet",
                    ["test", SOLUTION, "-c", "Debug", "--no-build"],
                    &dir,
                )
            } else {
                let (var, value) = native_lib_env(&native);
                run_env(
                    "dotnet",
                    ["test", SOLUTION, "-c", "Debug"],
                    &dir,
                    &[(var, value.as_os_str())],
                )
            }
        }
        CsCmd::Format { fix } => {
            let mut args = vec!["format", SOLUTION];
            if !fix {
                args.push("--verify-no-changes");
            }
            run("dotnet", args, &dir)
        }
        CsCmd::Example { name, debug, args } => {
            let native = build_ffi(root)?;
            let config = if debug { "Debug" } else { "Release" };
            let project_dir = dir.join("examples").join(&name);
            let project = project_dir.join(format!("{name}.csproj"));
            if !project.is_file() {
                bail!("example not found: {}", project.display());
            }
            run(
                "dotnet",
                ["build", &project.to_string_lossy(), "-c", config],
                &dir,
            )?;
            let exe = find_example_exe(&project_dir, config, &name)?;
            let (var, value) = native_lib_env(&native);
            run_env(
                &exe.to_string_lossy(),
                &args,
                &dir,
                &[(var, value.as_os_str())],
            )
        }
        CsCmd::Clean(args) => crate::clean::scope(root, args, clean),
    }
}

pub fn clean(cleaner: &mut Cleaner) -> Result<()> {
    cleaner.path("bindings/csharp/dist")?;
    cleaner.in_each_subdir(
        "bindings/csharp/src",
        &["runtimes", "THIRD-PARTY-LICENSES.md"],
    )?;
    cleaner.nested("bindings/csharp", &["bin", "obj"])
}

fn pack(root: &Path, native_dir: Option<PathBuf>, out: Option<PathBuf>) -> Result<()> {
    let dir = root.join("bindings").join("csharp");
    let ffi = root.join("bindings").join("ffi");
    let native_root = native_dir.unwrap_or_else(|| ffi.join("target").join("native"));

    let present: Vec<&str> = RIDS
        .iter()
        .copied()
        .filter(|rid| native_root.join(rid).is_dir())
        .collect();
    let host = if present.is_empty() {
        run("cargo", ["build", "--workspace", "--release"], &ffi)?;
        Some((host_rid()?, ffi.join("target").join("release")))
    } else {
        None
    };

    let out = out.unwrap_or_else(|| dir.join("dist"));
    std::fs::create_dir_all(&out)?;
    let src = dir.join("src");

    for pkg in PACKAGES {
        let lib = pkg.lib;
        let pkg = pkg.assembly;
        let pkg_dir = src.join(pkg);
        let runtimes = pkg_dir.join("runtimes");
        if runtimes.exists() {
            std::fs::remove_dir_all(&runtimes)
                .with_context(|| format!("clearing {}", runtimes.display()))?;
        }
        match &host {
            Some((rid, from)) => stage_native(from, rid, lib, &runtimes)?,
            None => {
                for rid in &present {
                    stage_native(&native_root.join(rid), rid, lib, &runtimes)?;
                }
            }
        }
        let proj = pkg_dir.join(format!("{pkg}.csproj"));
        run(
            "dotnet",
            [
                "pack",
                &proj.to_string_lossy(),
                "-c",
                "Release",
                "-o",
                &out.to_string_lossy(),
            ],
            &dir,
        )?;
    }
    println!("cs pack: nupkg written to {}", out.display());
    Ok(())
}

fn stage_native(from: &Path, rid: &str, lib: &str, runtimes: &Path) -> Result<()> {
    let (prefix, ext) = rid_affix(rid);
    let file = format!("{prefix}{lib}.{ext}");
    let src = from.join(&file);
    if !src.is_file() {
        bail!("native lib not found: {}", src.display());
    }
    let dst_dir = runtimes.join(rid).join("native");
    std::fs::create_dir_all(&dst_dir)?;
    std::fs::copy(&src, dst_dir.join(&file))
        .with_context(|| format!("staging {} -> {}", src.display(), dst_dir.display()))?;
    Ok(())
}

pub(crate) fn rid_affix(rid: &str) -> (&'static str, &'static str) {
    if rid.starts_with("win") {
        ("", "dll")
    } else if rid.starts_with("osx") {
        ("lib", "dylib")
    } else {
        ("lib", "so")
    }
}

pub(crate) fn host_rid() -> Result<&'static str> {
    Ok(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "linux-x64",
        ("windows", "x86_64") => "win-x64",
        ("macos", "aarch64") => "osx-arm64",
        ("macos", "x86_64") => "osx-x64",
        (os, arch) => bail!("unsupported host {os}/{arch}"),
    })
}

fn build_ffi(root: &Path) -> Result<PathBuf> {
    let ffi = root.join("bindings").join("ffi");
    run("cargo", ["build", "--workspace", "--release"], &ffi)?;
    Ok(ffi.join("target").join("release"))
}

fn find_example_exe(project_dir: &Path, config: &str, name: &str) -> Result<PathBuf> {
    let bin = project_dir.join("bin").join(config);
    if let Ok(entries) = std::fs::read_dir(&bin) {
        for entry in entries.flatten() {
            let candidate = entry.path().join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    bail!("built example executable not found under {}", bin.display());
}

fn native_lib_env(native: &Path) -> (&'static str, OsString) {
    if cfg!(target_os = "windows") {
        let existing = std::env::var("PATH").unwrap_or_default();
        ("PATH", format!("{};{existing}", native.display()).into())
    } else if cfg!(target_os = "macos") {
        ("DYLD_LIBRARY_PATH", native.into())
    } else {
        ("LD_LIBRARY_PATH", native.into())
    }
}

fn stage_native_libs(native: &Path, csharp_dir: &Path) -> Result<()> {
    let test_bin = csharp_dir.join("tests/AUTD3.Tests/bin/Debug");
    let (_, ext) = rid_affix(host_rid()?);
    let mut staged = 0;
    for tfm in std::fs::read_dir(&test_bin)
        .with_context(|| format!("reading test output dir {}", test_bin.display()))?
    {
        let tfm = tfm?.path();
        if !tfm.is_dir() {
            continue;
        }
        for lib in std::fs::read_dir(native)? {
            let lib = lib?.path();
            if lib.extension().and_then(|e| e.to_str()) == Some(ext) {
                std::fs::copy(&lib, tfm.join(lib.file_name().unwrap()))?;
                staged += 1;
            }
        }
    }
    if staged == 0 {
        bail!(
            "no native libraries staged from {} into {}",
            native.display(),
            test_bin.display()
        );
    }
    Ok(())
}
