mod emulator;
mod link;
mod server;

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use autd3_rs_link_remote::RemoteServer;
use autd3_rs_simulator_protocol::ServerMsg;
use clap::Parser;
use tokio::sync::watch;

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

    let link = EmulatorLink::new(transducer_counts);
    let states = link.states();

    let link_addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, args.link_port));
    std::thread::spawn(move || match RemoteServer::with_link(link_addr, link) {
        Ok(mut server) => {
            tracing::info!("remote link server listening on {link_addr}");
            if let Err(e) = server.serve() {
                tracing::error!("remote link server stopped: {e}");
            }
        }
        Err(e) => tracing::error!("failed to start remote link server: {e}"),
    });

    let empty_state: Arc<str> =
        serde_json::to_string(&ServerMsg::State { states: Vec::new() })?.into();
    let (state_tx, state_rx) = watch::channel(empty_state);
    tokio::spawn(async move {
        let mut last = String::new();
        let mut tick = tokio::time::interval(Duration::from_millis(33));
        loop {
            tick.tick().await;
            let snapshot = match states.lock() {
                Ok(guard) => guard.clone(),
                Err(_) => continue,
            };
            if snapshot.is_empty() {
                continue;
            }
            match serde_json::to_string(&ServerMsg::State { states: snapshot }) {
                Ok(json) if json != last => {
                    last.clone_from(&json);
                    let _ = state_tx.send(json.into());
                }
                Ok(_) => {}
                Err(e) => tracing::error!("failed to serialize state: {e}"),
            }
        }
    });

    let app = router(
        AppState {
            geometry_json,
            state_rx,
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
