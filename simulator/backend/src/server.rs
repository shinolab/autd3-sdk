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
    response::IntoResponse,
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use rust_embed::RustEmbed;
use tokio::sync::watch;
use tower_http::services::ServeDir;

use autd3_rs_simulator_protocol::ClientMsg;

use crate::emulator::geometry_from_poses;
use crate::scene::Scene;

#[derive(Clone)]
pub(crate) struct AppState {
    pub geometry_rx: watch::Receiver<Arc<str>>,
    pub state_rx: watch::Receiver<Arc<str>>,
    pub device_rx: watch::Receiver<Arc<str>>,
    pub mod_enabled: Arc<AtomicBool>,
    pub scene: Arc<Scene>,
}

#[derive(RustEmbed)]
#[folder = "web/"]
struct WebAssets;

pub(crate) fn router(state: AppState, web_dir: Option<PathBuf>) -> Router {
    async fn handle_socket(socket: WebSocket, state: AppState) {
        let (mut sender, mut receiver) = socket.split();
        let AppState {
            mut geometry_rx,
            mut state_rx,
            mut device_rx,
            mod_enabled,
            scene,
        } = state;

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
                    biased;
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

        let recv_task = async move {
            while let Some(Ok(message)) = receiver.next().await {
                if let Message::Text(text) = message {
                    let mod_enabled = Arc::clone(&mod_enabled);
                    let scene = Arc::clone(&scene);
                    if let Err(e) = tokio::task::spawn_blocking(move || {
                        apply_client_message(&mod_enabled, &scene, &text);
                    })
                    .await
                    {
                        tracing::error!("failed to apply a client message: {e}");
                    }
                }
            }
        };

        tokio::select! {
            () = send_task => {}
            () = recv_task => {}
        }
    }

    let router = Router::new().route(
        "/ws",
        get(
            |ws: WebSocketUpgrade, State(state): State<AppState>| async move {
                ws.on_upgrade(|socket| handle_socket(socket, state))
            },
        ),
    );
    let router = match web_dir {
        Some(dir) => router.fallback_service(ServeDir::new(dir)),
        None if WebAssets::get("index.html").is_some() => router.fallback(|uri: Uri| async move {
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
        }),
        None => router.route("/", get(|| async { "autd3-simulator backend" })),
    };
    router.with_state(state)
}

fn apply_client_message(mod_enabled: &AtomicBool, scene: &Scene, text: &str) {
    match serde_json::from_str::<ClientMsg>(text) {
        Ok(ClientMsg::SetModulationEnabled { enabled }) => {
            mod_enabled.store(enabled, Ordering::Relaxed);
        }
        Ok(ClientMsg::SetGeometry { devices }) => match geometry_from_poses(&devices) {
            Ok(geometry) => match scene.replace(&geometry) {
                Ok(()) => tracing::info!(
                    "the GUI set the geometry: {} emulated device(s)",
                    geometry.num_devices()
                ),
                Err(e) => tracing::error!("failed to rebuild the emulated devices: {e}"),
            },
            Err(e) => tracing::warn!("refused a geometry from the GUI: {e}"),
        },
        Err(e) => tracing::error!("failed to decode client message: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use autd3_rs::geometry::{Autd3, Geometry, Point3, UnitQuaternion};
    use autd3_rs_simulator_protocol::DevicePose;

    use crate::emulator::device_poses;
    use crate::scene::tests::{published_poses, scene};

    fn set_geometry(devices: &[DevicePose]) -> String {
        serde_json::to_string(&ClientMsg::SetGeometry {
            devices: devices.to_vec(),
        })
        .unwrap()
    }

    #[test]
    fn set_modulation_enabled_updates_the_control_state() {
        let scene = scene(1);
        let mod_enabled = AtomicBool::new(true);
        apply_client_message(
            &mod_enabled,
            &scene,
            r#"{"type":"set_modulation_enabled","enabled":false}"#,
        );
        assert!(!mod_enabled.load(Ordering::Relaxed));
        apply_client_message(
            &mod_enabled,
            &scene,
            r#"{"type":"set_modulation_enabled","enabled":true}"#,
        );
        assert!(mod_enabled.load(Ordering::Relaxed));
    }

    #[test]
    fn undecodable_client_message_leaves_the_control_state_untouched() {
        let scene = scene(1);
        let mut rx = scene.geometry();
        rx.borrow_and_update();
        let mod_enabled = AtomicBool::new(false);
        for text in [
            "",
            "{}",
            r#"{"type":"unknown"}"#,
            r#"{"type":"set_modulation_enabled"}"#,
            r#"{"type":"set_geometry"}"#,
        ] {
            apply_client_message(&mod_enabled, &scene, text);
            assert!(!mod_enabled.load(Ordering::Relaxed));
        }
        assert!(!rx.has_changed().unwrap());
    }

    #[test]
    fn set_geometry_replaces_the_scene() {
        let scene = scene(1);
        let mut rx = scene.geometry();
        let geometry = Geometry::new(vec![
            Autd3::default(),
            Autd3::new(
                Point3::new(Autd3::DEVICE_WIDTH, 0.0, 0.0),
                UnitQuaternion::identity(),
            ),
        ]);
        apply_client_message(
            &AtomicBool::new(true),
            &scene,
            &set_geometry(&device_poses(&geometry)),
        );
        assert_eq!(scene.sample(|devices| devices.len()), Some(2));
        assert_eq!(published_poses(&mut rx), device_poses(&geometry));
    }

    #[test]
    fn an_invalid_geometry_from_the_gui_is_refused() {
        let scene = scene(1);
        let mut rx = scene.geometry();
        rx.borrow_and_update();
        let identity = DevicePose {
            origin: [0.0; 3],
            rotation: [1.0, 0.0, 0.0, 0.0],
        };
        for devices in [
            Vec::new(),
            vec![identity; autd3_rs::MAX_DEVICES + 1],
            vec![DevicePose {
                rotation: [0.5, 0.0, 0.0, 0.0],
                ..identity
            }],
        ] {
            apply_client_message(&AtomicBool::new(true), &scene, &set_geometry(&devices));
        }
        assert!(!rx.has_changed().unwrap());
        assert_eq!(scene.sample(|devices| devices.len()), Some(1));
    }
}
