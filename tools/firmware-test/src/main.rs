mod cases;
mod io;

use std::net::SocketAddr;

use anyhow::{Context, Result};

use autd3_rs::commands::Command;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::link::Interface;
use autd3_rs::{Client, ClientConfig, DatagramBuilder};
use autd3_rs_link_ethercrab::EtherCrabLinkOption;
use autd3_rs_link_remote::RemoteLinkOption;
use autd3_rs_link_soem::SoemLinkOption;
use autd3_rs_link_twincat::TwinCATLinkOption;

use crate::io::{check, prompt, wait_enter};

pub struct Ctx<'a> {
    pub client: &'a Client,
    pub geometry: &'a Geometry,
}

impl Ctx<'_> {
    pub async fn send<'a, C: Command<'a>>(&self, cmd: C) -> Result<()> {
        let mut builder: DatagramBuilder<'a> = self.client.datagram_builder();
        builder.push(cmd);
        let frames = builder.build()?;
        for frame in &frames {
            self.client.send_checked(frame).await?;
        }
        Ok(())
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
];

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
        _ => Ok(()),
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    check("Devices flashed with the latest firmware are connected");
    check("An oscilloscope is attached to the GPIO[0]/GPIO[1] pins of each device");
    check("No output is present on any GPIO pin");

    let num_devices = prompt("Number of devices (default 1)")
        .await
        .trim()
        .parse::<usize>()
        .unwrap_or(1)
        .max(1);

    let backend = select_backend().await;
    run(backend, num_devices).await
}

enum Backend {
    Soem { iface: Interface },
    EtherCrab { iface: Interface },
    Twincat,
    Remote { addr: SocketAddr },
}

async fn select_backend() -> Backend {
    println!("[0]: SOEM");
    println!("[1]: EtherCrab");
    println!("[2]: TwinCAT (local)");
    println!("[3]: Remote");
    let sel = prompt("Select a link (default SOEM)")
        .await
        .trim()
        .parse::<usize>()
        .unwrap_or(0);
    match sel {
        1 => Backend::EtherCrab {
            iface: iface_prompt().await,
        },
        2 => Backend::Twincat,
        3 => {
            let raw = prompt("Server address (default 127.0.0.1:8080)").await;
            let addr = raw
                .trim()
                .parse::<SocketAddr>()
                .unwrap_or_else(|_| "127.0.0.1:8080".parse().expect("valid default addr"));
            Backend::Remote { addr }
        }
        _ => Backend::Soem {
            iface: iface_prompt().await,
        },
    }
}

async fn iface_prompt() -> Interface {
    let raw = prompt("Network interface name (blank for auto-select)").await;
    let name = raw.trim();
    if name.is_empty() {
        Interface::Auto
    } else {
        Interface::from(name)
    }
}

async fn run(backend: Backend, num_devices: usize) -> Result<()> {
    let geometry = Geometry::new((0..num_devices).map(|_| Autd3::default()).collect());
    let config = ClientConfig::default();

    let client = match backend {
        Backend::Soem { iface } => {
            let option = SoemLinkOption {
                iface,
                ..Default::default()
            };
            Client::open(&geometry, option, config).await
        }
        Backend::EtherCrab { iface } => {
            let option = EtherCrabLinkOption {
                iface,
                ..Default::default()
            };
            Client::open(&geometry, option, config).await
        }
        Backend::Twincat => Client::open(&geometry, TwinCATLinkOption::local(), config).await,
        Backend::Remote { addr } => {
            Client::open(&geometry, RemoteLinkOption::new(addr), config).await
        }
    }
    .context("opening link / client handshake")?;

    run_session(&client, &geometry).await;

    client.close().await.context("closing client")?;
    println!("Ok!");
    Ok(())
}

async fn run_session(client: &Client, geometry: &Geometry) {
    let ctx = Ctx { client, geometry };

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

    {
        use autd3_rs::commands::{GpioOut, SetGpioOut};
        if let Err(e) = ctx
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
            eprintln!("set gpio out failed: {e}");
        } else {
            wait_enter("The GPIO[0] output is synchronised across every device").await;
        }
    }

    match client.read_fpga_state().await {
        Ok(states) => {
            for (i, s) in states.iter().enumerate() {
                println!(
                    "  device[{i}] fpga state: raw={:#04x} thermal={} mod_bank={:?} pattern_bank={:?} mode={} reads_enabled={}",
                    s.raw(),
                    s.is_thermal_asserted(),
                    s.current_mod_bank(),
                    s.current_pattern_bank(),
                    if s.is_pattern_mode() {
                        "pattern"
                    } else {
                        "stm"
                    },
                    s.reads_enabled(),
                );
            }
        }
        Err(e) => eprintln!("read fpga state failed: {e}"),
    }

    loop {
        println!();
        for (i, name) in CASE_NAMES.iter().enumerate() {
            println!("[{i}]: {name}");
        }
        println!("[anything else]: quit");
        let sel = prompt("Select a number").await;
        let Ok(i) = sel.trim().parse::<usize>() else {
            break;
        };
        if i >= CASE_NAMES.len() {
            break;
        }

        println!("=== {} ===", CASE_NAMES[i]);
        if let Err(e) = dispatch(i, &ctx).await {
            eprintln!("test failed: {e:?}");
        }

        if let Err(e) = reset(&ctx).await {
            eprintln!("reset failed: {e:?}");
        }
    }
}

async fn reset(ctx: &Ctx<'_>) -> Result<()> {
    use autd3_rs::commands::{Clear, Pattern, SetSilencer};
    use autd3_rs_pattern::null;

    let mut emissions = ctx.geometry.pattern_buffer();
    null(&mut emissions);

    let mut builder = ctx.client.datagram_builder();
    builder
        .push(Pattern::new(&emissions))
        .push(SetSilencer::default());
    let frames = builder.build()?;
    for frame in &frames {
        ctx.client.send_checked(frame).await?;
    }

    ctx.send(Clear).await?;
    Ok(())
}
