use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Subcommand;

use crate::fpga::resolve_vivado;
use crate::util::run;

const MAGIC: u128 = 0xA55A;
const SIGNATURE: u128 = 0b10_1101;
const EXPECTED_JEDEC_ID: u64 = 0x20_BA18;
const FLASH_SIZE: u64 = 0x100_0000;
const WRITABLE_BASE: u32 = 0x80_0000;
const SECTOR_SIZE: u64 = 0x1000;
const SIM_TOP: &str = "sim_flash_probe";

const FSR_ERASE_FAILURE: u8 = 1 << 5;
const FSR_PROGRAM_FAILURE: u8 = 1 << 4;
const FSR_PROTECTION: u8 = 1 << 1;

#[derive(Subcommand)]
pub enum FlashProbeCmd {
    #[command(about = "Simulate the probe core against a behavioral SPI flash model")]
    Sim,
    #[command(about = "Synthesize and implement the probe bitstream (independent of the main project)")]
    Build {
        #[arg(long, help = "Build an SPI x1 variant (flash_probe_x1.bit) for MultiBoot experiments")]
        spi_x1: bool,
        #[arg(
            long,
            value_parser = parse_u32,
            conflicts_with = "spi_x1",
            help = "Build flash_probe_mb.bit with BITSTREAM.CONFIG.NEXT_CONFIG_ADDR set (bitstream-embedded MultiBoot)"
        )]
        next_config_addr: Option<u32>,
    },
    #[command(about = "Load the probe bitstream over JTAG (volatile; the SPI flash is not touched)")]
    Load,
    #[command(about = "Read the JEDEC ID of the configuration flash")]
    Id,
    #[command(about = "Read the status register of the configuration flash")]
    Sr,
    #[command(about = "Read the flag status register of the configuration flash")]
    Fsr,
    #[command(about = "Read 8 bytes from the configuration flash")]
    Read {
        #[arg(long, value_parser = parse_u32)]
        addr: u32,
    },
    #[command(about = "Compute the CRC32 of a flash range on the FPGA")]
    Crc {
        #[arg(long, value_parser = parse_u32)]
        addr: u32,
        #[arg(long, value_parser = parse_u32)]
        len: u32,
    },
    #[command(about = "Compare the flash with an .mcs file by the CRC32 of its whole extent (computed on the FPGA)")]
    Verify {
        #[arg(long, help = "Configuration memory file [default: firmware/fpga/autd3-fpga.mcs]")]
        mcs: Option<PathBuf>,
    },
    #[command(about = "Erase the 4 KiB sectors covering a range (only within [0x800000, 0x1000000))")]
    Erase {
        #[arg(long, value_parser = parse_u32)]
        addr: u32,
        #[arg(long, value_parser = parse_u32)]
        len: u32,
    },
    #[command(about = "Program a pseudo-random pattern into an erased range (only within [0x800000, 0x1000000))")]
    Program {
        #[arg(long, value_parser = parse_u32)]
        addr: u32,
        #[arg(long, value_parser = parse_u32)]
        len: u32,
        #[arg(long, default_value_t = 1)]
        seed: u8,
    },
    #[command(about = "Erase, program and read back a range, checking that everything below 0x800000 is untouched")]
    WriteTest {
        #[arg(long, default_value = "0xF00000", value_parser = parse_u32)]
        addr: u32,
        #[arg(long, default_value = "0x10000", value_parser = parse_u32)]
        len: u32,
        #[arg(long, default_value_t = 1)]
        seed: u8,
    },
    #[command(
        about = "Write the image of an .mcs file at a MultiBoot address with Vivado over JTAG, then reload the probe and check both copies"
    )]
    StageImage {
        #[arg(
            long,
            required_unless_present = "bit",
            conflicts_with = "bit",
            help = "Configuration memory file holding the image to stage"
        )]
        mcs: Option<PathBuf>,
        #[arg(long, help = "Bitstream to stage (converted to an SPIx4 image first)")]
        bit: Option<PathBuf>,
        #[arg(long, default_value = "0x800000", value_parser = parse_u32)]
        addr: u32,
        #[arg(long, value_parser = parse_u32, help = "Flip bit 0 of the byte at this image offset before writing")]
        corrupt_at: Option<u32>,
        #[arg(long, help = "Replace the BSPI write and BSPI_READ command after the sync word with NOOPs")]
        strip_bspi: bool,
    },
    #[command(
        about = "Overwrite address 0 with a bitstream (the probe by default) and stage an update image, so that the flash-booted design can issue IPROG"
    )]
    StageMultiboot {
        #[arg(long, help = "Bitstream written at address 0 [default: the probe bitstream]")]
        golden_bit: Option<PathBuf>,
        #[arg(long, help = "Configuration memory file holding the update image")]
        update_mcs: PathBuf,
        #[arg(long, default_value = "0x800000", value_parser = parse_u32)]
        update_addr: u32,
        #[arg(long, help = "Required: acknowledge that the image at address 0 is replaced")]
        overwrite_address_zero: bool,
    },
    #[command(about = "Reconfigure the FPGA from a flash address through ICAPE2 (IPROG), then show the boot status")]
    Iprog {
        #[arg(long, value_parser = parse_u32)]
        addr: u32,
        #[arg(long, value_parser = parse_u32, help = "Raw value written to the TIMER register before IPROG")]
        watchdog: Option<u32>,
        #[arg(long, default_value_t = 5000, help = "Time to wait before reading the boot status")]
        wait_ms: u64,
    },
    #[command(about = "Write WBSTAR (and TIMER) through ICAPE2 without IPROG, then read the registers back over JTAG")]
    WriteWbstar {
        #[arg(long, value_parser = parse_u32)]
        addr: u32,
        #[arg(long, value_parser = parse_u32, help = "Raw value written to the TIMER register")]
        watchdog: Option<u32>,
    },
    #[command(about = "Show the BOOT_STATUS and CONFIG_STATUS registers read over JTAG")]
    BootStatus,
}

#[derive(Clone, Copy)]
#[repr(u8)]
enum Op {
    Id = 0x01,
    Sr = 0x02,
    Read = 0x03,
    Crc = 0x04,
    Fsr = 0x05,
    Erase = 0x06,
    Program = 0x07,
    Iprog = 0x08,
    Wbstar = 0x09,
}

#[derive(Clone, Copy)]
struct Request {
    op: Op,
    addr: u32,
    len: u32,
}

impl Request {
    const fn new(op: Op, addr: u32, len: u32) -> Self {
        Self { op, addr, len }
    }

    fn tdi(self) -> String {
        let tdi = (MAGIC << 64)
            | (u128::from(self.len) << 32)
            | (u128::from(self.addr & 0xFF_FFFF) << 8)
            | u128::from(self.op as u8);
        format!("{tdi:020X}")
    }

    fn timeout_ms(self) -> u64 {
        let len = u64::from(self.len & 0xFF_FFFF);
        match self.op {
            Op::Crc => 10_000 + u64::from(self.len) / 50,
            Op::Erase => 10_000 + len.div_ceil(SECTOR_SIZE) * 6_000,
            Op::Program => 10_000 + len.div_ceil(256) * 6_000,
            Op::Id | Op::Sr | Op::Fsr | Op::Read | Op::Iprog | Op::Wbstar => 10_000,
        }
    }
}

pub fn run_flash_probe(fpga_dir: &Path, cmd: &FlashProbeCmd) -> Result<()> {
    let probe_dir = fpga_dir.join("flash-probe");
    match cmd {
        FlashProbeCmd::Sim => sim(&probe_dir),
        FlashProbeCmd::Build {
            spi_x1,
            next_config_addr,
        } => build(&probe_dir, *spi_x1, *next_config_addr),
        FlashProbeCmd::Load => load(&probe_dir),
        FlashProbeCmd::Id => {
            let id = exec(&probe_dir, Request::new(Op::Id, 0, 0))? & 0xFF_FFFF;
            println!(
                "JEDEC ID: {:02X} {:02X} {:02X}",
                id >> 16,
                (id >> 8) & 0xFF,
                id & 0xFF
            );
            if id != EXPECTED_JEDEC_ID {
                bail!("unexpected JEDEC ID (expected 20 BA 18 for MT25QL128)");
            }
            println!("matches MT25QL128");
            Ok(())
        }
        FlashProbeCmd::Sr => {
            let sr = exec(&probe_dir, Request::new(Op::Sr, 0, 0))?.to_le_bytes()[0];
            println!("status register: 0x{sr:02X}");
            Ok(())
        }
        FlashProbeCmd::Fsr => {
            let fsr = exec(&probe_dir, Request::new(Op::Fsr, 0, 0))?.to_le_bytes()[0];
            println!("flag status register: 0x{fsr:02X} ({})", describe_fsr(fsr));
            Ok(())
        }
        FlashProbeCmd::Read { addr } => {
            check_range(*addr, 8)?;
            let data = exec(&probe_dir, Request::new(Op::Read, *addr, 0))?;
            let bytes = data
                .to_be_bytes()
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect::<Vec<_>>()
                .join(" ");
            println!("0x{addr:06X}: {bytes}");
            Ok(())
        }
        FlashProbeCmd::Crc { addr, len } => {
            check_range(*addr, *len)?;
            let crc = exec(&probe_dir, Request::new(Op::Crc, *addr, *len))? & 0xFFFF_FFFF;
            println!(
                "CRC32 [0x{addr:06X}, 0x{:06X}): 0x{crc:08X}",
                u64::from(*addr) + u64::from(*len)
            );
            Ok(())
        }
        FlashProbeCmd::Verify { mcs } => {
            let mcs = mcs
                .clone()
                .unwrap_or_else(|| fpga_dir.join("autd3-fpga.mcs"));
            verify(&probe_dir, &mcs)
        }
        FlashProbeCmd::Erase { addr, len } => {
            check_writable(*addr, *len)?;
            let result = exec(&probe_dir, Request::new(Op::Erase, *addr, *len))?;
            check_write("erase", result)?;
            let (start, erased) = sector_span(*addr, *len);
            println!(
                "erased [0x{start:06X}, 0x{:06X})",
                u64::from(start) + u64::from(erased)
            );
            Ok(())
        }
        FlashProbeCmd::Program { addr, len, seed } => {
            check_writable(*addr, *len)?;
            let result = exec(
                &probe_dir,
                Request::new(Op::Program, *addr, (u32::from(*seed) << 24) | len),
            )?;
            check_write("program", result)?;
            println!(
                "programmed [0x{addr:06X}, 0x{:06X}) with seed {seed}; expected CRC32 0x{:08X}",
                u64::from(*addr) + u64::from(*len),
                crc32(&pattern(*addr, *len, *seed))
            );
            Ok(())
        }
        FlashProbeCmd::WriteTest { addr, len, seed } => write_test(&probe_dir, *addr, *len, *seed),
        FlashProbeCmd::StageImage {
            mcs,
            bit,
            addr,
            corrupt_at,
            strip_bspi,
        } => {
            let source = match (mcs, bit) {
                (Some(mcs), _) => read_mcs(mcs)?.1,
                (None, Some(bit)) => bit_to_image(&probe_dir, bit)?,
                (None, None) => bail!("either --mcs or --bit is required"),
            };
            stage_image(&probe_dir, source, *addr, *corrupt_at, *strip_bspi)
        }
        FlashProbeCmd::StageMultiboot {
            golden_bit,
            update_mcs,
            update_addr,
            overwrite_address_zero,
        } => {
            if !overwrite_address_zero {
                bail!(
                    "stage-multiboot replaces the image at address 0; pass --overwrite-address-zero to confirm \
                     (restore it afterwards with `cargo xtask fpga flash`)"
                );
            }
            let golden_bit = golden_bit.clone().unwrap_or_else(|| bit_file(&probe_dir));
            stage_multiboot(&probe_dir, &golden_bit, update_mcs, *update_addr)
        }
        FlashProbeCmd::Iprog {
            addr,
            watchdog,
            wait_ms,
        } => iprog(&probe_dir, *addr, *watchdog, *wait_ms),
        FlashProbeCmd::WriteWbstar { addr, watchdog } => {
            check_range(*addr, 0)?;
            exec(
                &probe_dir,
                Request::new(Op::Wbstar, *addr, watchdog.unwrap_or(0)),
            )?;
            println!(
                "wrote WBSTAR=0x{addr:08X}{} through ICAPE2 without IPROG; reading the registers back",
                watchdog.map_or(String::new(), |w| format!(" and TIMER=0x{w:08X}"))
            );
            let stdout = vivado_jtag(&probe_dir, &["status".to_string()])?;
            print_status(&stdout);
            Ok(())
        }
        FlashProbeCmd::BootStatus => {
            let stdout = vivado_jtag(&probe_dir, &["status".to_string()])?;
            print_status(&stdout);
            Ok(())
        }
    }
}

fn bit_to_image(probe_dir: &Path, bit: &Path) -> Result<Vec<u8>> {
    let bit =
        std::path::absolute(bit).with_context(|| format!("resolving {}", bit.display()))?;
    if !bit.is_file() {
        bail!("{} not found", bit.display());
    }
    let dir = build_dir(probe_dir)?;
    let converted = dir.join("source-from-bit.mcs");
    let log = dir.join("bit2mcs.log");
    run(
        &resolve_vivado()?,
        [
            "-mode",
            "batch",
            "-nojournal",
            "-log",
            &log.to_string_lossy(),
            "-source",
            "scripts/bit2mcs.tcl",
            "-tclargs",
            &bit.to_string_lossy(),
            &converted.to_string_lossy(),
        ],
        probe_dir,
    )
    .context("Vivado failed while converting the bitstream")?;
    let (start, image) = read_mcs(&converted)?;
    if start != 0 {
        bail!("converted image unexpectedly starts at 0x{start:X}");
    }
    Ok(image)
}

fn strip_bspi_read(image: &mut [u8]) -> Result<()> {
    const SYNC: [u8; 4] = [0xAA, 0x99, 0x55, 0x66];
    const NOOP: [u8; 4] = [0x20, 0x00, 0x00, 0x00];
    const WRITE_BSPI: [u8; 4] = [0x30, 0x03, 0xE0, 0x01];
    const WRITE_CMD: [u8; 4] = [0x30, 0x00, 0x80, 0x01];
    const CMD_BSPI_READ: [u8; 4] = [0x00, 0x00, 0x00, 0x12];

    let sync = image
        .windows(4)
        .position(|w| w == SYNC)
        .context("sync word not found in the image")?;
    let start = sync + 8;
    let words = image
        .get(sync + 4..start + 16)
        .context("image ends right after the sync word")?;
    if words[0..4] != NOOP
        || words[4..8] != WRITE_BSPI
        || words[12..16] != WRITE_CMD
        || words[16..20] != CMD_BSPI_READ
    {
        bail!("the header after the sync word is not NOOP, BSPI write, BSPI_READ");
    }
    for word in image[start..start + 16].chunks_exact_mut(4) {
        word.copy_from_slice(&NOOP);
    }
    println!("replaced the BSPI write and BSPI_READ at image offset 0x{start:06X} with NOOPs");
    Ok(())
}

fn require_probe_bit(probe_dir: &Path) -> Result<PathBuf> {
    let bit = bit_file(probe_dir);
    if !bit.is_file() {
        bail!(
            "{} not found; run `cargo xtask fpga flash-probe build` first",
            bit.display()
        );
    }
    Ok(bit)
}

fn write_images(probe_dir: &Path, name: &str, images: &[(u32, &[u8])]) -> Result<()> {
    let bit = require_probe_bit(probe_dir)?;
    let dir = build_dir(probe_dir)?;
    let mcs = dir.join(format!("{name}.mcs"));
    let mut tcl_args = vec![
        "stage".to_string(),
        mcs.to_string_lossy().into_owned(),
        bit.to_string_lossy().into_owned(),
    ];
    for (index, (addr, image)) in images.iter().enumerate() {
        let bin = dir.join(format!("{name}-{index}.bin"));
        std::fs::write(&bin, image).with_context(|| format!("writing {}", bin.display()))?;
        println!(
            "writing {} bytes at [0x{addr:06X}, 0x{:06X})",
            image.len(),
            u64::from(*addr) + image.len() as u64
        );
        tcl_args.push(format!("0x{addr:08X}"));
        tcl_args.push(bin.to_string_lossy().into_owned());
    }

    let vivado = resolve_vivado()?;
    let log = dir.join("stage.log");
    let mut args = vec![
        "-mode".to_string(),
        "batch".to_string(),
        "-nojournal".to_string(),
        "-log".to_string(),
        log.to_string_lossy().into_owned(),
        "-source".to_string(),
        "scripts/jtag.tcl".to_string(),
        "-tclargs".to_string(),
    ];
    args.extend(tcl_args);
    run(&vivado, args, probe_dir).context("Vivado failed while writing the flash")
}

fn stage_image(
    probe_dir: &Path,
    source: Vec<u8>,
    addr: u32,
    corrupt_at: Option<u32>,
    strip_bspi: bool,
) -> Result<()> {
    let len = u32::try_from(source.len()).context("image too large")?;
    check_writable(addr, len)?;

    let mut image = source.clone();
    if strip_bspi {
        strip_bspi_read(&mut image)?;
    }
    if let Some(offset) = corrupt_at {
        let byte = image
            .get_mut(usize::try_from(offset)?)
            .with_context(|| format!("--corrupt-at 0x{offset:X} is outside the {len}-byte image"))?;
        let original = *byte;
        *byte ^= 0x01;
        println!(
            "corrupting image offset 0x{offset:06X} (flash 0x{:06X}): 0x{original:02X} -> 0x{:02X}",
            u64::from(addr) + u64::from(offset),
            *byte
        );
    }

    write_images(probe_dir, "staged", &[(addr, &image)])?;

    let results = exec_many(
        probe_dir,
        &[
            Request::new(Op::Crc, addr, len),
            Request::new(Op::Crc, 0, len),
        ],
    )?;
    let staged = results[0] & 0xFFFF_FFFF;
    let staged_expected = u64::from(crc32(&image));
    let golden = results[1] & 0xFFFF_FFFF;
    let source_crc = u64::from(crc32(&source));
    println!(
        "staged copy [0x{addr:06X}, +0x{len:X}): flash 0x{staged:08X}, expected 0x{staged_expected:08X}"
    );
    println!(
        "image at 0x000000 [+0x{len:X}): flash 0x{golden:08X}, source 0x{source_crc:08X} ({})",
        if golden == source_crc {
            "same image"
        } else {
            "different image"
        }
    );
    if staged != staged_expected {
        bail!("the staged copy does not match");
    }
    println!("staged image verified");
    Ok(())
}

fn stage_multiboot(
    probe_dir: &Path,
    golden_bit: &Path,
    update_mcs: &Path,
    update_addr: u32,
) -> Result<()> {
    let golden = bit_to_image(probe_dir, golden_bit)?;
    let golden_len = u32::try_from(golden.len()).context("golden image too large")?;
    let (_, update) = read_mcs(update_mcs)?;
    let update_len = u32::try_from(update.len()).context("update image too large")?;
    check_writable(update_addr, update_len)?;
    if golden_len > update_addr {
        bail!("the golden image (0x{golden_len:X} bytes) overlaps the update image at 0x{update_addr:X}");
    }

    write_images(
        probe_dir,
        "multiboot",
        &[(0, &golden), (update_addr, &update)],
    )?;

    let results = exec_many(
        probe_dir,
        &[
            Request::new(Op::Crc, 0, golden_len),
            Request::new(Op::Crc, update_addr, update_len),
        ],
    )?;
    let golden_crc = results[0] & 0xFFFF_FFFF;
    let golden_expected = u64::from(crc32(&golden));
    let update_crc = results[1] & 0xFFFF_FFFF;
    let update_expected = u64::from(crc32(&update));
    println!("address 0 [+0x{golden_len:X}]: flash 0x{golden_crc:08X}, expected 0x{golden_expected:08X}");
    println!(
        "update [0x{update_addr:06X}, +0x{update_len:X}): flash 0x{update_crc:08X}, expected 0x{update_expected:08X}"
    );
    if golden_crc != golden_expected || update_crc != update_expected {
        bail!("the written images do not match");
    }
    println!("both images verified; power-cycle the device so that it boots from address 0");
    Ok(())
}

fn iprog(probe_dir: &Path, addr: u32, watchdog: Option<u32>, wait_ms: u64) -> Result<()> {
    check_range(addr, 0)?;
    let request = Request::new(Op::Iprog, addr, watchdog.unwrap_or(0));
    println!(
        "IPROG to 0x{addr:06X}{}",
        watchdog.map_or(String::new(), |w| format!(" with TIMER=0x{w:08X}"))
    );
    let stdout = vivado_jtag(
        probe_dir,
        &["iprog".to_string(), request.tdi(), wait_ms.to_string()],
    )?;
    print_status(&stdout);
    Ok(())
}

fn print_status(stdout: &str) {
    for line in stdout.lines().filter_map(|l| l.strip_prefix("STATUS ")) {
        println!("{line}");
    }
}

fn vivado_jtag(probe_dir: &Path, tcl_args: &[String]) -> Result<String> {
    let vivado = resolve_vivado()?;
    let log = build_dir(probe_dir)?.join("jtag.log");
    let mut args = vec![
        "-mode".to_string(),
        "batch".to_string(),
        "-nojournal".to_string(),
        "-log".to_string(),
        log.to_string_lossy().into_owned(),
        "-source".to_string(),
        "scripts/jtag.tcl".to_string(),
        "-tclargs".to_string(),
    ];
    args.extend(tcl_args.iter().cloned());
    let output = Command::new(&vivado)
        .args(&args)
        .current_dir(probe_dir)
        .output()
        .context("failed to spawn Vivado")?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if !output.status.success() {
        print!("{stdout}");
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        bail!("Vivado failed (see the log above)");
    }
    Ok(stdout)
}

fn write_test(probe_dir: &Path, addr: u32, len: u32, seed: u8) -> Result<()> {
    check_writable(addr, len)?;
    if len == 0 {
        bail!("len must be non-zero");
    }
    let (sector_start, sector_len) = sector_span(addr, len);
    let requests = [
        Request::new(Op::Crc, 0, WRITABLE_BASE),
        Request::new(Op::Erase, addr, len),
        Request::new(Op::Crc, sector_start, sector_len),
        Request::new(Op::Fsr, 0, 0),
        Request::new(Op::Program, addr, (u32::from(seed) << 24) | len),
        Request::new(Op::Crc, sector_start, sector_len),
        Request::new(Op::Fsr, 0, 0),
        Request::new(Op::Crc, 0, WRITABLE_BASE),
    ];
    println!(
        "write test on [0x{addr:06X}, 0x{:06X}) (sectors [0x{sector_start:06X}, 0x{:06X})), seed {seed}",
        u64::from(addr) + u64::from(len),
        u64::from(sector_start) + u64::from(sector_len)
    );
    println!("the protected half [0x000000, 0x{WRITABLE_BASE:06X}) is read twice; expect about a minute");
    let results = exec_many(probe_dir, &requests)?;

    let erased = vec![0xFF_u8; usize::try_from(sector_len)?];
    let mut programmed = erased.clone();
    let offset = usize::try_from(addr - sector_start)?;
    programmed[offset..offset + usize::try_from(len)?].copy_from_slice(&pattern(addr, len, seed));

    let mut failures = 0;
    let mut report = |name: &str, ok: bool, detail: String| {
        println!("{} {name}: {detail}", if ok { "PASS" } else { "FAIL" });
        if !ok {
            failures += 1;
        }
    };

    let erase = check_write("erase", results[1]);
    report("erase", erase.is_ok(), format!("{erase:?}"));
    let crc = results[2] & 0xFFFF_FFFF;
    let expected = u64::from(crc32(&erased));
    report(
        "erased sectors read back as 0xFF",
        crc == expected,
        format!("flash 0x{crc:08X}, expected 0x{expected:08X}"),
    );
    let fsr = results[3].to_le_bytes()[0];
    report(
        "flag status after erase",
        fsr & (FSR_ERASE_FAILURE | FSR_PROGRAM_FAILURE | FSR_PROTECTION) == 0,
        format!("0x{fsr:02X} ({})", describe_fsr(fsr)),
    );
    let program = check_write("program", results[4]);
    report("program", program.is_ok(), format!("{program:?}"));
    let crc = results[5] & 0xFFFF_FFFF;
    let expected = u64::from(crc32(&programmed));
    report(
        "programmed pattern reads back",
        crc == expected,
        format!("flash 0x{crc:08X}, expected 0x{expected:08X}"),
    );
    let fsr = results[6].to_le_bytes()[0];
    report(
        "flag status after program",
        fsr & (FSR_ERASE_FAILURE | FSR_PROGRAM_FAILURE | FSR_PROTECTION) == 0,
        format!("0x{fsr:02X} ({})", describe_fsr(fsr)),
    );
    let before = results[0] & 0xFFFF_FFFF;
    let after = results[7] & 0xFFFF_FFFF;
    report(
        "protected half untouched",
        before == after,
        format!("before 0x{before:08X}, after 0x{after:08X}"),
    );

    if failures > 0 {
        bail!("write test failed ({failures} check(s))");
    }
    println!("write test passed");
    Ok(())
}

fn sector_span(addr: u32, len: u32) -> (u32, u32) {
    let start = u64::from(addr) / SECTOR_SIZE * SECTOR_SIZE;
    let end = (u64::from(addr) + u64::from(len)).div_ceil(SECTOR_SIZE) * SECTOR_SIZE;
    (
        u32::try_from(start).unwrap_or(u32::MAX),
        u32::try_from(end - start).unwrap_or(u32::MAX),
    )
}

fn pattern(addr: u32, len: u32, seed: u8) -> Vec<u8> {
    let mut x = ((u32::from(seed) << 24) | (addr & 0xFF_FFFF)) ^ 0x9E37_79B9;
    if x == 0 {
        x = 1;
    }
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x.to_le_bytes()[0]
        })
        .collect()
}

fn describe_fsr(fsr: u8) -> String {
    let mut flags = Vec::new();
    if fsr & (1 << 7) != 0 {
        flags.push("ready");
    } else {
        flags.push("busy");
    }
    if fsr & FSR_ERASE_FAILURE != 0 {
        flags.push("erase failure");
    }
    if fsr & FSR_PROGRAM_FAILURE != 0 {
        flags.push("program failure");
    }
    if fsr & FSR_PROTECTION != 0 {
        flags.push("protection violation");
    }
    if fsr & 1 != 0 {
        flags.push("4-byte addressing");
    }
    flags.join(", ")
}

fn check_write(what: &str, result: u64) -> Result<u8> {
    let sr = result.to_le_bytes()[0];
    match result >> 56 {
        0 => Ok(sr),
        1 => bail!(
            "{what}: rejected by the probe's write protection (the range must lie within [0x{WRITABLE_BASE:06X}, 0x{FLASH_SIZE:06X}))"
        ),
        2 => bail!("{what}: timed out waiting for the flash to finish (SR=0x{sr:02X})"),
        code => bail!("{what}: unknown error code {code}"),
    }
}

fn check_writable(addr: u32, len: u32) -> Result<()> {
    check_range(addr, len)?;
    if addr < WRITABLE_BASE {
        bail!("refusing to write below 0x{WRITABLE_BASE:06X} (addr=0x{addr:06X})");
    }
    Ok(())
}

fn verify(probe_dir: &Path, mcs: &Path) -> Result<()> {
    let (start, image) = read_mcs(mcs)?;
    let len = u32::try_from(image.len()).context("image too large")?;
    check_range(start, len)?;
    let expected = crc32(&image);
    println!(
        "{}: [0x{start:06X}, 0x{:06X}) CRC32 0x{expected:08X}",
        mcs.display(),
        u64::from(start) + u64::from(len)
    );
    let actual = exec(probe_dir, Request::new(Op::Crc, start, len))? & 0xFFFF_FFFF;
    println!("flash: CRC32 0x{actual:08X}");
    if actual != u64::from(expected) {
        bail!("flash contents differ from {}", mcs.display());
    }
    println!("flash matches {}", mcs.display());
    Ok(())
}

pub(crate) fn read_mcs(path: &Path) -> Result<(u32, Vec<u8>)> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut records: Vec<(u32, Vec<u8>)> = Vec::new();
    let mut base = 0u32;
    for (index, line) in text.lines().enumerate() {
        let line_no = index + 1;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let hex = line
            .strip_prefix(':')
            .with_context(|| format!("line {line_no}: missing ':'"))?;
        let bytes = (0..hex.len())
            .step_by(2)
            .map(|i| hex.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok()))
            .collect::<Option<Vec<u8>>>()
            .with_context(|| format!("line {line_no}: malformed hex"))?;
        if bytes.len() < 5 || bytes.len() != usize::from(bytes[0]) + 5 {
            bail!("line {line_no}: bad record length");
        }
        if bytes.iter().fold(0u8, |acc, b| acc.wrapping_add(*b)) != 0 {
            bail!("line {line_no}: checksum mismatch");
        }
        let offset = u32::from(u16::from_be_bytes([bytes[1], bytes[2]]));
        let data = &bytes[4..bytes.len() - 1];
        match bytes[3] {
            0x00 => records.push((base + offset, data.to_vec())),
            0x01 => break,
            0x04 if data.len() == 2 => {
                base = u32::from(u16::from_be_bytes([data[0], data[1]])) << 16;
            }
            t => bail!("line {line_no}: unsupported record type 0x{t:02X}"),
        }
    }

    let start = records
        .iter()
        .map(|(addr, _)| *addr)
        .min()
        .with_context(|| format!("{} has no data records", path.display()))?;
    let start_index = usize::try_from(start)?;
    let mut end_index = start_index;
    for (addr, data) in &records {
        end_index = end_index.max(usize::try_from(*addr)? + data.len());
    }
    let mut image = vec![0xFF; end_index - start_index];
    for (addr, data) in &records {
        let offset = usize::try_from(*addr)? - start_index;
        image[offset..offset + data.len()].copy_from_slice(data);
    }
    Ok((start, image))
}

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, entry) in (0u32..).zip(table.iter_mut()) {
        let mut c = i;
        for _ in 0..8 {
            c = if c & 1 == 1 {
                (c >> 1) ^ 0xEDB8_8320
            } else {
                c >> 1
            };
        }
        *entry = c;
    }
    !data.iter().fold(0xFFFF_FFFF_u32, |c, b| {
        table[usize::from(c.to_le_bytes()[0] ^ b)] ^ (c >> 8)
    })
}

fn parse_u32(s: &str) -> Result<u32, String> {
    let parsed = match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(hex) => u32::from_str_radix(&hex.replace('_', ""), 16),
        None => s.replace('_', "").parse(),
    };
    parsed.map_err(|e| e.to_string())
}

fn check_range(addr: u32, len: u32) -> Result<()> {
    if u64::from(addr) + u64::from(len) > FLASH_SIZE {
        bail!("range exceeds the 16 MiB flash (addr=0x{addr:X}, len=0x{len:X})");
    }
    Ok(())
}

fn xilinx_tool(vivado: &str, name: &str) -> String {
    let vivado = Path::new(vivado);
    let file = match vivado.extension().and_then(|e| e.to_str()) {
        Some(ext) => format!("{name}.{ext}"),
        None => name.to_string(),
    };
    vivado.with_file_name(file).to_string_lossy().into_owned()
}

fn build_dir(probe_dir: &Path) -> Result<PathBuf> {
    let dir = probe_dir.join("build");
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    Ok(dir)
}

fn bit_file(probe_dir: &Path) -> PathBuf {
    probe_dir.join("build").join("flash_probe.bit")
}

fn sim(probe_dir: &Path) -> Result<()> {
    let vivado = resolve_vivado()?;
    let work = build_dir(probe_dir)?.join("sim");
    std::fs::create_dir_all(&work).with_context(|| format!("creating {}", work.display()))?;

    let mut xvlog_args = vec!["-sv".to_string()];
    xvlog_args.extend(
        [
            "rtl/flash_probe_core.sv",
            "rtl/flash_probe_iprog.sv",
            "rtl/flash_probe_top.sv",
            "sim/sim_flash_model.sv",
            "sim/sim_flash_probe.sv",
        ]
        .iter()
        .map(|s| probe_dir.join(s).to_string_lossy().into_owned()),
    );
    run(&xilinx_tool(&vivado, "xvlog"), xvlog_args, &work).context("xvlog failed")?;
    run(
        &xilinx_tool(&vivado, "xelab"),
        ["-debug", "off", SIM_TOP, "-s", SIM_TOP],
        &work,
    )
    .context("xelab failed")?;

    let output = Command::new(xilinx_tool(&vivado, "xsim"))
        .args([SIM_TOP, "-R"])
        .current_dir(&work)
        .output()
        .context("failed to spawn xsim")?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    print!("{stdout}");
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    if !output.status.success() || !stdout.contains(&format!("OK! {SIM_TOP}")) {
        bail!("{SIM_TOP} failed (see the log above)");
    }
    Ok(())
}

fn build(probe_dir: &Path, spi_x1: bool, next_config_addr: Option<u32>) -> Result<()> {
    let vivado = resolve_vivado()?;
    let suffix = if spi_x1 {
        "_x1"
    } else if next_config_addr.is_some() {
        "_mb"
    } else {
        ""
    };
    let log = build_dir(probe_dir)?.join(format!("build{suffix}.log"));
    let mut args = vec![
        "-mode".to_string(),
        "batch".to_string(),
        "-nojournal".to_string(),
        "-log".to_string(),
        log.to_string_lossy().into_owned(),
        "-source".to_string(),
        "scripts/build.tcl".to_string(),
    ];
    if spi_x1 {
        args.push("-tclargs".to_string());
        args.push("x1".to_string());
    }
    if let Some(addr) = next_config_addr {
        args.push("-tclargs".to_string());
        args.push("multiboot".to_string());
        args.push(format!("0x{addr:08X}"));
    }
    run(&vivado, args, probe_dir)?;
    let bit = probe_dir
        .join("build")
        .join(format!("flash_probe{suffix}.bit"));
    if !bit.is_file() {
        bail!("Vivado finished but {} was not created", bit.display());
    }
    println!("flash-probe built: {}", bit.display());
    Ok(())
}

fn load(probe_dir: &Path) -> Result<()> {
    let bit = require_probe_bit(probe_dir)?;
    let vivado = resolve_vivado()?;
    let log = build_dir(probe_dir)?.join("jtag.log");
    run(
        &vivado,
        [
            "-mode",
            "batch",
            "-nojournal",
            "-log",
            &log.to_string_lossy(),
            "-source",
            "scripts/jtag.tcl",
            "-tclargs",
            "load",
            &bit.to_string_lossy(),
        ],
        probe_dir,
    )
    .context("Vivado failed. Make sure the JTAG cable is connected and the AUTD3 is powered on.")?;
    println!("flash-probe loaded (volatile). Power-cycle the device to restore the normal firmware.");
    Ok(())
}

fn exec(probe_dir: &Path, request: Request) -> Result<u64> {
    Ok(exec_many(probe_dir, &[request])?[0])
}

fn exec_many(probe_dir: &Path, requests: &[Request]) -> Result<Vec<u64>> {
    let mut tcl_args = vec!["cmd".to_string()];
    for request in requests {
        tcl_args.push(request.tdi());
        tcl_args.push(request.timeout_ms().to_string());
    }
    let stdout = vivado_jtag(probe_dir, &tcl_args)?;

    let tdos = stdout
        .lines()
        .filter_map(|l| l.strip_prefix("RESULT "))
        .collect::<Vec<_>>();
    if tdos.len() != requests.len() {
        print!("{stdout}");
        bail!(
            "Vivado reported {} result(s) for {} command(s)",
            tdos.len(),
            requests.len()
        );
    }
    requests
        .iter()
        .zip(tdos)
        .map(|(request, tdo)| {
            let tdo = u128::from_str_radix(tdo.trim(), 16)
                .with_context(|| format!("malformed TDO: {tdo}"))?;
            if (tdo >> 74) & 0x3F != SIGNATURE {
                bail!("flash-probe signature mismatch (TDO={tdo:020X})");
            }
            if (tdo >> 64) & 0xFF != u128::from(request.op as u8) {
                bail!("result belongs to another command (TDO={tdo:020X})");
            }
            Ok(u64::try_from(tdo & u128::from(u64::MAX))?)
        })
        .collect()
}
