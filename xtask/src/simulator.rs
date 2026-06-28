use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::Subcommand;

use crate::util::{on_path, run, run_built_bin, run_tool};

pub fn build_backend_and_frontend(root: &Path, debug: bool) -> Result<(PathBuf, PathBuf)> {
    let sim = root.join("simulator");
    let frontend = sim.join("frontend");
    let profile = if debug { "debug" } else { "release" };

    build_frontend(&frontend, debug)?;
    let public = frontend
        .join("target")
        .join("dx")
        .join("autd3-rs-simulator-frontend")
        .join(profile)
        .join("web")
        .join("public");

    let mut build_args: Vec<&str> = vec!["build", "-p", "autd3-rs-simulator"];
    if !debug {
        build_args.push("--release");
    }
    run("cargo", build_args, &sim)?;
    let bin_name = if cfg!(windows) {
        "autd3-rs-simulator.exe"
    } else {
        "autd3-rs-simulator"
    };
    let bin = sim.join("target").join(profile).join(bin_name);
    Ok((bin, public))
}

#[derive(Subcommand)]
pub enum SimulatorCmd {
    /// Build the backend workspace and the browser frontend bundle.
    Build {
        #[arg(long)]
        debug: bool,
    },
    /// Clippy the backend workspace and the (wasm) frontend.
    Lint,
    /// Rustfmt the backend workspace and the frontend (check by default).
    Format {
        #[arg(long)]
        fix: bool,
    },
    /// Build the frontend and run the backend serving it.
    Run {
        #[arg(long)]
        debug: bool,
        #[arg(long)]
        open: bool,
        #[arg(long)]
        skip_web_build: bool,
        #[arg(long, default_value_t = 8081)]
        port: u16,
        #[arg(long, default_value_t = 8080)]
        link_port: u16,
    },
}

pub fn run_simulator(root: &Path, cmd: SimulatorCmd) -> Result<()> {
    let sim = root.join("simulator");
    let frontend = sim.join("frontend");
    match cmd {
        SimulatorCmd::Build { debug } => {
            let mut args = vec!["build"];
            if !debug {
                args.push("--release");
            }
            run("cargo", args, &sim)?;
            build_frontend(&frontend, debug)
        }
        SimulatorCmd::Lint => {
            run(
                "cargo",
                [
                    "clippy",
                    "--workspace",
                    "--all-targets",
                    "--",
                    "-D",
                    "warnings",
                ],
                &sim,
            )?;
            run(
                "cargo",
                [
                    "clippy",
                    "--target",
                    "wasm32-unknown-unknown",
                    "--all-targets",
                    "--",
                    "-D",
                    "warnings",
                ],
                &frontend,
            )
        }
        SimulatorCmd::Format { fix } => {
            let mut args = vec!["fmt", "--all"];
            if !fix {
                args.push("--");
                args.push("--check");
            }
            run("cargo", args.clone(), &sim)?;
            run("cargo", args, &frontend)
        }
        SimulatorCmd::Run {
            debug,
            open,
            skip_web_build,
            port,
            link_port,
        } => run_serve(&sim, &frontend, debug, open, skip_web_build, port, link_port),
    }
}

fn build_frontend(frontend: &Path, debug: bool) -> Result<()> {
    if !on_path("dx") {
        bail!(
            "`dx` (dioxus-cli) not found on PATH. Install it with \
             `cargo install dioxus-cli@^0.7`."
        );
    }
    if !on_path("npm") {
        bail!("`npm` not found on PATH (needed to build Tailwind/daisyUI CSS).");
    }
    if !frontend.join("node_modules").is_dir() {
        run_tool("npm", ["install"], frontend)?;
    }
    run_tool("npm", ["run", "css"], frontend)?;
    let mut dx_args = vec!["build", "--platform", "web"];
    if !debug {
        dx_args.extend(["--release", "--debug-symbols", "false"]);
    }
    run("dx", dx_args, frontend)
}

#[allow(clippy::too_many_arguments)]
fn run_serve(
    sim: &Path,
    frontend: &Path,
    debug: bool,
    open: bool,
    skip_web_build: bool,
    port: u16,
    link_port: u16,
) -> Result<()> {
    let profile = if debug { "debug" } else { "release" };

    if !skip_web_build {
        build_frontend(frontend, debug)?;
    }

    let public = frontend
        .join("target")
        .join("dx")
        .join("autd3-rs-simulator-frontend")
        .join(profile)
        .join("web")
        .join("public");
    if !public.join("index.html").is_file() {
        bail!(
            "frontend bundle not found at {}. Run without --skip-web-build first.",
            public.display()
        );
    }

    let mut build_args: Vec<&str> = vec!["build", "-p", "autd3-rs-simulator"];
    if !debug {
        build_args.push("--release");
    }
    run("cargo", build_args, sim)?;
    let bin = sim.join("target").join(profile).join("autd3-rs-simulator");

    let url = format!("http://127.0.0.1:{port}");
    println!("simulator UI at {url} (remote link on port {link_port})");
    if open {
        let url = url.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(1500));
            let _ = open_browser(&url);
        });
    }

    let args = vec![
        "--http-port".to_string(),
        port.to_string(),
        "--link-port".to_string(),
        link_port.to_string(),
        "--web-dir".to_string(),
        public.to_string_lossy().into_owned(),
    ];
    run_built_bin(&bin, &args, true, sim)
}

fn open_browser(url: &str) -> Result<()> {
    let (program, args): (&str, Vec<&str>) = if cfg!(target_os = "macos") {
        ("open", vec![url])
    } else if cfg!(target_os = "windows") {
        ("cmd", vec!["/C", "start", "", url])
    } else {
        ("xdg-open", vec![url])
    };
    Command::new(program)
        .args(args)
        .spawn()
        .with_context(|| format!("failed to open browser via `{program}`"))?;
    Ok(())
}
