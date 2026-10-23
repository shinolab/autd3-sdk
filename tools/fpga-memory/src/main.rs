use anyhow::{Context, Result, anyhow, bail, ensure};
use clap::{Args, Parser, Subcommand};

use autd3_cpu_wire::config::{CpuConfig, FpgaBusWait};
use autd3_cpu_wire::payload::{ReadFpgaMemoryPayload, SetCpuConfigPayload, WriteFpgaMemoryPayload};
use autd3_cpu_wire::{Cmd, PAYLOAD_BYTES, REPLY_DATA_BYTES_MAX};
use autd3_rs_firmware_ota::{Driver, Exchange, Frame, UdpArgs};

const WORDS_PER_LINE: usize = 8;
const REPORTED_ERRORS_PER_DEVICE: usize = 8;
const TEST_PATTERN_SEED: u32 = 0x2545_F491;

#[derive(Parser, Debug)]
#[command(
    name = "autd3-fpga-memory",
    about,
    long_about = "Read and write raw AUTD3 FPGA bus addresses over UDP (diagnostics only).\n\n\
        An address is the 16-bit bus address `BRAM select << 8 | address` (0x00__ controller, \
        0x01__ phase correction, 0x02__ output mask, 0x03__ flash registers, 0x04__-0x05__ flash page buffer, \
        0x06__ pulse width table, 0x07__ CPU watchdog, 0x08__ synchronizer phase shift, \
        0x4000-0x7FFF modulation, 0xC000-0xFFFF emission; the last two go to the bank and \
        page selected by the controller registers). Numbers are decimal or `0x` hexadecimal.\n\n\
        Every device on the chain receives the same request. Nothing is validated: a write can \
        open the failsafe gate, latch half-written settings or start a flash command. Only the \
        controller (0x00__) and flash (0x03__) registers, the CPU watchdog state (0x0703) and the synchronizer \
        phase shift control (0x0800) can be read back; the other addresses of 0x07__ and 0x08__ read as 0, and a read \
        of any other address returns the output of whichever of those was selected last, not the memory.\n\n\
        --bus-wait changes the wait cycles of the CPU bus to the FPGA for this run only: the \
        devices return to the default (3 cycles) when the chain is enumerated again."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Args, Debug)]
struct Chain {
    #[arg(long, help = "Number of devices on the chain")]
    devices: usize,
    #[arg(
        long,
        value_parser = parse_bus_wait,
        help = "Wait cycles of the CPU bus to the FPGA during this run: 2 or 3 (default: leave the device default, 3)"
    )]
    bus_wait: Option<FpgaBusWait>,
    #[command(flatten)]
    udp: UdpArgs,
}

#[derive(Subcommand, Debug)]
enum Command {
    #[command(about = "Read words starting at a bus address")]
    Read {
        #[arg(value_parser = parse_u16, help = "First bus address")]
        addr: u16,
        #[arg(long, default_value_t = 1, value_parser = parse_count, help = "Number of words to read")]
        count: usize,
        #[command(flatten)]
        chain: Chain,
    },
    #[command(about = "Write words to consecutive bus addresses")]
    Write {
        #[arg(value_parser = parse_u16, help = "First bus address")]
        addr: u16,
        #[arg(required = true, value_parser = parse_u16, help = "Words to write, one per address")]
        words: Vec<u16>,
        #[arg(
            long,
            help = "Read the words back and fail on a difference (controller and flash registers only)"
        )]
        verify: bool,
        #[command(flatten)]
        chain: Chain,
    },
    #[command(
        about = "Write pseudo-random words, read them back and count the differences",
        long_about = "Write pseudo-random words to a range of the controller registers, read them back and count the \
            differences per device. A word that differs is read once more: if the second read \
            matches, the first read was wrong, otherwise the write was. The range is read before \
            --bus-wait is applied and written back with the default bus wait at the end.\n\n\
            The default range is the 16 debug value registers (0x00F0-0x00FF), which take effect \
            only when latched, so the test does not change the output."
    )]
    Test {
        #[arg(default_value = "0x00F0", value_parser = parse_u16, help = "First bus address (controller registers, 0x0000-0x00FF)")]
        addr: u16,
        #[arg(long, default_value_t = 16, value_parser = parse_count, help = "Number of words in the range")]
        count: usize,
        #[arg(
            long,
            default_value_t = 1000,
            help = "Number of write and read-back rounds"
        )]
        iterations: u32,
        #[command(flatten)]
        chain: Chain,
    },
}

fn parse_number(text: &str) -> Result<u32, String> {
    let parsed = match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Some(hex) => u32::from_str_radix(hex, 16),
        None => text.parse(),
    };
    parsed.map_err(|e| format!("{e} (expected a decimal or 0x hexadecimal number)"))
}

fn parse_u16(text: &str) -> Result<u16, String> {
    u16::try_from(parse_number(text)?).map_err(|_| "larger than 0xffff".to_owned())
}

fn parse_bus_wait(text: &str) -> Result<FpgaBusWait, String> {
    text.parse()
        .ok()
        .and_then(FpgaBusWait::from_u8)
        .ok_or_else(|| "expected 2 or 3".to_owned())
}

fn parse_count(text: &str) -> Result<usize, String> {
    usize::try_from(parse_number(text)?).map_err(|e| e.to_string())
}

fn ensure_inside_the_bus(addr: u16, words: usize) -> Result<()> {
    ensure!(words > 0, "nothing to transfer: the word count is 0");
    ensure!(
        usize::from(addr) + words <= 1 << u16::BITS,
        "{words} words from {addr:#06x} run past the last bus address 0xffff"
    );
    Ok(())
}

fn read<L: Exchange>(driver: &mut Driver<L>, addr: u16, count: usize) -> Result<Vec<Vec<u16>>> {
    let mut words = vec![Vec::with_capacity(count); driver.num_devices()];
    let per_frame = REPLY_DATA_BYTES_MAX / size_of::<u16>();
    for (addr, start) in (addr..=u16::MAX)
        .step_by(per_frame)
        .zip((0..count).step_by(per_frame))
    {
        let len = (count - start).min(per_frame);
        let payload = ReadFpgaMemoryPayload::new(addr, u16::try_from(len)?)
            .context("the read request does not fit the bus")?;
        let replies =
            driver.send_checked(&Frame::with_payload(Cmd::ReadFpgaMemory, &payload, &[]))?;
        for (device, reply) in replies.iter().enumerate() {
            ensure!(
                reply.value.len() == len * size_of::<u16>(),
                "device {device} returned {} bytes for {len} words",
                reply.value.len()
            );
            words[device].extend(
                reply
                    .value
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|word| u16::from_le_bytes(*word)),
            );
        }
    }
    Ok(words)
}

fn write<L: Exchange>(driver: &mut Driver<L>, addr: u16, words: &[u16]) -> Result<()> {
    let per_frame = (PAYLOAD_BYTES - size_of::<WriteFpgaMemoryPayload>()) / size_of::<u16>();
    for (addr, chunk) in (addr..=u16::MAX)
        .step_by(per_frame)
        .zip(words.chunks(per_frame))
    {
        let payload = WriteFpgaMemoryPayload::new(addr, chunk.len())
            .context("the write request does not fit the bus")?;
        let data: Vec<u8> = chunk.iter().flat_map(|word| word.to_le_bytes()).collect();
        driver.send_checked(&Frame::with_payload(Cmd::WriteFpgaMemory, &payload, &data))?;
    }
    Ok(())
}

fn set_bus_wait<L: Exchange>(driver: &mut Driver<L>, wait: FpgaBusWait) -> Result<()> {
    let config = CpuConfig {
        fpga_bus_wait: wait,
        ..CpuConfig::default()
    };
    let payload = SetCpuConfigPayload::encode(&config).map_err(|e| anyhow!("{e:?}"))?;
    driver.send_checked(&Frame::with_payload(Cmd::SetCpuConfig, &payload, &[]))?;
    println!("FPGA bus wait: {} cycles", wait.as_u8());
    Ok(())
}

fn next_pattern(state: &mut u32) -> u16 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    let [_, low, high, _] = state.to_le_bytes();
    u16::from_le_bytes([low, high])
}

#[derive(Default)]
struct TestErrors {
    write: u64,
    read: u64,
    reported: Vec<String>,
}

fn test<L: Exchange>(
    driver: &mut Driver<L>,
    addr: u16,
    count: usize,
    iterations: u32,
) -> Result<Vec<TestErrors>> {
    let mut errors: Vec<TestErrors> = (0..driver.num_devices())
        .map(|_| TestErrors::default())
        .collect();
    let mut state = TEST_PATTERN_SEED;
    for iteration in 0..iterations {
        let pattern: Vec<u16> = (0..count).map(|_| next_pattern(&mut state)).collect();
        write(driver, addr, &pattern)?;
        let first = read(driver, addr, count)?;
        if first.iter().all(|words| *words == pattern) {
            continue;
        }
        let second = read(driver, addr, count)?;
        for (device, (first, second)) in first.iter().zip(&second).enumerate() {
            for (i, wrote) in pattern.iter().enumerate() {
                if first[i] == *wrote {
                    continue;
                }
                let kind = if second[i] == *wrote {
                    errors[device].read += 1;
                    "read"
                } else {
                    errors[device].write += 1;
                    "write"
                };
                if errors[device].reported.len() < REPORTED_ERRORS_PER_DEVICE {
                    errors[device].reported.push(format!(
                        "round {iteration} {:#06x}: wrote {wrote:#06x}, read {:#06x} then {:#06x} ({kind} error)",
                        usize::from(addr) + i,
                        first[i],
                        second[i]
                    ));
                }
            }
        }
    }
    Ok(errors)
}

fn print_words(addr: u16, devices: &[Vec<u16>]) {
    for (device, words) in devices.iter().enumerate() {
        println!("device {device}");
        for (line, chunk) in words.chunks(WORDS_PER_LINE).enumerate() {
            let values: Vec<String> = chunk.iter().map(|word| format!("{word:#06x}")).collect();
            println!(
                "  {:#06x}: {}",
                usize::from(addr) + line * WORDS_PER_LINE,
                values.join(" ")
            );
        }
    }
}

fn first_difference(addr: u16, expected: &[u16], devices: &[Vec<u16>]) -> Option<String> {
    devices.iter().enumerate().find_map(|(device, read)| {
        expected
            .iter()
            .zip(read)
            .position(|(wrote, read)| wrote != read)
            .map(|i| {
                format!(
                    "device {device} reads {:#06x} at {:#06x} after {:#06x} was written",
                    read[i],
                    usize::from(addr) + i,
                    expected[i]
                )
            })
    })
}

fn open(chain: &Chain) -> Result<Driver<impl Exchange>> {
    Ok(Driver::new(chain.udp.open(chain.devices)?))
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Read { addr, count, chain } => {
            ensure_inside_the_bus(addr, count)?;
            let mut driver = open(&chain)?;
            if let Some(wait) = chain.bus_wait {
                set_bus_wait(&mut driver, wait)?;
            }
            let words = read(&mut driver, addr, count)?;
            print_words(addr, &words);
            driver.close()?;
        }
        Command::Write {
            addr,
            words,
            verify,
            chain,
        } => {
            ensure_inside_the_bus(addr, words.len())?;
            let mut driver = open(&chain)?;
            if let Some(wait) = chain.bus_wait {
                set_bus_wait(&mut driver, wait)?;
            }
            write(&mut driver, addr, &words)?;
            println!(
                "wrote {} words from {addr:#06x} to {} devices",
                words.len(),
                driver.num_devices()
            );
            if verify {
                let read = read(&mut driver, addr, words.len())?;
                if let Some(difference) = first_difference(addr, &words, &read) {
                    bail!("{difference}");
                }
                println!("read back the same words from every device");
            }
            driver.close()?;
        }
        Command::Test {
            addr,
            count,
            iterations,
            chain,
        } => {
            ensure_inside_the_bus(addr, count)?;
            ensure!(
                usize::from(addr) + count <= 1 << 8,
                "the test needs words that read back: only the controller registers (0x0000-0x00ff) do"
            );
            let mut driver = open(&chain)?;
            let original = read(&mut driver, addr, count)?;
            ensure!(
                original.iter().all(|words| *words == original[0]),
                "the devices hold different words in the range, which one request to every device cannot restore"
            );
            if let Some(wait) = chain.bus_wait {
                set_bus_wait(&mut driver, wait)?;
            }
            let outcome = test(&mut driver, addr, count, iterations);
            if chain.bus_wait.is_some() {
                set_bus_wait(&mut driver, FpgaBusWait::default())?;
            }
            write(&mut driver, addr, &original[0])?;
            let errors = outcome?;
            let words = u64::from(iterations) * count as u64;
            for (device, errors) in errors.iter().enumerate() {
                println!(
                    "device {device}: {words} words, {} write errors, {} read errors",
                    errors.write, errors.read
                );
                for line in &errors.reported {
                    println!("  {line}");
                }
            }
            driver.close()?;
            if errors.iter().any(|e| e.write + e.read > 0) {
                bail!("the words read back differ from the words written");
            }
        }
    }
    Ok(())
}
