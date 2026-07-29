use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use axoupdater::{AxoUpdater, ReleaseSource, ReleaseSourceType, Version};
use eframe::egui;
use serde::{Deserialize, Serialize};

const APP_NAME: &str = "console";
const REPO_OWNER: &str = "shinolab";
const REPO_NAME: &str = "autd3-sdk";
pub const RELEASES_URL: &str = "https://github.com/shinolab/autd3-sdk/releases";

static RESTART_AFTER_EXIT: AtomicBool = AtomicBool::new(false);

pub fn restart_requested() -> bool {
    RESTART_AFTER_EXIT.load(Ordering::Relaxed)
}

#[derive(Clone, Serialize, Deserialize)]
pub struct UpdateConfig {
    pub auto_check: bool,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self { auto_check: true }
    }
}

#[derive(Default)]
enum State {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available {
        version: String,
        installable: bool,
    },
    Updating,
    Updated(String),
    Failed(String),
}

enum Msg {
    Checked(Result<Option<(String, bool)>, String>),
    Updated(Result<String, String>),
}

#[derive(Default)]
pub struct Updater {
    pub config: UpdateConfig,
    state: State,
    rx: Option<Receiver<Msg>>,
}

impl Updater {
    pub fn check(&mut self) {
        if self.is_busy() {
            return;
        }
        self.state = State::Checking;
        self.spawn(|| Msg::Checked(query()));
    }

    pub fn update(&mut self) {
        if self.is_busy() {
            return;
        }
        self.state = State::Updating;
        self.spawn(|| Msg::Updated(install()));
    }

    pub fn is_busy(&self) -> bool {
        matches!(self.state, State::Checking | State::Updating)
    }

    pub fn pump(&mut self) {
        let Some(rx) = &self.rx else {
            return;
        };
        match rx.try_recv() {
            Ok(Msg::Checked(Ok(Some((version, installable))))) => {
                self.state = State::Available {
                    version,
                    installable,
                };
            }
            Ok(Msg::Checked(Ok(None))) => self.state = State::UpToDate,
            Ok(Msg::Updated(Ok(version))) => self.state = State::Updated(version),
            Ok(Msg::Checked(Err(e)) | Msg::Updated(Err(e))) => self.state = State::Failed(e),
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                self.state = State::Failed("the updater thread stopped unexpectedly".to_string());
            }
        }
        self.rx = None;
    }

    pub fn banner(&mut self, ui: &mut egui::Ui) {
        match &self.state {
            State::Available {
                version,
                installable,
            } => {
                let version = version.clone();
                let installable = *installable;
                ui.horizontal(|ui| {
                    ui.label(format!("A new version is available: v{version}"));
                    if installable {
                        if ui.button("Update").clicked() {
                            self.update();
                        }
                    } else if ui.button("Open releases").clicked() {
                        open_releases();
                    }
                    if ui.button("Dismiss").clicked() {
                        self.state = State::Idle;
                    }
                });
                if !installable {
                    ui.weak(
                        "this build was not installed by an installer, so it cannot update itself",
                    );
                }
                ui.separator();
            }
            State::Updating => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("updating...");
                });
                ui.separator();
            }
            State::Updated(version) => {
                let version = version.clone();
                ui.horizontal(|ui| {
                    ui.label(format!("Updated to v{version}."));
                    if ui.button("Restart").clicked() {
                        RESTART_AFTER_EXIT.store(true, Ordering::Relaxed);
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.separator();
            }
            State::Failed(e) => {
                let e = e.clone();
                ui.horizontal(|ui| {
                    ui.colored_label(egui::Color32::LIGHT_RED, format!("update failed: {e}"));
                    if ui.button("Dismiss").clicked() {
                        self.state = State::Idle;
                    }
                });
                ui.separator();
            }
            State::Idle | State::Checking | State::UpToDate => {}
        }
    }

    pub fn settings_ui(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.config.auto_check, "Check for updates on startup");
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!self.is_busy(), egui::Button::new("Check now"))
                .clicked()
            {
                self.check();
            }
            match &self.state {
                State::Checking => {
                    ui.spinner();
                }
                State::UpToDate => {
                    ui.label("up to date");
                }
                State::Available { version, .. } => {
                    ui.label(format!("v{version} available"));
                }
                State::Failed(e) => {
                    ui.colored_label(egui::Color32::LIGHT_RED, e);
                }
                State::Idle | State::Updating | State::Updated(_) => {}
            }
        });
    }

    fn spawn(&mut self, task: impl FnOnce() -> Msg + Send + 'static) {
        let (tx, rx) = channel();
        self.rx = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(task());
        });
    }
}

fn prepare() -> Result<(AxoUpdater, bool), String> {
    let mut updater = AxoUpdater::new_for(APP_NAME);
    if updater.load_receipt().is_ok() {
        return Ok((updater, true));
    }

    let mut updater = AxoUpdater::new_for(APP_NAME);
    updater.set_release_source(ReleaseSource {
        release_type: ReleaseSourceType::GitHub,
        owner: REPO_OWNER.to_string(),
        name: REPO_NAME.to_string(),
        app_name: APP_NAME.to_string(),
    });
    let version = Version::parse(env!("CARGO_PKG_VERSION")).map_err(|e| e.to_string())?;
    updater
        .set_current_version(version)
        .map_err(|e| e.to_string())?;
    Ok((updater, false))
}

fn query() -> Result<Option<(String, bool)>, String> {
    let (mut updater, installable) = prepare()?;
    if !updater.is_update_needed_sync().map_err(|e| e.to_string())? {
        return Ok(None);
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let version = runtime
        .block_on(updater.query_new_version())
        .map_err(|e| e.to_string())?
        .map(|version| (version.to_string(), installable));
    Ok(version)
}

fn install() -> Result<String, String> {
    let (mut updater, installable) = prepare()?;
    if !installable {
        return Err("no install receipt found".to_string());
    }
    updater.disable_installer_output();
    match updater.run_sync().map_err(|e| e.to_string())? {
        Some(result) => Ok(result.new_version.to_string()),
        None => Err("no newer release was found".to_string()),
    }
}

fn open_releases() {
    let (program, args): (&str, &[&str]) = if cfg!(target_os = "macos") {
        ("open", &[RELEASES_URL])
    } else if cfg!(target_os = "windows") {
        ("cmd", &["/C", "start", "", RELEASES_URL])
    } else {
        ("xdg-open", &[RELEASES_URL])
    };
    let mut command = std::process::Command::new(program);
    command.args(args);
    crate::process::no_window(&mut command);
    let _ = command.spawn();
}
