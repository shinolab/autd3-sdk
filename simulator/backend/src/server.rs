use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

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

#[derive(Clone)]
pub struct AppState {
    pub geometry: Arc<str>,
    pub state_rx: watch::Receiver<Arc<str>>,
    pub device_rx: watch::Receiver<Arc<str>>,
    pub mod_enabled: Arc<AtomicBool>,
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
    let AppState {
        geometry,
        mut state_rx,
        mut device_rx,
        mod_enabled,
    } = state;

    let initial = [
        geometry,
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

    let recv_task = async move {
        while let Some(Ok(message)) = receiver.next().await {
            if let Message::Text(text) = message {
                apply_client_message(&mod_enabled, &text);
            }
        }
    };

    tokio::select! {
        () = send_task => {}
        () = recv_task => {}
    }
}

fn apply_client_message(mod_enabled: &AtomicBool, text: &str) {
    match serde_json::from_str::<ClientMsg>(text) {
        Ok(ClientMsg::SetModulationEnabled { enabled }) => {
            mod_enabled.store(enabled, Ordering::Relaxed);
        }
        Err(e) => tracing::error!("failed to decode client message: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_modulation_enabled_updates_the_control_state() {
        let mod_enabled = AtomicBool::new(true);
        apply_client_message(
            &mod_enabled,
            r#"{"type":"set_modulation_enabled","enabled":false}"#,
        );
        assert!(!mod_enabled.load(Ordering::Relaxed));
        apply_client_message(
            &mod_enabled,
            r#"{"type":"set_modulation_enabled","enabled":true}"#,
        );
        assert!(mod_enabled.load(Ordering::Relaxed));
    }

    #[test]
    fn undecodable_client_message_leaves_the_control_state_untouched() {
        let mod_enabled = AtomicBool::new(false);
        for text in [
            "",
            "{}",
            r#"{"type":"unknown"}"#,
            r#"{"type":"set_modulation_enabled"}"#,
        ] {
            apply_client_message(&mod_enabled, text);
            assert!(!mod_enabled.load(Ordering::Relaxed));
        }
    }
}
