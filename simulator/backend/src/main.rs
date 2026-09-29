mod control;
mod emulator;
#[cfg(test)]
mod harness;
mod server;

use std::net::{Ipv6Addr, SocketAddrV6};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::udp::emulator::UdpEmulator;
use autd3_rs_core::rt::{TracingOption, init_tracing};
use autd3_rs_simulator_protocol::{DeviceState, ServerMsg, TransState};
use clap::Parser;
use tokio::sync::watch;

use crate::control::ControlState;
use crate::emulator::{extract_device_states, extract_states_into, geometry_msg};
use crate::server::{AppState, router};

pub const DEFAULT_GROUP_PORT: u16 = 44336;
const STATE_PERIOD: Duration = Duration::from_millis(33);
const DEVICE_STATE_PERIOD: Duration = Duration::from_millis(200);

type SharedStates = Arc<Mutex<Vec<TransState>>>;
type SharedDeviceStates = Arc<Mutex<Vec<DeviceState>>>;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value_t = 8081)]
    http_port: u16,
    #[arg(
        long,
        default_value_t = SocketAddrV6::new(Ipv6Addr::LOCALHOST, DEFAULT_GROUP_PORT, 0, 0),
        help = "Loopback address the emulated devices take the multicast management messages on; \
                open the client on the same host with TransportOption.group set to it"
    )]
    group: SocketAddrV6,
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
    let control = Arc::new(ControlState::default());
    let states: SharedStates = Arc::new(Mutex::new(Vec::new()));
    let device_states: SharedDeviceStates = Arc::new(Mutex::new(Vec::new()));

    let emulator = Arc::new(
        UdpEmulator::spawn_at(args.group, geometry.num_devices())
            .with_context(|| format!("binding the emulated devices at {}", args.group))?,
    );
    tracing::info!(
        "{} emulated device(s) take management messages on {}",
        geometry.num_devices(),
        emulator.group()
    );
    spawn_sampler(
        Arc::clone(&emulator),
        Arc::clone(&states),
        Arc::clone(&device_states),
        Arc::clone(&control),
    );

    let geometry_json: Arc<str> = serde_json::to_string(&geometry_msg(&geometry))?.into();
    let (_geometry_tx, geometry_rx) = watch::channel(geometry_json);

    let state_rx =
        spawn_json_broadcaster(states, STATE_PERIOD, |states| ServerMsg::State { states })?;
    let device_rx = spawn_json_broadcaster(device_states, DEVICE_STATE_PERIOD, |devices| {
        ServerMsg::DeviceStates { devices }
    })?;

    let app = router(
        AppState {
            geometry_rx,
            state_rx,
            device_rx,
            control,
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
    states: SharedStates,
    device_states: SharedDeviceStates,
    control: Arc<ControlState>,
) {
    std::thread::spawn(move || {
        let mut buffer = Vec::new();
        loop {
            let mod_enabled = control.mod_enabled.load(Ordering::Relaxed);
            let devices = emulator.with_devices(|devices| {
                extract_states_into(devices, &mut buffer, mod_enabled);
                extract_device_states(devices)
            });
            if let Ok(mut guard) = states.lock() {
                guard.clone_from(&buffer);
            }
            if let Ok(mut guard) = device_states.lock() {
                *guard = devices;
            }
            std::thread::sleep(STATE_PERIOD);
        }
    });
}

fn spawn_json_broadcaster<T, F>(
    source: Arc<std::sync::Mutex<Vec<T>>>,
    period: Duration,
    to_msg: F,
) -> Result<watch::Receiver<Arc<str>>>
where
    T: Clone + Send + 'static,
    F: Fn(Vec<T>) -> ServerMsg + Send + 'static,
{
    let initial: Arc<str> = serde_json::to_string(&to_msg(Vec::new()))?.into();
    let (tx, rx) = watch::channel(initial);
    tokio::spawn(async move {
        let mut last = String::new();
        let mut tick = tokio::time::interval(period);
        loop {
            tick.tick().await;
            let snapshot = match source.lock() {
                Ok(guard) => guard.clone(),
                Err(_) => continue,
            };
            if snapshot.is_empty() {
                continue;
            }
            match serde_json::to_string(&to_msg(snapshot)) {
                Ok(json) if json != last => {
                    last.clone_from(&json);
                    let _ = tx.send(json.into());
                }
                Ok(_) => {}
                Err(e) => tracing::error!("failed to serialize broadcast payload: {e}"),
            }
        }
    });
    Ok(rx)
}
