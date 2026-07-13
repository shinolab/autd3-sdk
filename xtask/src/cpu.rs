use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Subcommand;

use crate::util::{on_path, run};

#[derive(Subcommand)]
pub enum CpuCmd {
    Build,
    Flash,
    Test,
    Lint,
    Format {
        #[arg(long)]
        fix: bool,
    },
}

pub fn run_cpu(root: &Path, cmd: &CpuCmd) -> Result<()> {
    match cmd {
        CpuCmd::Build => cpu_build(root).map(|_| ()),
        CpuCmd::Flash => cpu_flash(root),
        CpuCmd::Test => cpu_test(root),
        CpuCmd::Lint => cpu_lint(root),
        CpuCmd::Format { fix } => cpu_format(root, *fix),
    }
}

pub const CPU_TARGET: &str = "armv7r-none-eabi";

const CPU_LINK_FLAGS: &[&str] = &[
    "-mcpu=cortex-r4f",
    "-march=armv7-r",
    "-marm",
    "-mlittle-endian",
    "-mthumb-interwork",
    "-mfloat-abi=soft",
    "-mfpu=vfpv3",
];

fn board_dir(root: &Path) -> PathBuf {
    root.join("firmware/cpu/board")
}

fn cpu_flash(root: &Path) -> Result<()> {
    let bin = cpu_build(root)?;

    let jlink = match std::env::var("JLINK") {
        Ok(v) if !v.is_empty() => v,
        _ => ["JLinkExe", "JLink"]
            .into_iter()
            .find(|c| on_path(c))
            .map(str::to_string)
            .context(
                "J-Link Commander (JLinkExe) not found on PATH (install J-Link or set JLINK)",
            )?,
    };

    let script = format!(
        "r\nloadfile {} 0x30000000\nr\ng\nq\n",
        bin.to_string_lossy().replace('\\', "/")
    );
    let script_path = root.join("firmware/cpu/build/flash.jlink");
    std::fs::write(&script_path, script)
        .with_context(|| format!("writing {}", script_path.display()))?;

    run(
        &jlink,
        [
            "-device",
            "R7S910018_R4F",
            "-if",
            "JTAG",
            "-speed",
            "4000",
            "-jtagconf",
            "-1,-1",
            "-autoconnect",
            "1",
            "-ExitOnError",
            "1",
            "-CommanderScript",
            &script_path.to_string_lossy(),
        ],
        root,
    )
    .context("J-Link failed. Make sure the AUTD3 is connected and powered on.")?;
    println!("flash complete: {}", bin.display());
    Ok(())
}

pub fn cpu_build(root: &Path) -> Result<PathBuf> {
    gen_param(root)?;

    let prefix = std::env::var("CROSS_COMPILE").unwrap_or_else(|_| "arm-none-eabi-".to_string());
    let cc = format!("{prefix}gcc");
    let objcopy = format!("{prefix}objcopy");
    if std::process::Command::new(&cc)
        .arg("--version")
        .output()
        .is_err()
    {
        bail!("{cc} not found on PATH (install the Arm GNU toolchain or set CROSS_COMPILE)");
    }

    let cpu_dir = root.join("firmware/cpu");
    let platform_obj = cpu_dir.join("platform/autd3-platform.o");
    if !platform_obj.exists() {
        bail!(
            "{} not found (it is committed to the repository; check your checkout)",
            platform_obj.display()
        );
    }
    let linker_script = cpu_dir.join("platform/autd3-cpu.ld");
    let build_dir = cpu_dir.join("build");
    std::fs::create_dir_all(&build_dir)
        .with_context(|| format!("creating {}", build_dir.display()))?;

    let board = board_dir(root);
    run("cargo", ["build", "--release"], &board).context(
        "building the firmware staticlib failed \
         (is the target installed? `rustup target add armv7r-none-eabi`)",
    )?;
    let staticlib = board
        .join("target")
        .join(CPU_TARGET)
        .join("release/libautd3_cpu.a");
    if !staticlib.exists() {
        bail!("{} not found after cargo build", staticlib.display());
    }

    let elf = build_dir.join("autd3-cpu.x");
    let map_flag = format!("-Wl,-Map={}", build_dir.join("autd3-cpu.map").display());
    let script_flag = format!("-T{}", linker_script.display());
    let platform_str = platform_obj.to_string_lossy().into_owned();
    let staticlib_str = staticlib.to_string_lossy().into_owned();
    let elf_str = elf.to_string_lossy().into_owned();
    let mut args: Vec<&str> = Vec::new();
    args.extend(CPU_LINK_FLAGS);
    args.extend([
        "-nostartfiles",
        "--specs=nosys.specs",
        &script_flag,
        &map_flag,
        "-Wl,--no-warn-rwx-segments",
        "-Wl,-z,noexecstack",
        &platform_str,
        &staticlib_str,
        "-o",
        &elf_str,
    ]);
    run(&cc, args, root)?;

    let bin = build_dir.join("autd3-cpu.bin");
    let bin_str = bin.to_string_lossy().into_owned();
    run(
        &objcopy,
        ["-O", "binary", "--gap-fill", "0xff", &elf_str, &bin_str],
        root,
    )?;

    println!("firmware built: {}", bin.display());
    Ok(bin)
}

pub fn gen_param(root: &Path) -> Result<()> {
    run("python3", ["gen_param.py"], &root.join("firmware/cpu"))
}

fn cpu_test(root: &Path) -> Result<()> {
    gen_param(root)?;
    run("cargo", ["test", "-p", "autd3-cpu-fw"], root)
}

fn cpu_lint(root: &Path) -> Result<()> {
    gen_param(root)?;
    run(
        "cargo",
        [
            "clippy",
            "-p",
            "autd3-cpu-fw",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
        root,
    )?;
    run(
        "cargo",
        ["clippy", "--release", "--", "-D", "warnings"],
        &board_dir(root),
    )
}

fn cpu_format(root: &Path, fix: bool) -> Result<()> {
    let mut args = vec!["fmt", "-p", "autd3-cpu-fw"];
    if !fix {
        args.extend(["--", "--check"]);
    }
    run("cargo", args, root)?;

    let mut board_args = vec!["fmt"];
    if !fix {
        board_args.extend(["--", "--check"]);
    }
    run("cargo", board_args, &board_dir(root))
}
