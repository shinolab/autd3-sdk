use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Subcommand;

use crate::util::run;

#[derive(Subcommand)]
pub enum CpuCmd {
    Test,
    Lint,
    Format {
        #[arg(long)]
        fix: bool,
    },
}

pub fn run_cpu(root: &Path, cmd: &CpuCmd) -> Result<()> {
    match cmd {
        CpuCmd::Test => cpu_test(root),
        CpuCmd::Lint => cpu_lint(root),
        CpuCmd::Format { fix } => cpu_format(root, *fix),
    }
}

/* Regenerate `inc/params_fpga.h` from the FPGA `params.svh` so the CPU build
 * never drifts from the hardware single source of truth (legacy ran this as a
 * `generate-param` pre-test step). */
pub(crate) fn gen_param(root: &Path) -> Result<()> {
    run("python3", ["gen_param.py"], &root.join("firmware/cpu"))
}

fn cpu_test(root: &Path) -> Result<()> {
    gen_param(root)?;

    let tests_dir = root.join("firmware/cpu/tests");
    let build_dir = tests_dir.join("build");
    let build_arg = format!("-B{}", build_dir.display());
    let source_arg = format!("-S{}", tests_dir.display());
    let build_dir_str = build_dir.to_string_lossy().into_owned();

    run("cmake", [source_arg.as_str(), build_arg.as_str()], root)?;
    run(
        "cmake",
        ["--build", build_dir_str.as_str(), "--parallel"],
        root,
    )?;
    run(
        "ctest",
        ["--test-dir", build_dir_str.as_str(), "--output-on-failure"],
        root,
    )
}

fn cpu_lint(root: &Path) -> Result<()> {
    let files = collect_cpu_sources(root)?;
    if files.is_empty() {
        bail!("no C sources found under firmware/cpu/{{src,inc}}");
    }

    let inc = root.join("firmware/cpu/inc");
    let src = root.join("firmware/cpu/src");
    let inc_flag = format!("-I{}", inc.display());
    let src_flag = format!("-I{}", src.display());

    let mut args: Vec<String> = Vec::new();
    args.push("--warnings-as-errors=*".to_string());
    args.push("--quiet".to_string());
    for f in &files {
        args.push(f.to_string_lossy().into_owned());
    }
    args.push("--".to_string());
    args.push("-std=c11".to_string());
    args.push(inc_flag);
    args.push(src_flag);

    run("clang-tidy", args.iter().map(String::as_str), root)
}

fn cpu_format(root: &Path, fix: bool) -> Result<()> {
    let files = collect_cpu_sources(root)?;
    if files.is_empty() {
        bail!("no C sources found under firmware/cpu/{{src,inc}}");
    }

    let mut args: Vec<String> = Vec::new();
    args.push("--style=file".to_string());
    if fix {
        args.push("-i".to_string());
    } else {
        args.push("--dry-run".to_string());
        args.push("-Werror".to_string());
    }
    for f in &files {
        args.push(f.to_string_lossy().into_owned());
    }

    run("clang-format", args.iter().map(String::as_str), root)
}

fn collect_cpu_sources(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for dir in ["firmware/cpu/src", "firmware/cpu/inc"] {
        collect_c_files(&root.join(dir), &mut files)?;
    }
    files.sort();
    Ok(files)
}

fn collect_c_files(path: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let entries = std::fs::read_dir(path).with_context(|| format!("reading {}", path.display()))?;
    for entry in entries {
        let p = entry?.path();
        if p.is_dir() {
            collect_c_files(&p, files)?;
            continue;
        }
        let Some(ext) = p.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if matches!(ext, "c" | "h") {
            files.push(p);
        }
    }
    Ok(())
}
