mod cases;
mod cli;
mod io;

use std::future::Future;
use std::net::{Ipv6Addr, SocketAddrV6};
use std::ops::Deref;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;

use autd3_rs::commands::{
    Clear, Command, FixedCompletionTime, GpioOut, Pattern, SetGpioOut, SetSilencer, Synchronize,
};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::value::Intensity;
use autd3_rs::{Client, DeviceState, Error, ExecAt, Frames, Interface, TransportOption};

use crate::cli::Cli;
use crate::io::{check, prompt, wait_enter};
use tracing_subscriber::EnvFilter;

pub fn device_states(client: &Client) -> Vec<DeviceState> {
    client
        .device_status()
        .map(|status| status.iter().map(|s| s.state).collect())
        .unwrap_or_default()
}

pub fn failsafe_gates(client: &Client) -> Result<Vec<bool>> {
    Ok(client
        .device_status()?
        .iter()
        .map(|s| s.failsafe_active)
        .collect())
}

pub struct Ctx<'a> {
    pub client: &'a Client,
    pub geometry: &'a Geometry,
    pub heartbeat: Duration,
    pub exec: ExecAt,
}

pub struct Session {
    client: Arc<Client>,
    keepalive: tokio::task::JoinHandle<()>,
}

impl Session {
    pub fn new(client: Client, heartbeat: Duration) -> Self {
        let client = Arc::new(client);
        let keepalive = tokio::spawn({
            let client = Arc::clone(&client);
            async move {
                let mut ticker = tokio::time::interval(heartbeat);
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                loop {
                    ticker.tick().await;
                    if client.send_heartbeat().is_err() {
                        break;
                    }
                }
            }
        });
        Self { client, keepalive }
    }
}

impl Deref for Session {
    type Target = Client;

    fn deref(&self) -> &Client {
        &self.client
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.keepalive.abort();
    }
}

type Case = for<'a> fn(&'a Ctx<'a>) -> Pin<Box<dyn Future<Output = Result<()>> + 'a>>;

impl Ctx<'_> {
    pub async fn try_send_frames(&self, frames: &Frames) -> Result<(), Error> {
        for frame in frames {
            self.client
                .send_frame_at(frame, self.exec)
                .await?
                .await?
                .check()?;
        }
        Ok(())
    }

    pub async fn send_frames(&self, frames: &Frames) -> Result<()> {
        Ok(self.try_send_frames(frames).await?)
    }

    pub async fn send<C: Command>(&self, cmd: C) -> Result<()> {
        self.send_frames(&Frames::encode(self.client.geometry(), cmd)?)
            .await
    }

    pub async fn stop_output(&self) -> Result<()> {
        self.send(SetSilencer::new(FixedCompletionTime {
            strict_mode: false,
            ..Default::default()
        }))
        .await?;
        let phases = self.client.geometry().phase_buffer();
        self.send(Pattern::new(&phases, Intensity::MIN)).await
    }
}

const CASES: &[(&str, Case)] = &[
    ("Pattern", |ctx| Box::pin(cases::pattern::run(ctx))),
    ("Modulation", |ctx| Box::pin(cases::modulation::run(ctx))),
    ("FociSTM", |ctx| Box::pin(cases::foci_stm::run(ctx))),
    ("PatternSTM", |ctx| Box::pin(cases::pattern_stm::run(ctx))),
    ("Silencer", |ctx| Box::pin(cases::silencer::run(ctx))),
    ("ForceFan", |ctx| Box::pin(cases::force_fan::run(ctx))),
    ("Pulse Width Encoder", |ctx| {
        Box::pin(cases::pulse_width_encoder::run(ctx))
    }),
    ("Phase Correction", |ctx| {
        Box::pin(cases::phase_correction::run(ctx))
    }),
    ("Transition", |ctx| Box::pin(cases::transition::run(ctx))),
    ("GPIO", |ctx| Box::pin(cases::gpio::run(ctx))),
    ("Error", |ctx| Box::pin(cases::error::run(ctx))),
    ("Output Mask", |ctx| Box::pin(cases::output_mask::run(ctx))),
    ("FPGA State", |ctx| Box::pin(cases::fpga_state::run(ctx))),
    ("CPU Config", |ctx| Box::pin(cases::cpu_config::run(ctx))),
    ("Synchronize", |ctx| Box::pin(cases::synchronize::run(ctx))),
    ("PTP Unlock Failsafe", |ctx| {
        Box::pin(cases::ptp_unlock_failsafe::run(ctx))
    }),
];

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    async fn reset(ctx: &Ctx<'_>) -> Result<()> {
        let phases = ctx.geometry.phase_buffer();

        ctx.send(Pattern::new(&phases, Intensity::MIN)).await?;
        ctx.send(SetSilencer::default()).await?;
        ctx.send(Clear).await
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    if let Err(msg) = cli.validate() {
        anyhow::bail!(msg);
    }

    check("Devices flashed with the latest firmware are connected");
    check("An oscilloscope is attached to the GPIO[0]/GPIO[1] pins of each device");
    check("No output is present on any GPIO pin");
    check("Client-side validation is disabled: malformed commands are checked by the firmware");

    let geometry = Geometry::new((0..cli.devices.get()).map(|_| Autd3::default()));
    let heartbeat = Duration::from_micros(cli.heartbeat_us);

    let default = TransportOption::default();
    let option = TransportOption {
        iface: match cli.simulator {
            Some(port) => Interface::Addr(SocketAddrV6::new(Ipv6Addr::LOCALHOST, port, 0, 0)),
            None => cli
                .interface
                .clone()
                .map_or(Interface::Auto, Interface::Name),
        },
        lost_timeout: cli
            .lost_timeout_ms
            .map_or(default.lost_timeout, Duration::from_millis),
        ack_timeout: cli.ack_timeout(),
        ..default
    };
    let client = Client::open(&geometry, &option).context("opening the devices")?;
    client.send(Clear).await?;
    client.send(Synchronize).await?;
    let client = Session::new(client, heartbeat);

    describe(&client).await;
    match client
        .send(SetGpioOut {
            outputs: [
                GpioOut::BaseSignal,
                GpioOut::Off,
                GpioOut::Off,
                GpioOut::Off,
            ],
        })
        .await
    {
        Ok(()) => wait_enter("The GPIO[0] output is synchronised across every device").await,
        Err(e) => eprintln!("set gpio out failed: {e}"),
    }

    let ctx = Ctx {
        client: &client,
        geometry: &geometry,
        heartbeat,
        exec: cli.exec(),
    };
    loop {
        println!();
        for (i, (name, _)) in CASES.iter().enumerate() {
            println!("[{i}]: {name}");
        }
        println!("[anything else]: quit");
        let sel = prompt("Select a number").await;
        let Some((name, run)) = sel.trim().parse::<usize>().ok().and_then(|i| CASES.get(i)) else {
            break;
        };

        println!("=== {name} ===");
        if let Err(e) = run(&ctx).await {
            eprintln!("test failed: {e:?}");
        }
        if ctx.exec != ExecAt::Immediate {
            match ctx.client.read_telemetry().await {
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

        if let Err(e) = reset(&ctx).await {
            eprintln!("reset failed: {e:?}");
        }
    }

    client.close().context("closing client")?;
    println!("Ok!");
    Ok(())
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
            let gates = failsafe_gates(client).unwrap_or_default();
            for (i, s) in states.iter().enumerate() {
                println!(
                    "  device[{i}] fpga state: raw={:#04x} thermal={} mod_bank={:?} pattern_bank={:?} mode={} pattern_stopped={} mod_stopped={} transition_pending={} failsafe={:?}",
                    s.0,
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
                    gates.get(i),
                );
            }
        }
        Err(e) => eprintln!("read fpga state failed: {e}"),
    }
}
