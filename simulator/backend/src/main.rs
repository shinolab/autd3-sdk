mod emulator;
#[cfg(test)]
mod harness;
mod server;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs_core::rt::{TracingOption, init_tracing};
use autd3_rs_firmware_emulator::udp::UdpEmulator;
use autd3_rs_simulator_protocol::ServerMsg;
use clap::Parser;
use tokio::sync::watch;

use crate::emulator::{extract_device_states, extract_states, geometry_msg};
use crate::server::{AppState, router};

const STATE_PERIOD: Duration = Duration::from_millis(33);
const DEVICE_STATE_EVERY: u32 = 6;

type JsonReceiver = watch::Receiver<Arc<str>>;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value_t = 8081)]
    http_port: u16,
    #[arg(
        long,
        help = "Geometry JSON (Geometry::to_json). Omit for a single AUTD3 at the origin"
    )]
    geometry: Option<PathBuf>,
    #[arg(long)]
    web_dir: Option<PathBuf>,
}

fn load_geometry(path: Option<&PathBuf>) -> Result<Geometry> {
    let Some(path) = path else {
        return Ok(Geometry::new(vec![Autd3::default()]));
    };
    let json = std::fs::read_to_string(path)
        .with_context(|| format!("reading the geometry {}", path.display()))?;
    let geometry = Geometry::from_json(&json)
        .with_context(|| format!("parsing the geometry {}", path.display()))?;
    anyhow::ensure!(
        geometry.num_devices() > 0,
        "the geometry {} has no device",
        path.display()
    );
    Ok(geometry)
}

#[tokio::main]
async fn main() -> Result<()> {
    let _log_guard = init_tracing(TracingOption::default());
    let args = Args::parse();

    let geometry = load_geometry(args.geometry.as_ref())?;
    let mod_enabled = Arc::new(AtomicBool::new(true));

    let emulator = Arc::new(
        UdpEmulator::spawn_simulator(geometry.num_devices())
            .context("binding the emulated devices; is another simulator running?")?,
    );
    tracing::info!(
        "{} emulated device(s) take management messages on {}",
        geometry.num_devices(),
        emulator.addr()
    );
    let (state_rx, device_rx) = spawn_sampler(emulator, Arc::clone(&mod_enabled))?;

    let app = router(
        AppState {
            geometry: serde_json::to_string(&geometry_msg(&geometry))?.into(),
            state_rx,
            device_rx,
            mod_enabled,
        },
        args.web_dir,
    );
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", args.http_port)).await?;
    tracing::info!(
        "simulator http listening on http://localhost:{}",
        args.http_port
    );
    axum::serve(listener, app).await?;
    Ok(())
}

fn spawn_sampler(
    emulator: Arc<UdpEmulator>,
    mod_enabled: Arc<AtomicBool>,
) -> Result<(JsonReceiver, JsonReceiver)> {
    let (states, devices) = emulator.with_devices(|devices| {
        (
            extract_states(devices, mod_enabled.load(Ordering::Relaxed)),
            extract_device_states(devices),
        )
    });
    let mut last_state = serde_json::to_string(&ServerMsg::State { states })?;
    let mut last_device = serde_json::to_string(&ServerMsg::DeviceStates { devices })?;
    let (state_tx, state_rx) = watch::channel(Arc::from(last_state.as_str()));
    let (device_tx, device_rx) = watch::channel(Arc::from(last_device.as_str()));
    std::thread::spawn(move || {
        let mut tick = 0;
        loop {
            std::thread::sleep(STATE_PERIOD);
            tick = (tick + 1) % DEVICE_STATE_EVERY;
            let enabled = mod_enabled.load(Ordering::Relaxed);
            let (states, devices) = emulator.with_devices(|devices| {
                (
                    extract_states(devices, enabled),
                    (tick == 0).then(|| extract_device_states(devices)),
                )
            });
            publish(&state_tx, &mut last_state, &ServerMsg::State { states });
            if let Some(devices) = devices {
                publish(
                    &device_tx,
                    &mut last_device,
                    &ServerMsg::DeviceStates { devices },
                );
            }
        }
    });
    Ok((state_rx, device_rx))
}

fn publish(tx: &watch::Sender<Arc<str>>, last: &mut String, msg: &ServerMsg) {
    match serde_json::to_string(msg) {
        Ok(json) if json != *last => {
            last.clone_from(&json);
            let _ = tx.send(json.into());
        }
        Ok(_) => {}
        Err(e) => tracing::error!("failed to serialize broadcast payload: {e}"),
    }
}
