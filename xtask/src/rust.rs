use std::path::Path;

use anyhow::{Result, bail};
use clap::Subcommand;

use crate::clean::{CleanArgs, Cleaner};
use crate::util::{
    cargo_bin, cargo_build_args, cargo_clippy, cargo_fmt, on_path, publish_workspace,
    publishable_members, run,
};

#[derive(Subcommand)]
pub enum RustCmd {
    /// Build the `crates/` workspace
    Build,
    /// Run the `crates/` workspace tests
    Test,
    /// Measure `crates/` workspace test coverage with cargo-llvm-cov
    Coverage {
        /// Include `tools/` and `examples/`, which carry no tests by convention
        #[arg(long)]
        all: bool,
        /// Open the HTML report in a browser
        #[arg(long)]
        open: bool,
    },
    /// Clippy the `crates/` workspace
    Lint,
    /// Rustfmt the `crates/` workspace
    Format {
        /// Rewrite the files instead of only checking them
        #[arg(long)]
        fix: bool,
    },
    /// Run the `crates/` workspace criterion benchmarks
    Bench {
        /// Only run benchmarks whose id contains this string
        filter: Option<String>,
        /// Only benchmark this package
        #[arg(long, short)]
        package: Option<String>,
        /// Save the results under this criterion baseline name
        #[arg(long)]
        save_baseline: Option<String>,
        /// Compare the results against this saved criterion baseline
        #[arg(long)]
        baseline: Option<String>,
    },
    /// Check the `crates/` workspace API for SemVer violations with cargo-semver-checks
    Semver {
        /// Released version to compare against (defaults to the latest one on crates.io)
        #[arg(long)]
        baseline: Option<String>,
    },
    /// Publish the `crates/` workspace to crates.io, skipping already-published versions
    Publish {
        /// Run every check without uploading
        #[arg(long)]
        dry_run: bool,
    },
    /// Build and run an example from `examples/`
    Example {
        /// Example binary name (one binary per feature; see `examples/`)
        name: String,
        /// Build the dev profile instead of release
        #[arg(long)]
        debug: bool,
        /// Arguments forwarded to the example
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    #[command(about = "Remove the `crates/` workspace build outputs")]
    Clean(CleanArgs),
}

pub fn run_rust(root: &Path, cmd: &RustCmd) -> Result<()> {
    match cmd {
        RustCmd::Build => run("cargo", ["build", "--workspace", "--all-targets"], root),
        RustCmd::Test => run(
            "cargo",
            ["test", "--workspace", "--lib", "--bins", "--tests"],
            root,
        ),
        RustCmd::Coverage { all, open } => run_coverage(root, *all, *open),
        RustCmd::Lint => run_lint(root),
        RustCmd::Format { fix } => cargo_fmt(root, &["--all"], *fix),
        RustCmd::Bench {
            filter,
            package,
            save_baseline,
            baseline,
        } => {
            let mut args = vec!["bench".to_string()];
            match package {
                Some(package) => args.extend(["--package".to_string(), package.clone()]),
                None => args.push("--workspace".to_string()),
            }
            args.push("--benches".to_string());
            args.push("--".to_string());
            if let Some(filter) = filter {
                args.push(filter.clone());
            }
            if let Some(name) = save_baseline {
                args.extend(["--save-baseline".to_string(), name.clone()]);
            }
            if let Some(name) = baseline {
                args.extend(["--baseline".to_string(), name.clone()]);
            }
            run("cargo", args, root)
        }
        RustCmd::Semver { baseline } => run_semver(root, baseline.as_deref()),
        RustCmd::Publish { dry_run } => publish_workspace(root, *dry_run),
        RustCmd::Example { name, debug, args } => run_example(root, name, *debug, args),
        RustCmd::Clean(args) => crate::clean::scope(root, *args, clean),
    }
}

pub fn clean(cleaner: &mut Cleaner) -> Result<()> {
    cleaner.paths(&["target"])
}

fn run_lint(root: &Path) -> Result<()> {
    cargo_clippy(root, &["--workspace", "--all-targets"])?;
    cargo_clippy(
        root,
        &[
            "-p",
            "autd3-rs-pattern-holo",
            "--no-default-features",
            "--all-targets",
        ],
    )
}

const COVERAGE_IGNORE: &str = "/(tools|examples)/";

pub fn coverage(dir: &Path, test_args: &[&str], filter: &[&str], open: bool) -> Result<()> {
    if !on_path("cargo-llvm-cov") {
        bail!("`cargo-llvm-cov` is required (`cargo install cargo-llvm-cov --locked`)");
    }
    run("cargo", test_args, dir)?;

    let mut html_args = vec!["llvm-cov", "report", "--html"];
    html_args.extend_from_slice(filter);
    if open {
        html_args.push("--open");
    }
    run("cargo", html_args, dir)?;

    let mut summary_args = vec!["llvm-cov", "report", "--summary-only"];
    summary_args.extend_from_slice(filter);
    run("cargo", summary_args, dir)
}

fn run_coverage(root: &Path, all: bool, open: bool) -> Result<()> {
    let test_args = [
        "llvm-cov",
        "--no-report",
        "--workspace",
        "--lib",
        "--bins",
        "--tests",
    ];
    let filter: &[&str] = if all {
        &[]
    } else {
        &["--ignore-filename-regex", COVERAGE_IGNORE]
    };
    coverage(root, &test_args, filter, open)
}

fn run_semver(root: &Path, baseline: Option<&str>) -> Result<()> {
    if !on_path("cargo-semver-checks") {
        bail!("`cargo-semver-checks` is required");
    }
    let mut args = vec!["semver-checks".to_string()];
    for package in publishable_members(root)? {
        args.push("--package".to_string());
        args.push(package.name().to_string());
    }
    if let Some(baseline) = baseline {
        args.push("--baseline-version".to_string());
        args.push(baseline.to_string());
    }
    run("cargo", args, root)
}

fn run_example(root: &Path, name: &str, debug: bool, args: &[String]) -> Result<()> {
    let mut build_args = cargo_build_args("autd3-rs-examples", None, debug);
    build_args.extend(["--bin", name]);
    run("cargo", build_args, root)?;

    let bin = cargo_bin(root, None, debug, name);
    run(&bin.to_string_lossy(), args, root)
}
