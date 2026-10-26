use std::io::Write;

use anyhow::{Context, Result};
use clap::Parser;

use autd3_rs_firmware_emulator::udp::UdpEmulator;

#[derive(Parser)]
#[command(
    name = "autd3-udp-emulator",
    about,
    long_about = "Run emulated AUTD3 devices on the loopback (`[::1]`) over UDP.\n\n\
        Prints the address to open with `Interface::Addr` as the first line of stdout and \
        keeps the devices running until stdin is closed."
)]
struct Cli {
    #[arg(long, help = "Number of emulated devices")]
    devices: usize,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let emulator = UdpEmulator::spawn(cli.devices).context("failed to start the UDP emulator")?;
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "{}", emulator.addr())
        .and_then(|()| stdout.flush())
        .context("failed to print the address")?;
    drop(stdout);
    let _ = std::io::copy(&mut std::io::stdin(), &mut std::io::sink());
    drop(emulator);
    Ok(())
}
