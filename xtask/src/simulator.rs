use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::Subcommand;

use crate::clean::{CleanArgs, Cleaner};
use crate::util::{
    capture, cargo_bin, cargo_build_args, cargo_clippy, cargo_fmt, copy_dir, exe_name, on_path,
    run, run_cargo, run_env, run_tool,
};

pub(crate) fn build_backend_and_frontend(
    root: &Path,
    debug: bool,
    target: Option<&str>,
) -> Result<(PathBuf, PathBuf)> {
    let sim = root.join("simulator");
    let frontend = sim.join("frontend");

    build_frontend(&frontend, debug)?;
    let public = frontend_public(&frontend, debug);
    stage_web_assets(&sim, &public)?;

    run_cargo(cargo_build_args("autd3-rs-simulator", target, debug), &sim)?;
    let bin = cargo_bin(&sim, target, debug, "autd3-simulator");
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
pub(crate) enum SimulatorCmd {
    #[command(about = "Build the backend workspace and the browser frontend bundle")]
    Build {
        #[arg(long, help = "Build the dev profile instead of release")]
        debug: bool,
        #[arg(
            long,
            help = "Generate the third-party license notices with cargo-about (slow; use it for release artifacts)"
        )]
        license: bool,
    },
    #[command(about = "Run the backend workspace tests")]
    Test,
    #[command(about = "Clippy the backend workspace and the (wasm) frontend")]
    Lint,
    #[command(about = "Rustfmt the backend workspace and the frontend")]
    Format {
        #[arg(long, help = "Rewrite the files instead of only checking them")]
        fix: bool,
    },
    #[command(about = "Build the frontend and run the backend serving it")]
    Run {
        #[arg(long, help = "Build the dev profile instead of release")]
        debug: bool,
        #[arg(long, help = "Open the simulator in a browser")]
        open: bool,
        #[arg(long, help = "Reuse the already built frontend bundle")]
        skip_web_build: bool,
        #[arg(
            long,
            default_value_t = 44336,
            help = "UDP port the emulated devices listen on (the control port is port + 1)"
        )]
        port: u16,
        #[arg(long, default_value_t = 8081, help = "Port the frontend is served on")]
        http_port: u16,
    },
    #[command(about = "Fail on known vulnerabilities in the frontend's npm dependencies")]
    Audit,
    #[command(about = "Remove the simulator build outputs")]
    Clean(CleanArgs),
}

pub(crate) fn run_simulator(root: &Path, cmd: &SimulatorCmd) -> Result<()> {
    let sim = root.join("simulator");
    let frontend = sim.join("frontend");
    match cmd {
        SimulatorCmd::Build { debug, license } => build(root, *debug, *license),
        SimulatorCmd::Test => run(
            "cargo",
            ["test", "--workspace", "--lib", "--bins", "--tests"],
            &sim,
        ),
        SimulatorCmd::Lint => lint(&sim, &frontend),
        SimulatorCmd::Format { fix } => format(&sim, &frontend, *fix),
        SimulatorCmd::Run {
            debug,
            open,
            skip_web_build,
            port,
            http_port,
        } => run_serve(
            &sim,
            &frontend,
            *debug,
            *open,
            *skip_web_build,
            *port,
            *http_port,
        ),
        SimulatorCmd::Audit => audit(&frontend),
        SimulatorCmd::Clean(args) => crate::clean::scope(root, *args, clean),
    }
}

pub(crate) fn clean(cleaner: &mut Cleaner) -> Result<()> {
    cleaner.paths(&[
        "simulator/target",
        "simulator/THIRD-PARTY-LICENSES.md",
        "simulator/.third-party-frontend.md",
        "simulator/frontend/target",
        "simulator/frontend/dist",
        "simulator/frontend/assets/tailwind.css",
    ])?;
    cleaner.children("simulator/backend/web", &[".gitkeep"])?;
    cleaner.deps("simulator/frontend/node_modules")
}

fn build(root: &Path, debug: bool, license: bool) -> Result<()> {
    crate::license::generate_simulator(root, license)?;
    build_backend_and_frontend(root, debug, None).map(|_| ())
}

fn lint(sim: &Path, frontend: &Path) -> Result<()> {
    cargo_clippy(sim, &["--workspace", "--all-targets"])?;
    ensure_css(frontend)?;
    cargo_clippy(
        frontend,
        &["--target", "wasm32-unknown-unknown", "--all-targets"],
    )
}

fn format(sim: &Path, frontend: &Path, fix: bool) -> Result<()> {
    cargo_fmt(
        sim,
        &[
            "-p",
            "autd3-rs-simulator",
            "-p",
            "autd3-rs-simulator-protocol",
        ],
        fix,
    )?;
    cargo_fmt(frontend, &["-p", "autd3-rs-simulator-frontend"], fix)
}

fn audit(frontend: &Path) -> Result<()> {
    if !on_path("npm") {
        bail!("`npm` not found on PATH (needed for `npm audit`).");
    }
    run_tool("npm", ["audit"], frontend)
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
    run_env("dx", dx_args, frontend, &[("DX_HOME", dx_home.as_os_str())])
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
    let wasm_opt = exe_name("wasm-opt");
    let bin = install_dir.join("bin").join(&wasm_opt);
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
            "{asset} did not contain bin/{wasm_opt}; wasm-opt is missing from {}",
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
    let wasm_opt = exe_name("wasm-opt");
    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let mut components = path.components().rev();
        let (Some(name), Some(parent)) = (components.next(), components.next()) else {
            continue;
        };
        let (name, parent) = (name.as_os_str(), parent.as_os_str());
        if !(parent == "lib" || (parent == "bin" && name == wasm_opt.as_str())) {
            continue;
        }
        let dir = install_dir.join(parent);
        std::fs::create_dir_all(&dir)?;
        entry.unpack(dir.join(name))?;
    }
    Ok(())
}

fn run_serve(
    sim: &Path,
    frontend: &Path,
    debug: bool,
    open: bool,
    skip_web_build: bool,
    port: u16,
    http_port: u16,
) -> Result<()> {
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

    run(
        "cargo",
        cargo_build_args("autd3-rs-simulator", None, debug),
        sim,
    )?;
    let bin = cargo_bin(sim, None, debug, "autd3-simulator");

    let url = format!("http://127.0.0.1:{http_port}");
    println!("simulator UI at {url} (devices on [::1]:{port})");
    if open {
        let url = url.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(1500));
            let _ = open_browser(&url);
        });
    }

    let args = [
        "--port".to_string(),
        port.to_string(),
        "--http-port".to_string(),
        http_port.to_string(),
        "--web-dir".to_string(),
        public.to_string_lossy().into_owned(),
    ];
    run(&bin.to_string_lossy(), &args, sim)
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
