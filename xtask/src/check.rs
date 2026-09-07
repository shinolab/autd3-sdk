use std::ffi::OsStr;
use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::{Subcommand, ValueEnum};

use crate::cpu::gen_param;
use crate::util::{capture, host_triple, on_path, run_env, workspace_root};

const LEGACY_FEATURE: &str = "autd3-rs/legacy";

const MIRI_PACKAGES: &[&str] = &["autd3-cpu-wire", "autd3-cpu-fw", "autd3-rs-core"];

const MIRI_FLAGS: &str = "-Zmiri-disable-isolation -Zmiri-ignore-leaks";

const MUTANTS_PACKAGES: &[&str] = &[
    "autd3-cpu-wire",
    "autd3-cpu-fw",
    "autd3-rs-core",
    "autd3-rs-pattern",
    "autd3-rs-modulation",
];

const LEAK_SKIP: &[&str] = &[
    "shutdown_does_not_wait_for_a_pending_task",
    "a_client_that_vanishes_without_closing_does_not_block_the_next_one",
];

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SanitizerKind {
    Address,
    Leak,
    Thread,
}

impl SanitizerKind {
    fn name(self) -> &'static str {
        match self {
            Self::Address => "address",
            Self::Leak => "leak",
            Self::Thread => "thread",
        }
    }
}

#[derive(Subcommand)]
pub enum CheckCmd {
    #[command(
        about = "Detect undefined behavior with Miri (nightly toolchain + the `miri` component)"
    )]
    Miri {
        #[arg(long, short, help = "Check this package instead of the Miri-clean set")]
        package: Vec<String>,
        #[arg(long, help = "Only run the tests whose name contains this string")]
        filter: Option<String>,
    },
    #[command(
        about = "Run the tests under an LLVM sanitizer (nightly + the `rust-src` component, Linux only)"
    )]
    Sanitizer {
        #[arg(value_enum)]
        kind: SanitizerKind,
        #[arg(
            long,
            short,
            help = "Check this package instead of the whole workspace"
        )]
        package: Vec<String>,
    },
    #[command(about = "Run the tests against a std built with debug assertions (cargo-careful)")]
    Careful {
        #[arg(
            long,
            short,
            help = "Check this package instead of the whole workspace"
        )]
        package: Vec<String>,
    },
    #[command(about = "Measure how many mutations the tests catch (cargo-mutants)")]
    Mutants {
        #[arg(long, short, help = "Mutate this package instead of the default set")]
        package: Vec<String>,
        #[arg(long, help = "List the mutants instead of testing them")]
        list: bool,
        #[arg(long, help = "Test only one shard of the mutants, given as `k/n`")]
        shard: Option<String>,
        #[arg(
            long,
            help = "Scale the per-mutant timeout derived from the baseline run"
        )]
        timeout_multiplier: Option<f64>,
    },
}

pub fn run_check(root: &Path, cmd: &CheckCmd) -> Result<()> {
    match cmd {
        CheckCmd::Miri { package, filter } => run_miri(root, package, filter.as_deref()),
        CheckCmd::Sanitizer { kind, package } => run_sanitizer(root, *kind, package),
        CheckCmd::Careful { package } => run_careful(root, package),
        CheckCmd::Mutants {
            package,
            list,
            shard,
            timeout_multiplier,
        } => run_mutants(root, package, *list, shard.as_deref(), *timeout_multiplier),
    }
}

fn selection(requested: &[String], fallback: &[&str]) -> Vec<String> {
    if requested.is_empty() {
        fallback.iter().map(|name| (*name).to_string()).collect()
    } else {
        requested.to_vec()
    }
}

fn package_args(packages: &[String]) -> Vec<String> {
    packages
        .iter()
        .flat_map(|name| ["--package".to_string(), name.clone()])
        .collect()
}

fn scope_args(packages: &[String]) -> Vec<String> {
    if packages.is_empty() {
        vec![
            "--workspace".to_string(),
            "--features".to_string(),
            LEGACY_FEATURE.to_string(),
        ]
    } else {
        package_args(packages)
    }
}

fn ensure_nightly() -> Result<()> {
    let installed = capture("rustup", &["toolchain", "list"], &workspace_root())?;
    if installed.lines().any(|line| line.starts_with("nightly")) {
        return Ok(());
    }
    bail!("the nightly toolchain is required (`rustup toolchain install nightly`)")
}

fn ensure_component(component: &str) -> Result<()> {
    let installed = capture(
        "rustup",
        &["component", "list", "--toolchain", "nightly", "--installed"],
        &workspace_root(),
    )?;
    if installed.lines().any(|line| line.starts_with(component)) {
        return Ok(());
    }
    bail!(
        "the `{component}` component is required \
         (`rustup component add --toolchain nightly {component}`)"
    )
}

fn run_miri(root: &Path, packages: &[String], filter: Option<&str>) -> Result<()> {
    ensure_nightly()?;
    ensure_component("miri")?;
    gen_param(root)?;

    let target_dir = root.join("target").join("miri");
    for package in selection(packages, MIRI_PACKAGES) {
        let mut args = vec![
            "+nightly".to_string(),
            "miri".to_string(),
            "test".to_string(),
            "--package".to_string(),
            package,
            "--lib".to_string(),
            "--tests".to_string(),
        ];
        if let Some(filter) = filter {
            args.push("--".to_string());
            args.push(filter.to_string());
        }
        run_env(
            "cargo",
            args,
            root,
            &[
                ("CARGO_TARGET_DIR", target_dir.as_os_str()),
                ("MIRIFLAGS", OsStr::new(MIRI_FLAGS)),
            ],
        )?;
    }
    Ok(())
}

fn run_sanitizer(root: &Path, kind: SanitizerKind, packages: &[String]) -> Result<()> {
    if !cfg!(target_os = "linux") {
        bail!("`check sanitizer` is wired up for Linux only");
    }
    ensure_nightly()?;
    ensure_component("rust-src")?;
    gen_param(root)?;

    let mut args = vec![
        "+nightly".to_string(),
        "test".to_string(),
        "-Zbuild-std".to_string(),
        "--target".to_string(),
        host_triple()?,
        "--lib".to_string(),
        "--tests".to_string(),
    ];
    args.extend(scope_args(packages));
    if kind == SanitizerKind::Leak {
        args.push("--".to_string());
        for name in LEAK_SKIP {
            args.push("--skip".to_string());
            args.push((*name).to_string());
        }
    }

    let flags = format!("-Zsanitizer={}", kind.name());
    let target_dir = root
        .join("target")
        .join(format!("sanitizer-{}", kind.name()));
    let mut env: Vec<(&str, &OsStr)> = vec![
        ("CARGO_TARGET_DIR", target_dir.as_os_str()),
        ("RUSTFLAGS", OsStr::new(flags.as_str())),
    ];
    if kind == SanitizerKind::Address {
        env.push(("ASAN_OPTIONS", OsStr::new("detect_leaks=0")));
    }
    run_env("cargo", args, root, &env)
}

fn run_careful(root: &Path, packages: &[String]) -> Result<()> {
    ensure_nightly()?;
    if !on_path("cargo-careful") {
        bail!("`cargo-careful` is required (`cargo install cargo-careful --locked`)");
    }
    gen_param(root)?;

    let mut args = vec![
        "+nightly".to_string(),
        "careful".to_string(),
        "test".to_string(),
        "--lib".to_string(),
        "--bins".to_string(),
        "--tests".to_string(),
    ];
    args.extend(scope_args(packages));

    let target_dir = root.join("target").join("careful");
    run_env(
        "cargo",
        args,
        root,
        &[("CARGO_TARGET_DIR", target_dir.as_os_str())],
    )
}

fn run_mutants(
    root: &Path,
    packages: &[String],
    list: bool,
    shard: Option<&str>,
    timeout_multiplier: Option<f64>,
) -> Result<()> {
    if !on_path("cargo-mutants") {
        bail!("`cargo-mutants` is required (`cargo install cargo-mutants --locked`)");
    }
    gen_param(root)?;

    let output = root.join("target").join("mutants");
    let scratch = root.join("target").join("mutants-scratch");
    std::fs::create_dir_all(&scratch).with_context(|| format!("creating {}", scratch.display()))?;
    let mut args = vec![
        "mutants".to_string(),
        "--output".to_string(),
        output.to_string_lossy().into_owned(),
        "--gitignore".to_string(),
        "true".to_string(),
    ];
    args.extend(package_args(&selection(packages, MUTANTS_PACKAGES)));
    if list {
        args.push("--list".to_string());
    }
    if let Some(shard) = shard {
        args.push("--shard".to_string());
        args.push(shard.to_string());
    }
    if let Some(multiplier) = timeout_multiplier {
        args.push("--timeout-multiplier".to_string());
        args.push(multiplier.to_string());
    }
    run_env("cargo", args, root, &[("TMPDIR", scratch.as_os_str())])
}
