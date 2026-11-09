mod cases;
mod cli;
mod io;

use std::future::Future;
use std::net::{Ipv6Addr, SocketAddrV6};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;

use autd3_rs::commands::{
    Clear, Command, FixedCompletionTime, GpioOut, Nop, Pattern, SetGpioOut, SetSilencer,
    Synchronize,
};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::value::Intensity;
use autd3_rs::{
    Controller, DeviceState, DeviceStatus, Error, ExecAt, FirmwareVersion, FpgaState, Frames,
    Interface, Response, Telemetry, TransportOption,
};

use crate::cli::Cli;
use crate::io::{check, prompt, wait_enter};
use tracing_subscriber::EnvFilter;

pub struct Ctx<'a> {
    pub client: &'a Controller,
    pub geometry: &'a Geometry,
    pub heartbeat: Duration,
    pub ack_timeout: Duration,
}

type Case = for<'a> fn(&'a Ctx<'a>) -> Pin<Box<dyn Future<Output = Result<()>> + 'a>>;

impl Ctx<'_> {
    async fn within<T>(&self, replied: impl Future<Output = Result<T, Error>>) -> Result<T> {
        tokio::time::timeout(self.ack_timeout, replied)
            .await
            .with_context(|| format!("not every device replied within {:?}", self.ack_timeout))?
            .map_err(Into::into)
    }

    pub(crate) async fn send_frames(&self, frames: &Frames) -> Result<()> {
        for frame in frames {
            self.client.discard_unread_replies()?;
            self.within(async {
                let msg_id = self.client.send_frame(frame, ExecAt::Immediate).await?;
                self.client.collect(msg_id).await?.check()
            })
            .await?;
        }
        Ok(())
    }

    async fn heartbeat_response(&self) -> Result<Response> {
        self.client.discard_unread_replies()?;
        self.within(async {
            let msg_id = self.client.send_heartbeat().await?;
            self.client.collect(msg_id).await
        })
        .await
    }

    pub(crate) async fn device_status(&self) -> Result<Vec<DeviceStatus>> {
        Ok(self.heartbeat_response().await?.device_status())
    }

    pub(crate) async fn read_firmware_version(&self) -> Result<Vec<FirmwareVersion>> {
        self.within(self.client.read_firmware_version()).await
    }

    pub(crate) async fn read_fpga_state(&self) -> Result<Vec<FpgaState>> {
        let frames = Frames::encode(self.client.geometry(), Nop)?;
        let frame = frames.frame(0).context("Nop encodes to one frame")?;
        self.client.discard_unread_replies()?;
        let response = self
            .within(async {
                let msg_id = self.client.send_frame(frame, ExecAt::Immediate).await?;
                self.client.collect(msg_id).await
            })
            .await?;
        Ok(response.replies().iter().map(|r| r.fpga_state).collect())
    }

    pub(crate) async fn read_telemetry(&self) -> Result<Vec<Telemetry>> {
        Ok(self
            .heartbeat_response()
            .await?
            .replies()
            .iter()
            .map(|r| r.telemetry)
            .collect())
    }

    pub(crate) async fn device_states(&self) -> Result<Vec<DeviceState>> {
        Ok(self
            .device_status()
            .await?
            .iter()
            .map(|s| s.state)
            .collect())
    }

    pub(crate) async fn failsafe_gates(&self) -> Result<Vec<bool>> {
        Ok(self
            .device_status()
            .await?
            .iter()
            .map(|s| s.failsafe_active)
            .collect())
    }

    pub(crate) async fn send<C: Command>(&self, cmd: C) -> Result<()> {
        self.send_frames(&Frames::encode(self.client.geometry(), cmd)?)
            .await
    }

    pub(crate) async fn stop_output(&self) -> Result<()> {
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

#[allow(clippy::too_many_lines)]
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

    let option = TransportOption {
        iface: match cli.simulator {
            Some(port) => Interface::Addr(SocketAddrV6::new(Ipv6Addr::LOCALHOST, port, 0, 0)),
            None => cli
                .interface
                .clone()
                .map_or(Interface::Auto, Interface::Name),
        },
        ..TransportOption::default()
    };
    let client = Arc::new(
        Controller::open(&geometry, &option)
            .await
            .context("opening the devices")?,
    );
    let heartbeat = tokio::spawn({
        let client = Arc::clone(&client);
        let heartbeat_interval = cli.heartbeat;
        let mut interval = tokio::time::interval(heartbeat_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        async move {
            loop {
                interval.tick().await;
                match tokio::time::timeout(heartbeat_interval, client.send_heartbeat()).await {
                    Ok(Ok(_)) => {}
                    Ok(Err(e)) => eprintln!("sending a heartbeat failed: {e}"),
                    Err(_) => eprintln!(
                        "sending a heartbeat did not finish within {heartbeat_interval:?}"
                    ),
                }
            }
        }
    });
    let ctx = Ctx {
        client: &client,
        geometry: &geometry,
        heartbeat: cli.heartbeat,
        ack_timeout: cli.ack_timeout,
    };
    ctx.send(Clear).await?;
    ctx.send(Synchronize).await?;

    describe(&ctx).await;
    match ctx
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
        if let Err(e) = reset(&ctx).await {
            eprintln!("reset failed: {e:?}");
        }
    }

    heartbeat.abort();
    println!("Ok!");
    Ok(())
}

pub(crate) async fn describe(ctx: &Ctx<'_>) {
    let client = ctx.client;
    match ctx.read_firmware_version().await {
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

    match ctx.read_fpga_state().await {
        Ok(states) => {
            let gates = ctx.failsafe_gates().await.unwrap_or_default();
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
