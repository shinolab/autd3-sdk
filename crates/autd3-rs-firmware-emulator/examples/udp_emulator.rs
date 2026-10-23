use std::io::Write;
use std::process::ExitCode;

use autd3_rs_firmware_emulator::udp::UdpEmulator;

fn parse_devices(mut args: impl Iterator<Item = String>) -> Result<usize, String> {
    let mut devices = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--devices" => {
                let value = args.next().ok_or("--devices needs a value")?;
                devices = Some(
                    value
                        .parse::<usize>()
                        .map_err(|e| format!("--devices {value}: {e}"))?,
                );
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    devices.ok_or_else(|| "usage: udp_emulator --devices <N>".to_string())
}

fn main() -> ExitCode {
    let devices = match parse_devices(std::env::args().skip(1)) {
        Ok(devices) => devices,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let emulator = match UdpEmulator::spawn(devices) {
        Ok(emulator) => emulator,
        Err(e) => {
            eprintln!("failed to start the UDP emulator: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut stdout = std::io::stdout().lock();
    if writeln!(stdout, "{}", emulator.addr())
        .and_then(|()| stdout.flush())
        .is_err()
    {
        return ExitCode::FAILURE;
    }
    drop(stdout);
    let _ = std::io::copy(&mut std::io::stdin(), &mut std::io::sink());
    drop(emulator);
    ExitCode::SUCCESS
}
