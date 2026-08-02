mod admin;
mod system;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use autd3_rs_appliance::{
    Accepted, ApiError, ApplianceStatus, BusActual, BusDesired, BusStatus, ClientStatus,
    ConfigDocument, ImageRelease, LogLines, ProbeResult, UplinkKind, UplinkStatus, WifiCredentials,
};
use autd3_rs_link_remote::{Actual, BusSnapshot, Desired, RemoteLinkError, Sessions, SharedBus};
use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post, put};
use serde::Deserialize;

use crate::config::Config;

const UI: &str = include_str!("ui.html");
const UPDATE_BODY_LIMIT: usize = 64 * 1024 * 1024;
const RESTART_DELAY: Duration = Duration::from_millis(500);
const DEFAULT_LOG_LINES: usize = 200;
const MAX_LOG_LINES: usize = 5000;
const OTHER_LOG_UNITS: &[&str] = &["NetworkManager", "autd3-wifi-init", "autd3-firstboot"];

pub struct AppState {
    bus: Arc<SharedBus>,
    sessions: Arc<Sessions>,
    config_path: PathBuf,
    instance: String,
    interface: String,
    unit: String,
    allow_admin: bool,
    image: Option<ImageRelease>,
    binary: Option<String>,
    started: Instant,
    updating: tokio::sync::Mutex<()>,
}

impl AppState {
    pub fn new(
        config: &Config,
        config_path: PathBuf,
        instance: String,
        bus: Arc<SharedBus>,
        sessions: Arc<Sessions>,
    ) -> Self {
        Self {
            bus,
            sessions,
            config_path,
            instance,
            interface: config.bus.interface.clone().unwrap_or_default(),
            unit: config.control.unit.clone(),
            allow_admin: config.control.allow_admin,
            image: system::image_release(std::path::Path::new(system::IMAGE_RELEASE)),
            binary: std::env::current_exe()
                .ok()
                .map(|path| path.display().to_string()),
            started: Instant::now(),
            updating: tokio::sync::Mutex::new(()),
        }
    }
}

struct Error {
    status: StatusCode,
    message: String,
}

impl Error {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        tracing::warn!(status = %self.status, error = self.message, "control API request failed");
        (
            self.status,
            axum::Json(ApiError {
                error: self.message,
            }),
        )
            .into_response()
    }
}

type ApiResult<T> = std::result::Result<axum::Json<T>, Error>;

fn accepted(message: impl Into<String>) -> axum::Json<Accepted> {
    axum::Json(Accepted {
        message: message.into(),
    })
}

fn require_admin(state: &AppState) -> std::result::Result<(), Error> {
    if state.allow_admin {
        return Ok(());
    }
    Err(Error::new(
        StatusCode::FORBIDDEN,
        "administrative endpoints are disabled; set `control.allow_admin` to true to enable them",
    ))
}

async fn blocking<T, F>(task: F) -> std::result::Result<T, Error>
where
    T: Send + 'static,
    F: FnOnce() -> std::result::Result<T, String> + Send + 'static,
{
    blocking_api(move || task().map_err(Error::internal)).await
}

async fn blocking_api<T, F>(task: F) -> std::result::Result<T, Error>
where
    T: Send + 'static,
    F: FnOnce() -> std::result::Result<T, Error> + Send + 'static,
{
    tokio::task::spawn_blocking(task)
        .await
        .map_err(|e| Error::internal(format!("the task panicked: {e}")))?
}

fn bus_status(snapshot: &BusSnapshot) -> BusStatus {
    let (actual, failure) = match &snapshot.actual {
        Actual::Closed => (BusActual::Closed, None),
        Actual::Opening => (BusActual::Opening, None),
        Actual::Open => (BusActual::Open, None),
        Actual::Recovering => (BusActual::Recovering, None),
        Actual::Failed { reason } => (BusActual::Failed, Some(reason.clone())),
    };
    BusStatus {
        desired: match snapshot.desired {
            Desired::Closed => BusDesired::Closed,
            Desired::Open => BusDesired::Open,
        },
        actual,
        failure,
        num_devices: snapshot.num_devices,
        devices: snapshot.devices.iter().map(ToString::to_string).collect(),
        recoveries: snapshot.recoveries,
        stale_cycles: snapshot.stale_cycles,
        lost_cycles: snapshot.lost_cycles,
        phase_excursions: snapshot.phase_excursions,
        worst_phase_deviation_ns: snapshot.worst_phase_deviation_ns,
        exchanges: snapshot.exchanges,
        exchange_mean_ns: snapshot.exchange_mean_ns,
        exchange_worst_ns: snapshot.exchange_worst_ns,
    }
}

async fn index() -> Html<&'static str> {
    Html(UI)
}

async fn status(State(state): State<Arc<AppState>>) -> ApiResult<ApplianceStatus> {
    let snapshot = state.bus.sampled();
    let interface = state.interface.clone();
    let (nic, uplinks, storage) = blocking(move || {
        Ok((
            system::interface_status(&interface),
            system::uplinks(&interface),
            system::storage(system::DATA_DIR),
        ))
    })
    .await?;
    Ok(axum::Json(ApplianceStatus {
        instance: state.instance.clone(),
        sdk_version: env!("CARGO_PKG_VERSION").to_owned(),
        wire_version: autd3_rs_link_remote::WIRE_VERSION,
        uptime_secs: state.started.elapsed().as_secs(),
        allow_admin: state.allow_admin,
        bus: bus_status(&snapshot),
        binary: state.binary.clone(),
        interface: nic,
        uplinks,
        storage,
        client: state.sessions.current().map(|session| ClientStatus {
            peer: session.peer.to_string(),
            devices: session.devices,
            connected_secs: session.since.elapsed().as_secs(),
        }),
        image: state.image.clone(),
    }))
}

async fn get_config(State(state): State<Arc<AppState>>) -> ApiResult<ConfigDocument> {
    let path = state.config_path.clone();
    let toml = blocking(move || match std::fs::read_to_string(&path) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!("failed to read {}: {e}", path.display())),
    })
    .await?;
    Ok(axum::Json(ConfigDocument { toml }))
}

async fn put_config(
    State(state): State<Arc<AppState>>,
    axum::Json(document): axum::Json<ConfigDocument>,
) -> ApiResult<Accepted> {
    require_admin(&state)?;

    let parsed: Config = toml::from_str(&document.toml)
        .map_err(|e| Error::bad_request(format!("the config does not parse: {e}")))?;
    parsed
        .validate()
        .map_err(|e| Error::bad_request(format!("the config is not usable: {e}")))?;

    let path = state.config_path.clone();
    blocking(move || {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("failed to create {}: {e}", dir.display()))?;
        }
        std::fs::write(&path, document.toml)
            .map_err(|e| format!("failed to write {}: {e}", path.display()))
    })
    .await?;

    tracing::info!(path = %state.config_path.display(), "the config was replaced over the API");
    Ok(accepted("saved; restart the server to apply it"))
}

async fn bus_open(State(state): State<Arc<AppState>>) -> ApiResult<Accepted> {
    state.bus.set_desired(Desired::Open);
    tracing::info!("the bus was asked to open over the API");
    Ok(accepted("the bus was asked to open"))
}

async fn bus_close(State(state): State<Arc<AppState>>) -> ApiResult<Accepted> {
    state.bus.set_desired(Desired::Closed);
    tracing::info!("the bus was asked to close over the API");
    Ok(accepted("the bus was asked to close"))
}

async fn bus_probe(State(state): State<Arc<AppState>>) -> ApiResult<ProbeResult> {
    let bus = Arc::clone(&state.bus);
    let num_devices = tokio::task::spawn_blocking(move || bus.probe())
        .await
        .map_err(|e| Error::internal(format!("the task panicked: {e}")))?
        .map_err(probe_error)?;
    Ok(axum::Json(ProbeResult { num_devices }))
}

fn probe_error(error: RemoteLinkError) -> Error {
    match error {
        RemoteLinkError::ProbeBusOpened
        | RemoteLinkError::ProbeTimeout { .. }
        | RemoteLinkError::BusUnavailable { .. } => {
            Error::new(StatusCode::SERVICE_UNAVAILABLE, error.to_string())
        }
        other => Error::internal(other.to_string()),
    }
}

fn exit_after_reply(reason: &'static str) {
    tracing::info!(reason, "exiting so systemd restarts the server");
    std::thread::spawn(|| {
        std::thread::sleep(RESTART_DELAY);
        std::process::exit(0);
    });
}

async fn restart() -> axum::Json<Accepted> {
    exit_after_reply("restart requested over the API");
    accepted("restarting")
}

async fn reboot(State(state): State<Arc<AppState>>) -> ApiResult<Accepted> {
    require_admin(&state)?;
    let message = blocking(admin::reboot).await?;
    Ok(accepted(if message.is_empty() {
        "rebooting".to_owned()
    } else {
        message
    }))
}

async fn shutdown(State(state): State<Arc<AppState>>) -> ApiResult<Accepted> {
    require_admin(&state)?;
    let message = blocking(admin::shutdown).await?;
    Ok(accepted(if message.is_empty() {
        "shutting down".to_owned()
    } else {
        message
    }))
}

async fn update(State(state): State<Arc<AppState>>, body: Bytes) -> ApiResult<Accepted> {
    require_admin(&state)?;
    if body.is_empty() {
        return Err(Error::bad_request("the request body is empty"));
    }
    let Ok(_updating) = state.updating.try_lock() else {
        return Err(Error::new(
            StatusCode::CONFLICT,
            "another update is already running; wait for it to finish",
        ));
    };

    let version = blocking_api(move || {
        let staged = admin::staged_binary();
        let temp = admin::staging_temp();
        if let Some(dir) = staged.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| Error::internal(format!("failed to create {}: {e}", dir.display())))?;
        }
        let result = stage_and_install(&temp, &staged, &body);
        let _ = std::fs::remove_file(&temp);
        result
    })
    .await?;

    tracing::info!(
        from = env!("CARGO_PKG_VERSION"),
        to = version,
        "the server binary was replaced over the API",
    );
    exit_after_reply("binary updated over the API");
    Ok(accepted(format!("installed {version}; restarting")))
}

fn stage_and_install(
    temp: &std::path::Path,
    staged: &std::path::Path,
    body: &[u8],
) -> std::result::Result<String, Error> {
    std::fs::write(temp, body)
        .map_err(|e| Error::internal(format!("failed to stage {}: {e}", temp.display())))?;
    set_executable(temp).map_err(Error::internal)?;
    let version = admin::version_of(temp)
        .map_err(|reason| Error::new(StatusCode::UNPROCESSABLE_ENTITY, reason))?;
    std::fs::rename(temp, staged).map_err(|e| {
        Error::internal(format!(
            "failed to move {} to {}: {e}",
            temp.display(),
            staged.display(),
        ))
    })?;
    if let Err(e) = admin::install_staged() {
        let _ = std::fs::remove_file(staged);
        return Err(Error::internal(e));
    }
    Ok(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bad_upload_is_the_uploaders_fault_not_a_server_fault() {
        let dir = std::env::temp_dir().join(format!("autd3-update-reject-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let temp = dir.join("staged.tmp");
        let staged = dir.join("staged");
        let err = stage_and_install(&temp, &staged, b"#!/bin/sh\necho not-this-server 1.0\n")
            .unwrap_err();
        assert_eq!(err.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(!staged.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn uplink(kind: UplinkKind, carrier: bool, addresses: &[&str]) -> UplinkStatus {
        UplinkStatus {
            name: "eth0".to_owned(),
            kind,
            operstate: if carrier { "up" } else { "down" }.to_owned(),
            carrier,
            addresses: addresses.iter().map(|a| (*a).to_owned()).collect(),
            wifi: None,
        }
    }

    #[test]
    fn only_a_wired_uplink_that_carries_an_address_makes_wifi_droppable() {
        let wired = uplink(UplinkKind::Ethernet, true, &["192.168.0.2"]);
        assert!(reachable_without_wifi(std::slice::from_ref(&wired)));
        assert!(reachable_without_wifi(&[
            uplink(UplinkKind::Wifi, true, &["192.168.0.3"]),
            wired,
        ]));

        assert!(!reachable_without_wifi(&[]));
        for lonely in [
            uplink(UplinkKind::Ethernet, false, &["192.168.0.2"]),
            uplink(UplinkKind::Ethernet, true, &[]),
            uplink(UplinkKind::Wifi, true, &["192.168.0.3"]),
        ] {
            assert!(
                !reachable_without_wifi(std::slice::from_ref(&lonely)),
                "{lonely:?}",
            );
        }
    }
}

#[cfg(unix)]
fn set_executable(path: &std::path::Path) -> std::result::Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("failed to make {} executable: {e}", path.display()))
}

#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
fn set_executable(_path: &std::path::Path) -> std::result::Result<(), String> {
    Ok(())
}

async fn set_wifi(
    State(state): State<Arc<AppState>>,
    axum::Json(credentials): axum::Json<WifiCredentials>,
) -> ApiResult<Accepted> {
    require_admin(&state)?;
    admin::validate_wifi(&credentials).map_err(Error::bad_request)?;
    blocking(move || admin::set_wifi(&credentials)).await?;
    Ok(accepted("the Wi-Fi credentials were stored"))
}

#[derive(Deserialize)]
struct WifiForgetQuery {
    #[serde(default)]
    radio_off: bool,
    #[serde(default)]
    force: bool,
}

fn reachable_without_wifi(uplinks: &[UplinkStatus]) -> bool {
    uplinks.iter().any(|uplink| {
        uplink.kind == UplinkKind::Ethernet && uplink.carrier && !uplink.addresses.is_empty()
    })
}

async fn forget_wifi(
    State(state): State<Arc<AppState>>,
    Query(query): Query<WifiForgetQuery>,
) -> ApiResult<Accepted> {
    require_admin(&state)?;
    if !query.force {
        let interface = state.interface.clone();
        let uplinks = blocking(move || Ok(system::uplinks(&interface))).await?;
        if !reachable_without_wifi(&uplinks) {
            return Err(Error::new(
                StatusCode::CONFLICT,
                "no wired uplink carries an address, so dropping Wi-Fi would take this appliance \
                 off the network; connect an Ethernet cable or repeat the request with `force`",
            ));
        }
    }
    blocking(move || admin::forget_wifi(query.radio_off)).await?;
    Ok(accepted(if query.radio_off {
        "the Wi-Fi profile was removed and the radio is blocked"
    } else {
        "the Wi-Fi profile was removed"
    }))
}

#[derive(Deserialize)]
struct LogQuery {
    lines: Option<usize>,
    unit: Option<String>,
}

async fn logs(
    State(state): State<Arc<AppState>>,
    Query(query): Query<LogQuery>,
) -> ApiResult<LogLines> {
    let count = query.lines.unwrap_or(DEFAULT_LOG_LINES).min(MAX_LOG_LINES);
    let unit = match query.unit {
        None => state.unit.clone(),
        Some(unit) if unit == state.unit || OTHER_LOG_UNITS.contains(&unit.as_str()) => unit,
        Some(unit) => {
            return Err(Error::bad_request(format!(
                "`{unit}` is not a unit this appliance exposes; ask for {} or {}",
                state.unit,
                OTHER_LOG_UNITS.join(", "),
            )));
        }
    };
    let lines = blocking(move || system::journal_tail(&unit, count)).await?;
    Ok(axum::Json(LogLines { lines }))
}

fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/status", get(status))
        .route("/config", get(get_config).put(put_config))
        .route("/bus/open", post(bus_open))
        .route("/bus/close", post(bus_close))
        .route("/bus/probe", post(bus_probe))
        .route("/restart", post(restart))
        .route("/reboot", post(reboot))
        .route("/shutdown", post(shutdown))
        .route(
            "/update",
            post(update).layer(DefaultBodyLimit::max(UPDATE_BODY_LIMIT)),
        )
        .route("/network/wifi", put(set_wifi).delete(forget_wifi))
        .route("/logs", get(logs))
        .with_state(state)
}

pub fn spawn(config: &Config, state: Arc<AppState>) -> Result<()> {
    let bind = config.control.bind;
    std::thread::Builder::new()
        .name("autd3-remote-control".to_owned())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(e) => {
                    tracing::error!(error = %e, "failed to start the control API runtime");
                    return;
                }
            };
            runtime.block_on(async move {
                let listener = match tokio::net::TcpListener::bind(bind).await {
                    Ok(listener) => listener,
                    Err(e) => {
                        tracing::error!(%bind, error = %e, "failed to bind the control API");
                        return;
                    }
                };
                tracing::info!(%bind, "control API listening");
                if let Err(e) = axum::serve(listener, router(state)).await {
                    tracing::error!(error = %e, "the control API stopped");
                }
            });
        })
        .context("failed to spawn the control API thread")?;
    Ok(())
}
