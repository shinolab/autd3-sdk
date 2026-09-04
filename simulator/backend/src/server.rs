use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use axum::{
    Router,
    body::Body,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use rust_embed::RustEmbed;
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

#[derive(RustEmbed)]
#[folder = "web/"]
struct WebAssets;

pub fn router(state: AppState, web_dir: Option<PathBuf>) -> Router {
    let router = Router::new().route("/ws", get(ws_handler));
    let router = match web_dir {
        Some(dir) => router.fallback_service(ServeDir::new(dir)),
        None if WebAssets::get("index.html").is_some() => router.fallback(embedded_handler),
        None => router.route("/", get(|| async { "autd3-rs-simulator backend" })),
    };
    router.with_state(state)
}

async fn embedded_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    match WebAssets::get(path) {
        Some(file) => (
            [(header::CONTENT_TYPE, file.metadata.mimetype())],
            Body::from(file.data.into_owned()),
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
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
                apply_client_message(&control, &text);
            }
        }
    };

    tokio::select! {
        () = send_task => {}
        () = recv_task => {}
    }
}

fn apply_client_message(control: &ControlState, text: &str) {
    match serde_json::from_str::<ClientMsg>(text) {
        Ok(ClientMsg::SetModulationEnabled { enabled }) => {
            control.mod_enabled.store(enabled, Ordering::Relaxed);
        }
        Err(e) => tracing::error!("failed to decode client message: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_modulation_enabled_updates_the_control_state() {
        let control = ControlState::default();
        apply_client_message(
            &control,
            r#"{"type":"set_modulation_enabled","enabled":false}"#,
        );
        assert!(!control.mod_enabled.load(Ordering::Relaxed));
        apply_client_message(
            &control,
            r#"{"type":"set_modulation_enabled","enabled":true}"#,
        );
        assert!(control.mod_enabled.load(Ordering::Relaxed));
    }

    #[test]
    fn undecodable_client_message_leaves_the_control_state_untouched() {
        let control = ControlState::default();
        control.mod_enabled.store(false, Ordering::Relaxed);
        for text in [
            "",
            "{}",
            r#"{"type":"unknown"}"#,
            r#"{"type":"set_modulation_enabled"}"#,
        ] {
            apply_client_message(&control, text);
            assert!(!control.mod_enabled.load(Ordering::Relaxed));
        }
    }
}
