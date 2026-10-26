use std::path::Path;

use anyhow::{Result, bail};
use clap::Subcommand;

use crate::clean::{CleanArgs, Cleaner};
use crate::util::{cargo_clippy, cargo_fmt, on_path, publish_workspace, run, workspace_members};

#[derive(Subcommand)]
pub(crate) enum RustCmd {
    #[command(about = "Build the `crates/` workspace")]
    Build,
    #[command(about = "Run the `crates/` workspace tests")]
    Test {
        #[arg(
            long,
            short,
            help = "Only test this package (repeatable; defaults to the whole workspace)"
        )]
        package: Vec<String>,
        #[arg(
            long,
            help = "Also run the tests marked `#[ignore]`, such as the ones that depend on real time"
        )]
        include_ignored: bool,
    },
    #[command(about = "Measure `crates/` workspace test coverage with cargo-llvm-cov")]
    Coverage {
        #[arg(
            long,
            help = "Include `tools/`, which is left out of the coverage by convention"
        )]
        all: bool,
        #[arg(long, help = "Open the HTML report in a browser")]
        open: bool,
    },
    #[command(about = "Clippy the `crates/` workspace")]
    Lint,
    #[command(about = "Rustfmt the `crates/` workspace")]
    Format {
        #[arg(long, help = "Rewrite the files instead of only checking them")]
        fix: bool,
    },
    #[command(about = "Run the `crates/` workspace criterion benchmarks")]
    Bench {
        #[arg(help = "Only run benchmarks whose id contains this string")]
        filter: Option<String>,
        #[arg(long, short, help = "Only benchmark this package")]
        package: Option<String>,
        #[arg(long, help = "Save the results under this criterion baseline name")]
        save_baseline: Option<String>,
        #[arg(
            long,
            help = "Compare the results against this saved criterion baseline"
        )]
        baseline: Option<String>,
    },
    #[command(
        about = "Check the `crates/` workspace API for SemVer violations with cargo-semver-checks"
    )]
    Semver {
        #[arg(
            long,
            help = "Released version to compare against (defaults to the latest one on crates.io)"
        )]
        baseline: Option<String>,
    },
    #[command(
        about = "Publish the `crates/` workspace to crates.io, skipping already-published versions"
    )]
    Publish {
        #[arg(long, help = "Run every check without uploading")]
        dry_run: bool,
    },
    #[command(
        about = "List public items no indexed workspace references (rust-analyzer SCIP), narrowed by a repository-wide grep"
    )]
    Deadcode {
        #[arg(
            long,
            help = "Reuse the indexes left in `target/deadcode/` instead of regenerating them"
        )]
        reuse: bool,
        #[arg(long, short, help = "Print every grep hit instead of the first few")]
        verbose: bool,
    },
    #[command(about = "Remove the `crates/` workspace build outputs")]
    Clean(CleanArgs),
}

#[allow(clippy::too_many_lines)]
pub(crate) fn run_rust(root: &Path, cmd: &RustCmd) -> Result<()> {
    match cmd {
        RustCmd::Build => run("cargo", ["build", "--workspace", "--all-targets"], root),
        RustCmd::Test {
            package,
            include_ignored,
        } => {
            let mut args = vec!["test"];
            if package.is_empty() {
                args.push("--workspace");
            }
            for package in package {
                args.extend(["--package", package.as_str()]);
            }
            args.extend(["--lib", "--bins", "--tests"]);
            if *include_ignored {
                args.extend(["--", "--include-ignored"]);
            }
            run("cargo", args, root)
        }
        RustCmd::Coverage { all, open } => {
            let test_args = [
                "llvm-cov",
                "--no-report",
                "--workspace",
                "--lib",
                "--bins",
                "--tests",
            ];
            let filter: &[&str] = if *all {
                &[]
            } else {
                &["--ignore-filename-regex", COVERAGE_IGNORE]
            };
            coverage(root, &test_args, filter, *open)
        }
        RustCmd::Lint => {
            cargo_clippy(root, &["--workspace", "--all-targets"])?;
            cargo_clippy(
                root,
                &[
                    "-p",
                    "autd3-rs-pattern-holo",
                    "--no-default-features",
                    "--all-targets",
                ],
            )?;
            cargo_clippy(
                root,
                &[
                    "-p",
                    "autd3-rs-firmware-emulator",
                    "--no-default-features",
                    "--all-targets",
                ],
            )
        }
        RustCmd::Format { fix } => cargo_fmt(root, &["--all"], *fix),
        RustCmd::Bench {
            filter,
            package,
            save_baseline,
            baseline,
        } => {
            let filter = filter.as_deref();
            let package = package.as_deref();
            let save_baseline = save_baseline.as_deref();
            let baseline = baseline.as_deref();
            let mut args = vec!["bench"];
            match package {
                Some(package) => args.extend(["--package", package]),
                None => args.push("--workspace"),
            }
            args.push("--benches");
            args.push("--");
            if let Some(filter) = filter {
                args.push(filter);
            }
            if let Some(name) = save_baseline {
                args.extend(["--save-baseline", name]);
            }
            if let Some(name) = baseline {
                args.extend(["--baseline", name]);
            }
            run("cargo", args, root)
        }
        RustCmd::Semver { baseline } => {
            let baseline = baseline.as_deref();
            if !on_path("cargo-semver-checks") {
                bail!("`cargo-semver-checks` is required");
            }
            let mut args = vec!["semver-checks".to_string()];
            for package in workspace_members(root)?
                .iter()
                .filter(|package| package.publishable())
            {
                args.push("--package".to_string());
                args.push(package.name().to_string());
            }
            if let Some(baseline) = baseline {
                args.push("--baseline-version".to_string());
                args.push(baseline.to_string());
            }
            run("cargo", args, root)
        }
        RustCmd::Publish { dry_run } => publish_workspace(root, *dry_run),
        RustCmd::Deadcode { reuse, verbose } => {
            crate::deadcode::run_deadcode(root, *reuse, *verbose)
        }
        RustCmd::Clean(args) => crate::clean::scope(root, *args, clean),
    }
}

pub(crate) fn clean(cleaner: &mut Cleaner) -> Result<()> {
    cleaner.paths(&["target"])
}

const COVERAGE_IGNORE: &str = "/tools/";

pub(crate) fn coverage(dir: &Path, test_args: &[&str], filter: &[&str], open: bool) -> Result<()> {
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
