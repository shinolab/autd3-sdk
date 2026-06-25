mod control;
mod emulator;
mod link;
mod server;

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use autd3_rs_link_remote::RemoteServer;
use autd3_rs_simulator_protocol::{ServerMsg, TransducerInfo};
use clap::Parser;
use tokio::sync::watch;

use crate::control::ControlState;
use crate::emulator::{build_geometry, geometry_msg};
use crate::link::EmulatorLink;
use crate::server::{AppState, router};

#[derive(Parser)]
struct Args {
    #[arg(long, default_value_t = 8081)]
    http_port: u16,
    #[arg(long, default_value_t = 8080)]
    link_port: u16,
    #[arg(long, default_value_t = 1)]
    devices: usize,
    #[arg(long)]
    web_dir: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    let geometry = build_geometry(args.devices);
    let geometry_json: Arc<str> = serde_json::to_string(&geometry_msg(&geometry))?.into();
    let transducer_counts: Vec<usize> = geometry.iter().map(|d| d.positions().len()).collect();

    let control = Arc::new(ControlState::default());
    let link = EmulatorLink::new(transducer_counts, Arc::clone(&control));
    let states = link.states();
    let device_states = link.device_states();

    let (geometry_tx, geometry_rx) = watch::channel(geometry_json);

    let link_addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, args.link_port));
    std::thread::spawn(move || {
        let on_geometry = move |layout: Vec<autd3_rs_link_remote::TransducerLayout>| {
            let transducers = layout
                .iter()
                .map(|t| TransducerInfo {
                    pos: t.pos,
                    dir: t.dir,
                })
                .collect();
            match serde_json::to_string(&ServerMsg::Geometry { transducers }) {
                Ok(json) => {
                    let _ = geometry_tx.send(json.into());
                }
                Err(e) => tracing::error!("failed to serialize client geometry: {e}"),
            }
        };
        match RemoteServer::with_link_and_geometry(link_addr, link, on_geometry) {
            Ok(mut server) => {
                tracing::info!("remote link server listening on {link_addr}");
                if let Err(e) = server.serve() {
                    tracing::error!("remote link server stopped: {e}");
                }
            }
            Err(e) => tracing::error!("failed to start remote link server: {e}"),
        }
    });

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
