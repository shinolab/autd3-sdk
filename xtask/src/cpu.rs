use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Subcommand;

use crate::clean::{CleanArgs, Cleaner};
use crate::cpu_codegen::gen_param;
use crate::util::{cargo_clippy, cargo_fmt, run, run_env};

#[derive(Subcommand)]
pub(crate) enum CpuCmd {
    #[command(
        about = "Build the loader and the application from `board` and merge them into the flashable `.bin`"
    )]
    Build,
    #[command(about = "Build, then write the `.bin` to the device with `firmware-writer` (J-Link)")]
    Flash,
    #[command(about = "Run the portable firmware logic (`autd3-cpu-fw`) tests on the host")]
    Test {
        #[arg(
            long,
            help = "Model-check the ISR/main-loop FIFO handoff with loom instead of the regular tests"
        )]
        loom: bool,
    },
    #[command(about = "Regenerate `fw/src/fpga_params.rs` from the FPGA `params.svh`")]
    GenParam,
    #[command(about = "Clippy the firmware")]
    Lint {
        #[arg(long, help = "Clippy the loom model instead of the regular targets")]
        loom: bool,
    },
    #[command(about = "Rustfmt the firmware")]
    Format {
        #[arg(long, help = "Rewrite the files instead of only checking them")]
        fix: bool,
    },
    #[command(about = "Remove the CPU firmware build outputs")]
    Clean(CleanArgs),
}

pub(crate) fn run_cpu(root: &Path, cmd: &CpuCmd) -> Result<()> {
    match cmd {
        CpuCmd::Build => cpu_build(root).map(|_| ()),
        CpuCmd::Flash => cpu_flash(root),
        CpuCmd::Test { loom } => cpu_test(root, *loom),
        CpuCmd::GenParam => gen_param(root),
        CpuCmd::Lint { loom } => cpu_lint(root, *loom),
        CpuCmd::Format { fix } => {
            cargo_fmt(root, &["-p", "autd3-cpu-fw"], *fix)?;
            cargo_fmt(&board_dir(root), &[], *fix)
        }
        CpuCmd::Clean(args) => crate::clean::scope(root, *args, clean),
    }
}

pub(crate) fn clean(cleaner: &mut Cleaner) -> Result<()> {
    cleaner.paths(&["firmware/cpu/build", "firmware/cpu/board/target"])
}

pub(crate) const CPU_TARGET: &str = "armv7r-none-eabi";

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
    crate::firmware::run_writer(root, [OsStr::new("--cpu"), bin.as_os_str()])?;
    println!("flash complete: {}", bin.display());
    Ok(())
}

pub(crate) fn cpu_build(root: &Path) -> Result<PathBuf> {
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

    let loader_elf = board
        .join("target")
        .join(CPU_TARGET)
        .join("release/autd3-cpu-loader");
    if !loader_elf.exists() {
        bail!("{} not found after cargo build", loader_elf.display());
    }

    let elf = build_dir.join("autd3-cpu.x");
    let map_flag = format!("-Wl,-Map={}", build_dir.join("autd3-cpu.map").display());
    let script_flag = format!("-T{}", linker_script.display());
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
        "-Wl,--undefined=_vector_table",
        &staticlib_str,
        "-o",
        &elf_str,
    ]);
    run(&cc, args, root)?;

    let loader = objcopy_binary(
        &objcopy,
        &loader_elf,
        &build_dir.join("autd3-cpu-loader.bin"),
        root,
    )?;
    let app = objcopy_binary(&objcopy, &elf, &build_dir.join("autd3-cpu-app.bin"), root)?;
    let app_at = autd3_cpu_wire::update::Slot::A.image_base() as usize;
    if loader.len() > autd3_cpu_wire::update::LOADER_REGION_END as usize {
        bail!(
            "the loader is {} bytes; it overruns the loader region",
            loader.len()
        );
    }
    let mut image = loader;
    image.resize(app_at, 0xFF);
    image.extend_from_slice(&app);
    let bin = build_dir.join("autd3-cpu.bin");
    std::fs::write(&bin, &image).with_context(|| format!("writing {}", bin.display()))?;

    stamp_image(&bin)?;

    println!("firmware built: {}", bin.display());
    Ok(bin)
}

fn objcopy_binary(objcopy: &str, elf: &Path, bin: &Path, cwd: &Path) -> Result<Vec<u8>> {
    let elf_str = elf.to_string_lossy().into_owned();
    let bin_str = bin.to_string_lossy().into_owned();
    run(
        objcopy,
        ["-O", "binary", "--gap-fill", "0xff", &elf_str, &bin_str],
        cwd,
    )?;
    std::fs::read(bin).with_context(|| format!("reading {}", bin.display()))
}

fn stamp_image(bin: &Path) -> Result<()> {
    use autd3_cpu_wire::update::{ImageHeader, SLOT_IMAGE_CAPACITY, Slot, crc32};
    use zerocopy::IntoBytes;

    let mut image = std::fs::read(bin).with_context(|| format!("reading {}", bin.display()))?;
    let header_at = Slot::A.base() as usize;
    let body_at = Slot::A.image_base() as usize;
    if image.len() <= body_at {
        bail!(
            "{} is {} bytes; it does not reach the slot-A image at 0x{body_at:X}",
            bin.display(),
            image.len()
        );
    }
    let body_len = image.len() - body_at;
    let Some(body_len) = u32::try_from(body_len)
        .ok()
        .filter(|&len| len <= SLOT_IMAGE_CAPACITY)
    else {
        bail!("slot-A image is {body_len} bytes, over the {SLOT_IMAGE_CAPACITY}-byte capacity");
    };
    let header = ImageHeader::new(0, body_len, crc32(&image[body_at..]));
    let slot = &mut image[header_at..][..core::mem::size_of::<ImageHeader>()];
    if slot != header.as_bytes() && slot.iter().any(|&b| b != 0xFF) {
        bail!(
            "{} already carries a different slot-A header",
            bin.display()
        );
    }
    slot.copy_from_slice(header.as_bytes());
    std::fs::write(bin, &image).with_context(|| format!("writing {}", bin.display()))?;
    println!(
        "stamped slot-A header: length {body_len} bytes, crc32 0x{:08X}",
        header.crc32.get()
    );
    Ok(())
}

fn cpu_test(root: &Path, loom: bool) -> Result<()> {
    gen_param(root)?;
    if !loom {
        return run("cargo", ["test", "-p", "autd3-cpu-fw"], root);
    }
    run_env(
        "cargo",
        ["test", "-p", "autd3-cpu-fw", "--lib"],
        root,
        &[("RUSTFLAGS", OsStr::new("--cfg loom"))],
    )
}

fn cpu_lint(root: &Path, loom: bool) -> Result<()> {
    gen_param(root)?;
    if loom {
        return run_env(
            "cargo",
            [
                "clippy",
                "-p",
                "autd3-cpu-fw",
                "--lib",
                "--profile",
                "test",
                "--",
                "-D",
                "warnings",
            ],
            root,
            &[("RUSTFLAGS", OsStr::new("--cfg loom"))],
        );
    }
    cargo_clippy(root, &["-p", "autd3-cpu-fw", "--all-targets"])?;
    cargo_clippy(&board_dir(root), &["--release"])
}
