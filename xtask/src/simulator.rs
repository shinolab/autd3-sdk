use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::Subcommand;

use crate::util::{
    capture, cargo_bin, cargo_build_args, copy_dir, on_path, run, run_built_bin, run_cargo,
    run_env, run_tool,
};

pub fn build_backend_and_frontend(
    root: &Path,
    debug: bool,
    target: Option<&str>,
) -> Result<(PathBuf, PathBuf)> {
    let sim = root.join("simulator");
    let frontend = sim.join("frontend");

    crate::license::generate_simulator(root)?;
    build_frontend(&frontend, debug)?;
    let public = frontend_public(&frontend, debug);
    stage_web_assets(&sim, &public)?;

    run_cargo(cargo_build_args("autd3-rs-simulator", target, debug), &sim)?;
    let bin = cargo_bin(&sim, target, debug, "autd3-rs-simulator");
    Ok((bin, public))
}

fn frontend_public(frontend: &Path, debug: bool) -> PathBuf {
    frontend
        .join("target")
        .join("dx")
        .join("autd3-rs-simulator-frontend")
        .join(if debug { "debug" } else { "release" })
        .join("web")
        .join("public")
}

fn stage_web_assets(sim: &Path, public: &Path) -> Result<()> {
    if !public.join("index.html").is_file() {
        bail!("frontend bundle not found at {}", public.display());
    }
    let dst = sim.join("backend").join("web");
    std::fs::create_dir_all(&dst)?;
    for entry in std::fs::read_dir(&dst).with_context(|| format!("reading {}", dst.display()))? {
        let entry = entry?;
        if entry.file_name() == ".gitkeep" {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            std::fs::remove_dir_all(&path)?;
        } else {
            std::fs::remove_file(&path)?;
        }
    }
    copy_dir(public, &dst)
}

#[derive(Subcommand)]
pub enum SimulatorCmd {
    /// Build the backend workspace and the browser frontend bundle
    Build {
        /// Build the dev profile instead of release
        #[arg(long)]
        debug: bool,
    },
    /// Clippy the backend workspace and the (wasm) frontend
    Lint,
    /// Rustfmt the backend workspace and the frontend
    Format {
        /// Rewrite the files instead of only checking them
        #[arg(long)]
        fix: bool,
    },
    /// Build the frontend and run the backend serving it
    Run {
        /// Build the dev profile instead of release
        #[arg(long)]
        debug: bool,
        /// Open the simulator in a browser
        #[arg(long)]
        open: bool,
        /// Reuse the already built frontend bundle
        #[arg(long)]
        skip_web_build: bool,
        /// Port the frontend is served on
        #[arg(long, default_value_t = 8081)]
        port: u16,
        /// Port the client Link connects to
        #[arg(long, default_value_t = 8080)]
        link_port: u16,
    },
}

pub fn run_simulator(root: &Path, cmd: &SimulatorCmd) -> Result<()> {
    let sim = root.join("simulator");
    let frontend = sim.join("frontend");
    match cmd {
        SimulatorCmd::Build { debug } => build_backend_and_frontend(root, *debug, None).map(|_| ()),
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
            ensure_css(&frontend)?;
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
            let check: &[&str] = if *fix { &[] } else { &["--", "--check"] };
            let mut sim_args = vec![
                "fmt",
                "-p",
                "autd3-rs-simulator",
                "-p",
                "autd3-rs-simulator-protocol",
            ];
            sim_args.extend_from_slice(check);
            run("cargo", sim_args, &sim)?;
            let mut frontend_args = vec!["fmt", "-p", "autd3-rs-simulator-frontend"];
            frontend_args.extend_from_slice(check);
            run("cargo", frontend_args, &frontend)
        }
        SimulatorCmd::Run {
            debug,
            open,
            skip_web_build,
            port,
            link_port,
        } => run_serve(
            &sim,
            &frontend,
            *debug,
            *open,
            *skip_web_build,
            *port,
            *link_port,
        ),
    }
}

fn ensure_css(frontend: &Path) -> Result<()> {
    if !on_path("npm") {
        bail!("`npm` not found on PATH (needed to build Tailwind/daisyUI CSS).");
    }
    if !frontend.join("node_modules").is_dir() {
        run_tool("npm", ["install"], frontend)?;
    }
    run_tool("npm", ["run", "css"], frontend)
}

const DX_VERSION: &str = "0.7.10";
const DX_BINARYEN_TOOLS_DIR: &str = "binaryen-129";
const BINARYEN_RELEASE: &str = "version_127";

fn build_frontend(frontend: &Path, debug: bool) -> Result<()> {
    if !on_path("dx") {
        bail!("`dx` (dioxus-cli) not found on PATH.");
    }
    ensure_css(frontend)?;
    let dx_home = dx_home()?;
    ensure_wasm_opt(frontend, &dx_home)?;
    let mut dx_args = vec!["build", "--platform", "web"];
    if !debug {
        dx_args.extend(["--release", "--debug-symbols", "false"]);
    }
    run_env("dx", dx_args, frontend, &[("DX_HOME", &dx_home)])
}

fn ensure_wasm_opt(frontend: &Path, dx_home: &Path) -> Result<()> {
    let installed = capture("dx", &["--version"], frontend)?;
    if !installed.contains(DX_VERSION) {
        bail!(
            "expected dioxus-cli {DX_VERSION} but found `{installed}`. \
             `dx` downloads wasm-opt itself without checking the HTTP status or the extracted \
             files, which silently produces a missing binary; xtask pre-installs it into \
             {DX_BINARYEN_TOOLS_DIR} instead. Re-check that directory name against \
             BINARYEN_VERSION in the new dioxus-cli before bumping DX_VERSION."
        );
    }

    let install_dir = dx_home.join("tools").join(DX_BINARYEN_TOOLS_DIR);
    let bin = install_dir.join("bin").join(wasm_opt_name());
    if bin.is_file() {
        return Ok(());
    }

    let asset = binaryen_asset()?;
    let url = format!(
        "https://github.com/WebAssembly/binaryen/releases/download/{BINARYEN_RELEASE}/{asset}"
    );
    std::fs::create_dir_all(&install_dir)?;
    let archive = install_dir.join(&asset);
    run(
        "curl",
        [
            "-fsSL",
            "--retry",
            "5",
            "--retry-all-errors",
            "-o",
            &archive.to_string_lossy(),
            &url,
        ],
        &install_dir,
    )?;
    unpack_binaryen(&archive, &install_dir)?;
    std::fs::remove_file(&archive)?;

    if !bin.is_file() {
        bail!(
            "{asset} did not contain bin/{}; wasm-opt is missing from {}",
            wasm_opt_name(),
            install_dir.display()
        );
    }
    Ok(())
}

fn dx_home() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("DX_HOME") {
        return Ok(PathBuf::from(path));
    }
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .context("neither DX_HOME nor the home directory is set")?;
    if cfg!(any(target_os = "macos", target_os = "windows")) {
        return Ok(home.join(".dx"));
    }
    let data = std::env::var_os("XDG_DATA_HOME")
        .map_or_else(|| home.join(".local").join("share"), PathBuf::from);
    Ok(data.join(".dx"))
}

fn wasm_opt_name() -> &'static str {
    if cfg!(windows) {
        "wasm-opt.exe"
    } else {
        "wasm-opt"
    }
}

fn binaryen_asset() -> Result<String> {
    let platform = if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "x86_64-windows"
    } else if cfg!(all(target_os = "windows", target_arch = "aarch64")) {
        "arm64-windows"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "x86_64-linux"
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        "aarch64-linux"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "x86_64-macos"
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "arm64-macos"
    } else {
        bail!("no binaryen release for this platform; install wasm-opt manually");
    };
    Ok(format!("binaryen-{BINARYEN_RELEASE}-{platform}.tar.gz"))
}

fn unpack_binaryen(archive: &Path, install_dir: &Path) -> Result<()> {
    let file =
        std::fs::File::open(archive).with_context(|| format!("opening {}", archive.display()))?;
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let mut components = path.components().rev();
        let (Some(name), Some(parent)) = (components.next(), components.next()) else {
            continue;
        };
        let (name, parent) = (name.as_os_str(), parent.as_os_str());
        if !(parent == "lib" || (parent == "bin" && name == wasm_opt_name())) {
            continue;
        }
        let dir = install_dir.join(parent);
        std::fs::create_dir_all(&dir)?;
        entry.unpack(dir.join(name))?;
    }
    Ok(())
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

    let public = frontend_public(frontend, debug);
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
