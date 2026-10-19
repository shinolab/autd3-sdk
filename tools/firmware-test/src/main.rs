mod cases;
mod cli;
mod io;

use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;

use autd3_rs::commands::Command;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::rt::{LogWriter, TracingOption, init_tracing};
use autd3_rs::{Client, ClientConfig, Error, Frames, Interface, StateChecker, UdpOption};

use crate::cli::Cli;
use crate::io::{check, prompt, wait_enter};

const REOPEN_ATTEMPTS: u32 = 5;
const REOPEN_INTERVAL: Duration = Duration::from_secs(2);

pub struct Ctx<'a> {
    pub client: &'a Client,
    pub geometry: &'a Geometry,
    pub states: &'a StateChecker,
    pub heartbeat: Duration,
    pub lost_timeout: Duration,
    pub mute: bool,
}

pub struct Link<'a> {
    pub geometry: &'a Geometry,
    pub option: &'a UdpOption,
    pub config: ClientConfig,
    pub mute: bool,
}

impl Link<'_> {
    async fn open(&self) -> Result<Client, Error> {
        Client::open(self.geometry, self.option, self.config).await
    }

    pub async fn reopen(&self) -> Result<Client> {
        let mut attempt = 1;
        loop {
            match self.open().await {
                Ok(client) => {
                    restore_gpio_out(&client).await;
                    if self.mute && !mute(&client).await {
                        anyhow::bail!("muting the reopened devices failed");
                    }
                    return Ok(client);
                }
                Err(e) if attempt < REOPEN_ATTEMPTS => {
                    eprintln!(
                        "reopening attempt {attempt}/{REOPEN_ATTEMPTS} failed: {e}; retrying in {REOPEN_INTERVAL:?}"
                    );
                    attempt += 1;
                    tokio::time::sleep(REOPEN_INTERVAL).await;
                }
                Err(e) => {
                    return Err(e).context(format!(
                        "reopening the devices failed {REOPEN_ATTEMPTS} time(s)"
                    ));
                }
            }
        }
    }
}

enum Selected {
    Quit,
    Lost,
    Reboot,
    AutoRecovery,
    CpuWatchdog,
}

impl Ctx<'_> {
    pub async fn try_send_frames(&self, frames: &Frames) -> Result<(), Error> {
        for frame in frames {
            self.client.send_frame(frame).await?.await?.check()?;
        }
        Ok(())
    }

    pub async fn send_frames(&self, frames: &Frames) -> Result<()> {
        Ok(self.try_send_frames(frames).await?)
    }

    pub async fn send<'a, C: Command<'a>>(&self, cmd: C) -> Result<()> {
        self.send_frames(&Frames::encode(self.client.geometry(), cmd)?)
            .await
    }
}

const CASE_NAMES: &[&str] = &[
    "Pattern",
    "Modulation",
    "FociSTM",
    "PatternSTM",
    "Silencer",
    "ForceFan",
    "Pulse Width Encoder",
    "Phase Correction",
    "Transition",
    "GPIO",
    "Error",
    "Output Mask",
    "FPGA State",
    "CPU Config",
    "Synchronize",
    "PTP Unlock Failsafe",
    "Lost",
    "Reboot",
    "Auto Recovery",
    "CPU Watchdog",
];

const PULSE_WIDTH_ENCODER_INDEX: usize = 6;
const LOST_INDEX: usize = 16;
const REBOOT_INDEX: usize = 17;
const AUTO_RECOVERY_INDEX: usize = 18;
const CPU_WATCHDOG_INDEX: usize = 19;

const _: () = assert!(CASE_NAMES.len() == CPU_WATCHDOG_INDEX + 1);

async fn dispatch(index: usize, ctx: &Ctx<'_>) -> Result<()> {
    match index {
        0 => cases::pattern::run(ctx).await,
        1 => cases::modulation::run(ctx).await,
        2 => cases::foci_stm::run(ctx).await,
        3 => cases::pattern_stm::run(ctx).await,
        4 => cases::silencer::run(ctx).await,
        5 => cases::force_fan::run(ctx).await,
        6 => cases::pulse_width_encoder::run(ctx).await,
        7 => cases::phase_correction::run(ctx).await,
        8 => cases::transition::run(ctx).await,
        9 => cases::gpio::run(ctx).await,
        10 => cases::error::run(ctx).await,
        11 => cases::output_mask::run(ctx).await,
        12 => cases::fpga_state::run(ctx).await,
        13 => cases::cpu_config::run(ctx).await,
        14 => cases::synchronize::run(ctx).await,
        15 => cases::ptp_unlock_failsafe::run(ctx).await,
        _ => Ok(()),
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let _log_guard = init_tracing(TracingOption {
        default_filter: "warn",
        writer: LogWriter::Stderr,
    });

    let cli = Cli::parse();
    if let Err(msg) = cli.validate() {
        anyhow::bail!(msg);
    }

    check("Devices flashed with the latest firmware are connected");
    check("An oscilloscope is attached to the GPIO[0]/GPIO[1] pins of each device");
    check("No output is present on any GPIO pin");
    check("Client-side validation is disabled: malformed commands are checked by the firmware");
    if cli.mute {
        check(
            "--mute: the pulse width table is all zero, so no transducer emits and PwmOut stays low",
        );
    }

    run(&cli).await
}

async fn run(cli: &Cli) -> Result<()> {
    let geometry = Geometry::new((0..cli.devices).map(|_| Autd3::default()).collect());
    let heartbeat = Duration::from_micros(cli.heartbeat_us);

    let default = UdpOption::default();
    let option = UdpOption {
        iface: match cli.simulator {
            Some(port) => Interface::Simulator(port),
            None => cli.interface.clone().into(),
        },
        heartbeat: Some(heartbeat),
        lost_timeout: cli
            .lost_timeout_ms
            .map_or(default.lost_timeout, Duration::from_millis),
        ..default
    };
    let link = Link {
        geometry: &geometry,
        option: &option,
        config: cli.client_config(),
        mute: cli.mute,
    };
    let mut client = link.open().await.context("opening the devices")?;
    if cli.mute && !mute(&client).await {
        anyhow::bail!("muting the devices failed");
    }

    describe(&client).await;
    if restore_gpio_out(&client).await {
        wait_enter("The GPIO[0] output is synchronised across every device").await;
    }

    loop {
        let states = client.state_checker();
        let ctx = Ctx {
            client: &client,
            geometry: &geometry,
            states: &states,
            heartbeat,
            lost_timeout: option.lost_timeout,
            mute: cli.mute,
        };
        client = match select(&ctx).await {
            Selected::Quit => break,
            Selected::Lost => cases::lost::run(client, &link).await?,
            Selected::Reboot => cases::reboot::run(client, &link).await?,
            Selected::AutoRecovery => cases::auto_recovery::run(client, &link).await?,
            Selected::CpuWatchdog => cases::cpu_watchdog::run(client, &link).await?,
        };
    }

    client.close().await.context("closing client")?;
    println!("Ok!");
    Ok(())
}

async fn restore_gpio_out(client: &Client) -> bool {
    use autd3_rs::commands::{GpioOut, SetGpioOut};
    let sent = client
        .send(SetGpioOut {
            outputs: [
                GpioOut::BaseSignal,
                GpioOut::Off,
                GpioOut::Off,
                GpioOut::Off,
            ],
        })
        .await;
    if let Err(e) = &sent {
        eprintln!("set gpio out failed: {e}");
    }
    sent.is_ok()
}

pub async fn mute(client: &Client) -> bool {
    use autd3_rs::commands::SetPulseWidthTable;
    let sent = client
        .send(SetPulseWidthTable {
            table: &SetPulseWidthTable::empty_table(),
        })
        .await;
    if let Err(e) = &sent {
        eprintln!("muting failed: {e}");
    }
    sent.is_ok()
}

pub async fn describe(client: &Client) {
    match client.read_firmware_version().await {
        Ok(fw) => {
            check(&format!("Connected devices: {}", client.num_devices()));
            for (i, v) in fw.iter().enumerate() {
                println!("  device[{i}] firmware: {v}");
            }
            if fw.len() != client.num_devices() {
                eprintln!(
                    "warning: firmware version count {} != device count {}",
                    fw.len(),
                    client.num_devices()
                );
            }
        }
        Err(e) => eprintln!("read firmware version failed: {e}"),
    }

    match client.read_fpga_state().await {
        Ok(states) => {
            for (i, s) in states.iter().enumerate() {
                println!(
                    "  device[{i}] fpga state: raw={:#04x} thermal={} mod_bank={:?} pattern_bank={:?} mode={} pattern_stopped={} mod_stopped={} transition_pending={} failsafe={}",
                    s.raw(),
                    s.is_thermal_asserted(),
                    s.current_mod_bank(),
                    s.current_pattern_bank(),
                    if s.is_pattern_mode() {
                        "pattern"
                    } else {
                        "stm"
                    },
                    s.is_pattern_stopped(),
                    s.is_mod_stopped(),
                    s.is_transition_pending(),
                    s.is_failsafe_active(),
                );
            }
        }
        Err(e) => eprintln!("read fpga state failed: {e}"),
    }
}

async fn select(ctx: &Ctx<'_>) -> Selected {
    loop {
        println!();
        for (i, name) in CASE_NAMES.iter().enumerate() {
            println!("[{i}]: {name}");
        }
        println!("[anything else]: quit");
        let sel = prompt("Select a number").await;
        let Ok(i) = sel.trim().parse::<usize>() else {
            return Selected::Quit;
        };
        if i >= CASE_NAMES.len() {
            return Selected::Quit;
        }

        println!("=== {} ===", CASE_NAMES[i]);
        match i {
            LOST_INDEX => return Selected::Lost,
            REBOOT_INDEX => return Selected::Reboot,
            AUTO_RECOVERY_INDEX => return Selected::AutoRecovery,
            CPU_WATCHDOG_INDEX => return Selected::CpuWatchdog,
            PULSE_WIDTH_ENCODER_INDEX if ctx.mute => {
                println!("  [SKIP] the case replaces the pulse width table that --mute zeroes");
                continue;
            }
            _ => {}
        }
        if let Err(e) = dispatch(i, ctx).await {
            eprintln!("test failed: {e:?}");
        }
        if ctx.client.config().exec_lead.is_some() {
            report_exec_late(ctx.client).await;
        }

        if let Err(e) = reset(ctx).await {
            eprintln!("reset failed: {e:?}");
        }
        if ctx.mute && !mute(ctx.client).await {
            eprintln!("stopping: the devices could not be muted after the case");
            return Selected::Quit;
        }
    }
}

async fn report_exec_late(client: &Client) {
    match client.read_telemetry().await {
        Ok(telemetry) => {
            let late: Vec<u32> = telemetry.iter().map(|t| t.exec_late).collect();
            println!(
                "  {} no frame since the last Clear started 1 us or more after its EXEC_AT: exec_late {late:?}",
                if late.iter().all(|n| *n == 0) {
                    "[OK]"
                } else {
                    "[FAIL]"
                }
            );
        }
        Err(e) => eprintln!("read telemetry failed: {e}"),
    }
}

async fn reset(ctx: &Ctx<'_>) -> Result<()> {
    use autd3_rs::commands::{Clear, Pattern, SetSilencer};
    use autd3_rs::value::Intensity;
    use autd3_rs_pattern::set_intensity;

    let phases = ctx.geometry.phase_buffer();
    let mut intensities = ctx.geometry.intensity_buffer();
    set_intensity(Intensity::MIN, &mut intensities);

    ctx.send_frames(&Frames::encode(
        ctx.client.geometry(),
        (Pattern::new(&phases, &intensities), SetSilencer::default()),
    )?)
    .await?;

    ctx.send(Clear).await
}
