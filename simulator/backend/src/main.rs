mod control;
mod emulator;
mod geometry_file;
#[cfg(test)]
mod harness;
mod server;

use std::net::{Ipv6Addr, SocketAddrV6};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use autd3_rs_firmware_emulator::udp::UdpEmulator;
use autd3_rs_simulator_control::{DEFAULT_PORT, control_port};
use autd3_rs_simulator_protocol::ServerMsg;
use clap::Parser;
use tokio::sync::watch;
use tracing_subscriber::EnvFilter;

use crate::control::Scene;
use crate::emulator::{extract_device_states, extract_states};
use crate::server::{AppState, router};

const STATE_PERIOD: Duration = Duration::from_millis(33);
const DEVICE_STATE_EVERY: u32 = 6;

type JsonReceiver = watch::Receiver<Arc<str>>;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value_t = DEFAULT_PORT)]
    port: u16,
    #[arg(long, default_value_t = 8081)]
    http_port: u16,
    #[arg(long)]
    web_dir: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    let args = Args::parse();

    let mod_enabled = Arc::new(AtomicBool::new(true));

    let port = args.port;
    let Some(control_port) = control_port(port) else {
        bail!(
            "--port must be within 1..=65534 (the control port is port + 1), but {port} was given"
        );
    };
    let saved_geometry = geometry_file::default_path();
    if let Some(path) = &saved_geometry {
        tracing::info!(
            "the geometry set from the GUI is saved to {}",
            path.display()
        );
    } else {
        tracing::warn!("no configuration directory; the geometry set from the GUI is not saved");
    }
    let geometry = geometry_file::load_or_default(saved_geometry.as_deref());
    let scene = Scene::new(
        Box::new(move |num_devices| UdpEmulator::spawn_simulator(port, num_devices)),
        &geometry,
        saved_geometry,
    )
    .context("binding the emulated devices; is another simulator running?")?;
    let control = control::spawn(
        Arc::clone(&scene),
        SocketAddrV6::new(Ipv6Addr::LOCALHOST, control_port, 0, 0),
    )
    .context("binding the control port; is another simulator running?")?;
    tracing::info!("emulated devices on [::1]:{port}; set the geometry in the Geometry tab");
    tracing::debug!("control port on {control}");
    let geometry_rx = scene.geometry();
    let (state_rx, device_rx) = spawn_sampler(Arc::clone(&scene), Arc::clone(&mod_enabled))?;

    let app = router(
        AppState {
            geometry_rx,
            state_rx,
            device_rx,
            mod_enabled,
            scene,
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
    scene: Arc<Scene>,
    mod_enabled: Arc<AtomicBool>,
) -> Result<(JsonReceiver, JsonReceiver)> {
    let (states, devices) = scene
        .sample(|devices| {
            (
                extract_states(devices, mod_enabled.load(Ordering::Relaxed)),
                extract_device_states(devices),
            )
        })
        .unwrap_or_default();
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
            let Some((states, devices)) = scene.sample(|devices| {
                (
                    extract_states(devices, enabled),
                    (tick == 0).then(|| extract_device_states(devices)),
                )
            }) else {
                continue;
            };
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
