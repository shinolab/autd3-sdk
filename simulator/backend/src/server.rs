use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use axum::{
    Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::watch;
use tower_http::services::ServeDir;

use autd3_rs_simulator_protocol::ClientMsg;

use crate::control::ControlState;

#[derive(Clone)]
pub struct AppState {
    pub geometry_rx: watch::Receiver<Arc<str>>,
    pub state_rx: watch::Receiver<Arc<str>>,
    pub device_rx: watch::Receiver<Arc<str>>,
    pub control: Arc<ControlState>,
}

pub fn router(state: AppState, web_dir: Option<PathBuf>) -> Router {
    let router = Router::new().route("/ws", get(ws_handler));
    let router = match web_dir {
        Some(dir) => router.fallback_service(ServeDir::new(dir)),
        None => router.route("/", get(|| async { "autd3-rs-simulator backend" })),
    };
    router.with_state(state)
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let mut geometry_rx = state.geometry_rx.clone();
    let mut state_rx = state.state_rx.clone();
    let mut device_rx = state.device_rx.clone();

    let initial = [
        geometry_rx.borrow_and_update().clone(),
        state_rx.borrow_and_update().clone(),
        device_rx.borrow_and_update().clone(),
    ];
    for message in initial {
        if sender
            .send(Message::Text(message.as_ref().into()))
            .await
            .is_err()
        {
            return;
        }
    }

    let send_task = async move {
        loop {
            let message = tokio::select! {
                changed = geometry_rx.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    geometry_rx.borrow_and_update().clone()
                }
                changed = state_rx.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    state_rx.borrow_and_update().clone()
                }
                changed = device_rx.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    device_rx.borrow_and_update().clone()
                }
            };
            if sender
                .send(Message::Text(message.as_ref().into()))
                .await
                .is_err()
            {
                break;
            }
        }
    };

    let control = state.control;
    let recv_task = async move {
        while let Some(Ok(message)) = receiver.next().await {
            if let Message::Text(text) = message {
                match serde_json::from_str::<ClientMsg>(&text) {
                    Ok(ClientMsg::SetModulationEnabled { enabled }) => {
                        control.mod_enabled.store(enabled, Ordering::Relaxed);
                    }
                    Err(e) => tracing::error!("failed to decode client message: {e}"),
                }
            }
        }
    };

    tokio::select! {
        () = send_task => {}
        () = recv_task => {}
    }
}
