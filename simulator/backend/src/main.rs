mod control;
mod emulator;
mod link;
mod server;

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use autd3_rs_link_remote::{DeviceLayout, RemoteLinkError, RemoteServer};
use autd3_rs_simulator_protocol::ServerMsg;
use clap::Parser;
use tokio::sync::watch;

use crate::control::ControlState;
use crate::emulator::geometry_msg_from_layout;
use crate::link::{EmulatorLink, SharedDeviceStates, SharedStates};
use crate::server::{AppState, router};

#[derive(Parser)]
struct Args {
    #[arg(long, default_value_t = 8081)]
    http_port: u16,
    #[arg(long, default_value_t = 8080)]
    link_port: u16,
    #[arg(long)]
    web_dir: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    let control = Arc::new(ControlState::default());
    let states: SharedStates = Arc::new(Mutex::new(Vec::new()));
    let device_states: SharedDeviceStates = Arc::new(Mutex::new(Vec::new()));

    let empty_geometry: Arc<str> = serde_json::to_string(&ServerMsg::Geometry {
        transducers: Vec::new(),
    })?
    .into();
    let (geometry_tx, geometry_rx) = watch::channel(empty_geometry);

    let link_addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, args.link_port));
    {
        let states = Arc::clone(&states);
        let device_states = Arc::clone(&device_states);
        let control = Arc::clone(&control);
        std::thread::spawn(move || {
            let factory = move |layout: &[DeviceLayout]| -> Result<EmulatorLink, RemoteLinkError> {
                match serde_json::to_string(&geometry_msg_from_layout(layout)) {
                    Ok(json) => {
                        let _ = geometry_tx.send(json.into());
                    }
                    Err(e) => tracing::error!("failed to serialize client geometry: {e}"),
                }
                let counts: Vec<usize> = layout.iter().map(|d| d.transducers.len()).collect();
                Ok(EmulatorLink::new(
                    counts,
                    Arc::clone(&states),
                    Arc::clone(&device_states),
                    Arc::clone(&control),
                ))
            };
            tracing::info!("remote link server listening on {link_addr}");
            if let Err(e) = RemoteServer::serve_with_factory(link_addr, factory) {
                tracing::error!("remote link server stopped: {e}");
            }
        });
    }

    let state_rx = spawn_json_broadcaster(states, Duration::from_millis(33), |states| {
        ServerMsg::State { states }
    })?;
    let device_rx = spawn_json_broadcaster(device_states, Duration::from_millis(200), |devices| {
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
