use std::ffi::OsStr;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Subcommand;

use crate::clean::{CleanArgs, Cleaner};
use crate::util::{run, run_env, which};

#[derive(Subcommand)]
pub enum CpuCmd {
    /// Build `board` and link it with `platform/autd3-platform.o` into the flashable `.bin`
    Build {
        /// Toggle PORTA pin 5 around the EtherCAT ISR frame handler so its width can be scoped
        #[arg(long)]
        isr_probe: bool,
    },
    /// Build, then write the `.bin` to the device with J-Link
    Flash {
        /// Toggle PORTA pin 5 around the EtherCAT ISR frame handler so its width can be scoped
        #[arg(long)]
        isr_probe: bool,
    },
    /// Run the portable firmware logic (`autd3-cpu-fw`) tests on the host
    Test {
        /// Model-check the ISR/main-loop FIFO handoff with loom instead of the regular tests
        #[arg(long)]
        loom: bool,
    },
    /// Regenerate `fw/src/params.rs` from the FPGA `params.svh`
    GenParam,
    /// Write the slot-A image header (magic/generation 0/length/CRC32) into a linked `.bin`
    StampImage {
        /// The flash image produced by the final link (`autd3-cpu.bin`)
        bin: PathBuf,
    },
    /// Clippy the firmware
    Lint {
        /// Clippy the loom model instead of the regular targets
        #[arg(long)]
        loom: bool,
    },
    /// Rustfmt the firmware
    Format {
        /// Rewrite the files instead of only checking them
        #[arg(long)]
        fix: bool,
    },
    #[command(about = "Remove the CPU firmware build outputs")]
    Clean(CleanArgs),
}

pub fn run_cpu(root: &Path, cmd: &CpuCmd) -> Result<()> {
    match cmd {
        CpuCmd::Build { isr_probe } => cpu_build(root, *isr_probe).map(|_| ()),
        CpuCmd::Flash { isr_probe } => cpu_flash(root, *isr_probe),
        CpuCmd::Test { loom } => cpu_test(root, *loom),
        CpuCmd::GenParam => gen_param(root),
        CpuCmd::StampImage { bin } => stamp_image(bin),
        CpuCmd::Lint { loom } => cpu_lint(root, *loom),
        CpuCmd::Format { fix } => cpu_format(root, *fix),
        CpuCmd::Clean(args) => crate::clean::scope(root, *args, clean),
    }
}

pub fn clean(cleaner: &mut Cleaner) -> Result<()> {
    cleaner.paths(&["firmware/cpu/build", "firmware/cpu/board/target"])
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

fn cpu_flash(root: &Path, isr_probe: bool) -> Result<()> {
    let bin = cpu_build(root, isr_probe)?;

    let jlink = match std::env::var("JLINK") {
        Ok(v) if !v.is_empty() => v,
        _ => ["JLinkExe", "JLink"]
            .into_iter()
            .find_map(which)
            .map(|path| path.to_string_lossy().into_owned())
            .context(
                "J-Link Commander (JLinkExe) not found on PATH (install J-Link or set JLINK)",
            )?,
    };

    let slot_b_start = 0x3000_0000 + autd3_cpu_wire::update::SLOT_B_BASE;
    let slot_b_last = slot_b_start + autd3_cpu_wire::update::SLOT_BYTES - 1;
    let script = format!(
        "r\nexec EnableEraseAllFlashBanks\nerase 0x{slot_b_start:X} 0x{slot_b_last:X}\nloadfile {} 0x30000000\nr\ng\nq\n",
        bin.to_string_lossy().replace('\\', "/")
    );
    let script_path = root.join("firmware/cpu/build/flash.jlink");
    std::fs::write(&script_path, script)
        .with_context(|| format!("writing {}", script_path.display()))?;

    run_jlink(&jlink, &script_path, root)
        .context("J-Link failed. Make sure the AUTD3 is connected and powered on.")?;
    println!("flash complete: {}", bin.display());
    Ok(())
}

const JLINK_NO_FLASH_BANK: &str = "No Flash bank within given address range";

fn run_jlink(jlink: &str, script: &Path, cwd: &Path) -> Result<()> {
    let mut child = std::process::Command::new(jlink)
        .args([
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
        ])
        .arg(script)
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn `{jlink}` (is it installed and on PATH?)"))?;
    let stdout = child.stdout.take().context("J-Link stdout is not piped")?;
    let stderr = child.stderr.take().context("J-Link stderr is not piped")?;
    let out = std::thread::spawn(move || {
        let mut log = String::new();
        tee_lines(stdout, &mut log);
        log
    });
    let err = std::thread::spawn(move || {
        let mut log = String::new();
        tee_lines(stderr, &mut log);
        log
    });
    let status = child.wait().context("waiting for J-Link")?;
    let mut log = out.join().unwrap_or_default();
    log.push_str(&err.join().unwrap_or_default());
    if !status.success() {
        bail!("`{jlink}` exited with {status}");
    }
    if log.contains(JLINK_NO_FLASH_BANK) {
        bail!(
            "J-Link did not erase the serial flash: {JLINK_NO_FLASH_BANK}. \
             The QSPI bank stayed untouched, so an OTA image in slot B would still win the boot."
        );
    }
    Ok(())
}

fn tee_lines(reader: impl std::io::Read, log: &mut String) {
    for line in std::io::BufReader::new(reader).lines() {
        let Ok(line) = line else { break };
        eprintln!("{line}");
        log.push_str(&line);
        log.push('\n');
    }
}

pub fn cpu_build(root: &Path, isr_probe: bool) -> Result<PathBuf> {
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
    let mut build_args = vec!["build", "--release"];
    if isr_probe {
        build_args.push("--features");
        build_args.push("isr-probe");
    }
    run("cargo", build_args, &board).context(
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

    stamp_image(&bin)?;

    println!("firmware built: {}", bin.display());
    Ok(bin)
}

pub fn stamp_image(bin: &Path) -> Result<()> {
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
    if body_len > SLOT_IMAGE_CAPACITY as usize {
        bail!("slot-A image is {body_len} bytes, over the {SLOT_IMAGE_CAPACITY}-byte capacity");
    }
    let header = ImageHeader::new(0, body_len as u32, crc32(&image[body_at..]));
    let slot = &mut image[header_at..header_at + core::mem::size_of::<ImageHeader>()];
    if slot != header.as_bytes() && slot.iter().any(|&b| b != 0xFF) {
        bail!("{} already carries a different slot-A header", bin.display());
    }
    slot.copy_from_slice(header.as_bytes());
    std::fs::write(bin, &image).with_context(|| format!("writing {}", bin.display()))?;
    println!(
        "stamped slot-A header: length {body_len} bytes, crc32 0x{:08X}",
        header.crc32.get()
    );
    Ok(())
}

pub fn gen_param(root: &Path) -> Result<()> {
    crate::cpu_codegen::gen_param(root)
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
