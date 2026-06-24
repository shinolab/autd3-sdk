use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
    routing::get,
};
use tokio::sync::watch;
use tower_http::services::ServeDir;

#[derive(Clone)]
pub struct AppState {
    pub geometry_json: Arc<str>,
    pub state_rx: watch::Receiver<Arc<str>>,
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

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    if socket
        .send(Message::Text(state.geometry_json.as_ref().into()))
        .await
        .is_err()
    {
        return;
    }

    let mut rx = state.state_rx.clone();
    loop {
        let json = rx.borrow_and_update().clone();
        if socket
            .send(Message::Text(json.as_ref().into()))
            .await
            .is_err()
        {
            return;
        }
        if rx.changed().await.is_err() {
            return;
        }
    }
}
