mod panels;

use std::time::Duration;

use autd3_rs::geometry::Geometry;
use eframe::egui;

use crate::emulator::{
    DeviceState, extract_device_states, extract_states, geometry_from_poses, transducer_buffers,
};
use crate::geometry::{DeviceDraft, to_poses};
use crate::render::{GizmoMode, Renderer};
use crate::scene::Scene;
use crate::settings::Settings;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Home,
    Geometry,
    Slice,
    Camera,
    Environment,
    State,
    Settings,
    About,
}

impl Tab {
    const ALL: [Tab; 8] = [
        Tab::Home,
        Tab::Geometry,
        Tab::Slice,
        Tab::Camera,
        Tab::Environment,
        Tab::State,
        Tab::Settings,
        Tab::About,
    ];

    fn label(self) -> &'static str {
        match self {
            Tab::Home => "Home",
            Tab::Geometry => "Geometry",
            Tab::Slice => "Slice",
            Tab::Camera => "Camera",
            Tab::Environment => "Environment",
            Tab::State => "State",
            Tab::Settings => "Settings",
            Tab::About => "About",
        }
    }
}

const SETTINGS_KEY: &str = "settings";

pub(crate) struct SimulatorApp {
    scene: Scene,
    renderer: Option<Renderer>,
    settings: Settings,
    applied_settings: Option<Settings>,
    tab: Tab,
    drafts: Vec<DeviceDraft>,
    applied_drafts: Vec<DeviceDraft>,
    geometry_error: Option<String>,
    device_states: Vec<DeviceState>,
    gizmo_dragging: bool,
}

impl SimulatorApp {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>, scene: Scene, geometry: &Geometry) -> Self {
        let settings = cc
            .storage
            .and_then(|storage| eframe::get_value(storage, SETTINGS_KEY))
            .unwrap_or_default();
        let renderer = cc
            .wgpu_render_state
            .as_ref()
            .map(|render_state| Renderer::new(render_state, &settings));
        let drafts: Vec<DeviceDraft> = crate::emulator::device_poses(geometry)
            .iter()
            .map(DeviceDraft::from_pose)
            .collect();
        let mut app = Self {
            scene,
            renderer,
            settings,
            applied_settings: None,
            tab: Tab::Home,
            drafts: drafts.clone(),
            applied_drafts: drafts,
            geometry_error: None,
            device_states: Vec::new(),
            gizmo_dragging: false,
        };
        app.show_geometry(geometry);
        app
    }

    fn show_geometry(&mut self, geometry: &Geometry) {
        let (positions, directions) = transducer_buffers(geometry);
        if let Some(r) = &mut self.renderer {
            r.set_geometry(&positions, &directions);
        }
    }

    fn commit(&mut self, next: Vec<DeviceDraft>) {
        match geometry_from_poses(&to_poses(&next)) {
            Ok(geometry) => self.apply_geometry(next, &geometry),
            Err(e) => {
                self.geometry_error = Some(e.to_string());
                self.drafts.clone_from(&self.applied_drafts);
            }
        }
    }

    fn apply_geometry(&mut self, next: Vec<DeviceDraft>, geometry: &Geometry) {
        match self.scene.resize(geometry.num_devices()) {
            Ok(()) => {
                tracing::info!(
                    "the GUI set the geometry: {} emulated device(s)",
                    geometry.num_devices()
                );
                self.geometry_error = None;
                self.drafts.clone_from(&next);
                self.applied_drafts = next;
                self.show_geometry(geometry);
            }
            Err(e) => {
                self.geometry_error = Some(format!("failed to rebuild the emulated devices: {e}"));
                self.drafts.clone_from(&self.applied_drafts);
            }
        }
    }

    fn save_geometry(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("JSON", &["json"])
            .set_file_name("geometry.json")
            .save_file()
        else {
            return;
        };
        self.geometry_error = crate::geometry::to_file(&self.applied_drafts)
            .and_then(|content| {
                std::fs::write(&path, content)
                    .map_err(|e| format!("could not save {}: {e}", path.display()))
            })
            .err();
    }

    fn load_geometry(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("JSON", &["json"])
            .pick_file()
        else {
            return;
        };
        let loaded = std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| crate::geometry::from_file(&text));
        match loaded {
            Ok((next, geometry)) => self.apply_geometry(next, &geometry),
            Err(message) => self.geometry_error = Some(format!("{}: {message}", path.display())),
        }
    }

    fn viewport(&mut self, ui: &mut egui::Ui) {
        let Some(r) = &mut self.renderer else {
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                "The wgpu renderer is unavailable; the sound field cannot be drawn.",
            );
            return;
        };
        let (rect, response) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        let ndc = |pos: egui::Pos2| {
            glam::Vec2::new(
                (pos.x - rect.left()) / rect.width().max(1.0) * 2.0 - 1.0,
                1.0 - (pos.y - rect.top()) / rect.height().max(1.0) * 2.0,
            )
        };

        if response.drag_started()
            && let Some(pos) = ui.input(|i| i.pointer.press_origin())
            && let Some(axis) = r.pick_gizmo_axis(ndc(pos))
        {
            r.begin_gizmo_drag(axis, ndc(pos));
            self.gizmo_dragging = true;
        }
        if response.dragged() {
            if self.gizmo_dragging {
                if let Some(pos) = response.interact_pointer_pos() {
                    let snap = ui.input(|i| i.modifiers.shift);
                    r.update_gizmo_drag(ndc(pos), snap);
                }
            } else {
                let delta = response.drag_delta();
                r.orbit(delta.x, delta.y);
            }
        } else if let Some(pos) = response.hover_pos() {
            r.set_hover(ndc(pos));
        }
        if response.drag_stopped() {
            r.end_gizmo_drag();
            self.gizmo_dragging = false;
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                r.zoom(-scroll);
            }
        }

        let ppp = ui.ctx().pixels_per_point();
        r.resize(
            (rect.width() * ppp).round() as u32,
            (rect.height() * ppp).round() as u32,
        );
        r.set_gizmo_visible(self.tab == Tab::Slice && self.settings.gizmo_on);
        r.render();
        ui.painter().image(
            r.texture_id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }
}

impl eframe::App for SimulatorApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(r) = &mut self.renderer {
            if self.applied_settings != Some(self.settings) {
                r.apply_settings(&self.settings);
                r.set_gizmo_mode(if self.settings.gizmo_rotate {
                    GizmoMode::Rotate
                } else {
                    GizmoMode::Move
                });
                self.applied_settings = Some(self.settings);
            }
            if self.settings.playing {
                let mod_enabled = self.settings.mod_enabled;
                if let Some(states) = self.scene.sample(|devices| {
                    extract_states(devices, mod_enabled)
                        .iter()
                        .map(|s| [s.amp, s.phase, f32::from(u8::from(s.enable)), 0.0])
                        .collect::<Vec<_>>()
                }) {
                    r.set_states(&states);
                }
            }
        }
        if self.tab == Tab::State {
            self.device_states = self.scene.sample(extract_device_states).unwrap_or_default();
        }
        ctx.request_repaint_after(Duration::from_millis(33));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                for tab in Tab::ALL {
                    ui.selectable_value(&mut self.tab, tab, tab.label());
                }
            });
        });
        if self.tab != Tab::Home {
            egui::Panel::left("panel")
                .resizable(true)
                .default_size(420.0)
                .size_range(280.0..=800.0)
                .frame(egui::Frame::side_top_panel(ui.style()).inner_margin(14))
                .show(ui, |ui| {
                    let spacing = ui.spacing_mut();
                    spacing.item_spacing = egui::vec2(10.0, 8.0);
                    spacing.button_padding = egui::vec2(10.0, 4.0);
                    spacing.interact_size.y = 24.0;
                    egui::ScrollArea::vertical().show(ui, |ui| match self.tab {
                        Tab::Geometry => panels::geometry(self, ui),
                        Tab::Slice => panels::slice(self, ui),
                        Tab::Camera => panels::camera(self, ui),
                        Tab::Environment => panels::environment(self, ui),
                        Tab::State => panels::state(&self.device_states, ui),
                        Tab::Settings => panels::settings(&mut self.settings, ui),
                        Tab::About => panels::about(ui),
                        Tab::Home => {}
                    });
                });
        }
        egui::CentralPanel::default().show(ui, |ui| self.viewport(ui));
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, SETTINGS_KEY, &self.settings);
    }
}
