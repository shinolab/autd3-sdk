use autd3_rs::MAX_DEVICES;
use eframe::egui;

use super::SimulatorApp;
use crate::emulator::DeviceState;
use crate::render::{RESOLUTION_MAX, RESOLUTION_MIN, SLICE_MAX_MM, SLICE_MIN_MM};
use crate::settings::Settings;

const AXIS_COLORS: [egui::Color32; 3] = [
    egui::Color32::from_rgb(0xf8, 0x72, 0x72),
    egui::Color32::from_rgb(0x4a, 0xde, 0x80),
    egui::Color32::from_rgb(0x60, 0xa5, 0xfa),
];

fn vec3_fields(
    ui: &mut egui::Ui,
    title: &str,
    labels: [&str; 3],
    values: [f32; 3],
    bounds: Option<[(f32, f32); 3]>,
    mut set: impl FnMut(usize, f32),
) {
    ui.vertical(|ui| {
        ui.label(egui::RichText::new(title).strong());
        for axis in 0..3 {
            ui.horizontal(|ui| {
                ui.colored_label(AXIS_COLORS[axis], labels[axis]);
                let mut value = values[axis];
                let mut drag = egui::DragValue::new(&mut value)
                    .speed(0.5)
                    .max_decimals(1)
                    .clamp_existing_to_range(false);
                if let Some(bounds) = bounds {
                    drag = drag.range(bounds[axis].0..=bounds[axis].1);
                }
                if ui.add(drag).changed() {
                    set(axis, value);
                }
            });
        }
    });
}

fn renderer_missing(ui: &mut egui::Ui) {
    ui.label("The renderer is unavailable.");
}

pub(super) fn geometry(app: &mut SimulatorApp, ui: &mut egui::Ui) {
    let count = app.drafts.len();
    let mut commit_poses = false;
    let mut next = None;
    for index in 0..count {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("Device {index}")).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(count > 1, egui::Button::new("Remove"))
                        .clicked()
                    {
                        let mut drafts = app.applied_drafts.clone();
                        drafts.remove(index);
                        next = Some(drafts);
                    }
                    if ui
                        .add_enabled(index + 1 < count, egui::Button::new("Down"))
                        .clicked()
                    {
                        let mut drafts = app.applied_drafts.clone();
                        drafts.swap(index, index + 1);
                        next = Some(drafts);
                    }
                    if ui.add_enabled(index > 0, egui::Button::new("Up")).clicked() {
                        let mut drafts = app.applied_drafts.clone();
                        drafts.swap(index - 1, index);
                        next = Some(drafts);
                    }
                });
            });
            let draft = &mut app.drafts[index];
            egui::Grid::new(("pose", index)).show(ui, |ui| {
                for (title, labels, values) in [
                    ("Origin (mm)", ["X", "Y", "Z"], &mut draft.origin),
                    (
                        "Rotation ZYZ (deg)",
                        ["Z", "Y", "Z"],
                        &mut draft.euler_zyz_deg,
                    ),
                ] {
                    ui.label(title);
                    for axis in 0..3 {
                        ui.colored_label(AXIS_COLORS[axis], labels[axis]);
                        let response = ui.add(
                            egui::DragValue::new(&mut values[axis])
                                .speed(0.5)
                                .max_decimals(3)
                                .update_while_editing(false),
                        );
                        if response.drag_stopped() || (response.changed() && !response.dragged()) {
                            commit_poses = true;
                        }
                    }
                    ui.end_row();
                }
            });
        });
    }
    ui.horizontal(|ui| {
        if ui
            .add_enabled(count < MAX_DEVICES, egui::Button::new("Add device"))
            .clicked()
        {
            let mut drafts = app.applied_drafts.clone();
            if let Some(last) = drafts.last() {
                drafts.push(last.next_to());
            }
            next = Some(drafts);
        }
        if ui.button("Save to file").clicked() {
            app.save_geometry();
        }
        if ui.button("Load from file").clicked() {
            app.load_geometry();
        }
    });
    if let Some(message) = &app.geometry_error {
        ui.colored_label(egui::Color32::LIGHT_RED, message);
    }
    if let Some(next) = next {
        app.commit(next);
    } else if commit_poses {
        app.commit(app.drafts.clone());
    }
}

pub(super) fn slice(app: &mut SimulatorApp, ui: &mut egui::Ui) {
    let settings = &mut app.settings;
    ui.horizontal(|ui| {
        ui.checkbox(&mut settings.gizmo_on, "Show gizmo");
        ui.selectable_value(&mut settings.gizmo_rotate, false, "Move");
        ui.selectable_value(&mut settings.gizmo_rotate, true, "Rotate");
        ui.separator();
        ui.label("Align");
        for (label, rot) in [
            ("XY", [90.0, 0.0, 0.0]),
            ("YZ", [0.0, 0.0, 90.0]),
            ("ZX", [0.0, 0.0, 0.0]),
        ] {
            if ui.button(label).clicked()
                && let Some(r) = &mut app.renderer
            {
                for (axis, v) in rot.into_iter().enumerate() {
                    r.set_slice_rot(axis, v);
                }
            }
        }
    });
    let Some(r) = &mut app.renderer else {
        renderer_missing(ui);
        return;
    };
    ui.horizontal_top(|ui| {
        let (center, bounds) = (r.slice_center(), r.axis_bounds());
        vec3_fields(
            ui,
            "Position (mm)",
            ["X", "Y", "Z"],
            center,
            Some(bounds),
            |axis, v| {
                r.set_slice_coord(axis, v);
            },
        );
        ui.add_space(24.0);
        let rot = r.slice_rot();
        vec3_fields(
            ui,
            "Rotation (deg)",
            ["RX", "RY", "RZ"],
            rot,
            Some([(-180.0, 180.0); 3]),
            |axis, v| r.set_slice_rot(axis, v),
        );
    });
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Size (mm)").strong());
        let size = r.slice_size();
        for (axis, label) in ["Width", "Height"].into_iter().enumerate() {
            ui.label(label);
            let mut value = size[axis];
            if ui
                .add(
                    egui::DragValue::new(&mut value)
                        .range(SLICE_MIN_MM..=SLICE_MAX_MM)
                        .max_decimals(0),
                )
                .changed()
            {
                r.set_slice_size(axis, value);
            }
        }
        ui.separator();
        ui.label(egui::RichText::new("Texels per mm").strong());
        ui.add(
            egui::DragValue::new(&mut settings.slice_resolution)
                .range(RESOLUTION_MIN..=RESOLUTION_MAX)
                .speed(0.05)
                .max_decimals(2),
        );
    });
    ui.horizontal(|ui| {
        ui.add(
            egui::Slider::new(&mut settings.max_pressure, 500.0..=20000.0)
                .step_by(100.0)
                .text("Max pressure (Pa)"),
        );
        egui::ComboBox::from_label("Color map")
            .selected_text(if settings.colormap == 1 {
                "Viridis"
            } else {
                "Inferno"
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut settings.colormap, 0, "Inferno");
                ui.selectable_value(&mut settings.colormap, 1, "Viridis");
            });
    });
}

pub(super) fn camera(app: &mut SimulatorApp, ui: &mut egui::Ui) {
    let settings = &mut app.settings;
    let Some(r) = &mut app.renderer else {
        renderer_missing(ui);
        return;
    };
    ui.horizontal(|ui| {
        if ui.button("Reset view").clicked() {
            r.reset_camera();
        }
        ui.weak("Drag: orbit / Shift+drag: pan / Ctrl+drag, wheel: zoom");
    });
    ui.horizontal(|ui| {
        ui.label("FoV (deg)");
        ui.add(egui::DragValue::new(&mut settings.fov).range(5.0..=150.0));
        ui.label("Near (mm)");
        ui.add(
            egui::DragValue::new(&mut settings.near)
                .range(0.01..=100_000.0)
                .speed(0.5),
        );
        ui.label("Far (mm)");
        ui.add(
            egui::DragValue::new(&mut settings.far)
                .range(0.0..=100_000.0)
                .speed(100.0),
        );
        ui.label("Move speed");
        ui.add(
            egui::DragValue::new(&mut settings.move_speed)
                .range(0.1..=1000.0)
                .speed(0.1),
        );
    });
    ui.horizontal_top(|ui| {
        let pos = r.camera_pos();
        vec3_fields(
            ui,
            "Position (mm)",
            ["X", "Y", "Z"],
            pos,
            None,
            |axis, v| {
                r.set_camera_pos(axis, v);
            },
        );
        ui.add_space(24.0);
        let rot = r.camera_rot();
        vec3_fields(
            ui,
            "Rotation (deg)",
            ["RX", "RY", "RZ"],
            rot,
            Some([(-180.0, 180.0); 3]),
            |axis, v| r.set_camera_rot(axis, v),
        );
    });
}

pub(super) fn environment(app: &mut SimulatorApp, ui: &mut egui::Ui) {
    ui.add(
        egui::Slider::new(&mut app.settings.sound_speed, 300_000.0..=360_000.0)
            .step_by(500.0)
            .text("Sound speed (mm/s)"),
    );
}

pub(super) fn settings(settings: &mut Settings, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label("Background color");
        let mut srgb = settings
            .bg
            .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
        if ui.color_edit_button_srgb(&mut srgb).changed() {
            settings.bg = srgb.map(|v| f32::from(v) / 255.0);
        }
    });
    ui.horizontal(|ui| {
        ui.label("Animation");
        let label = if settings.playing { "Pause" } else { "Play" };
        if ui.button(label).clicked() {
            settings.playing = !settings.playing;
        }
    });
    ui.checkbox(&mut settings.show_markers, "Show devices");
    ui.checkbox(&mut settings.mod_enabled, "Apply modulation");
    if ui.button("Reset to defaults").clicked() {
        *settings = Settings::default();
    }
}

fn waveform(ui: &mut egui::Ui, values: &[u8], y: impl Fn(u8) -> f32, color: egui::Color32) {
    if values.len() < 2 {
        return;
    }
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 60.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
    let denom = (values.len() - 1) as f32;
    let points = values
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            egui::pos2(
                rect.left() + i as f32 / denom * rect.width(),
                rect.top() + y(v) / 60.0 * rect.height(),
            )
        })
        .collect();
    painter.add(egui::Shape::line(points, egui::Stroke::new(1.5, color)));
}

pub(super) fn state(device_states: &[DeviceState], ui: &mut egui::Ui) {
    fn kv(ui: &mut egui::Ui, k: &str, v: &dyn std::fmt::Display) {
        ui.label(k);
        ui.monospace(v.to_string());
        ui.end_row();
    }

    if device_states.is_empty() {
        ui.label("No emulated device.");
        return;
    }
    for (idx, dev) in device_states.iter().enumerate() {
        egui::CollapsingHeader::new(format!(
            "Device {idx} · {} transducers",
            dev.num_transducers
        ))
        .id_salt(idx)
        .default_open(true)
        .show(ui, |ui| {
            ui.strong("Silencer");
            egui::Grid::new(("silencer", idx)).show(ui, |ui| {
                kv(
                    ui,
                    "Mode",
                    &if dev.silencer_fixed_update_rate {
                        "Fixed update rate"
                    } else {
                        "Completion steps"
                    },
                );
                kv(ui, "Intensity", &dev.silencer_intensity);
                kv(ui, "Phase", &dev.silencer_phase);
            });
            ui.strong("Modulation");
            egui::Grid::new(("modulation", idx)).show(ui, |ui| {
                kv(ui, "Freq division", &dev.mod_freq_div);
                kv(ui, "Cycle (samples)", &dev.mod_cycle);
                kv(ui, "Index", &dev.mod_idx);
            });
            waveform(
                ui,
                &dev.mod_buffer,
                |v| 59.0 - (f32::from(v) / 255.0) * 58.0,
                ui.visuals().selection.bg_fill,
            );
            ui.strong("STM");
            if dev.stm_cycle <= 1 {
                ui.label("Static (single pattern)");
            } else {
                egui::Grid::new(("stm", idx)).show(ui, |ui| {
                    kv(ui, "Freq division", &dev.stm_freq_div);
                    kv(ui, "Cycle (patterns)", &dev.stm_cycle);
                    kv(ui, "Index", &dev.stm_idx);
                });
            }
            ui.strong("GPIO out");
            for i in 0..4 {
                ui.horizontal(|ui| {
                    ui.label(format!("GPIO {i}"));
                    ui.monospace(&dev.gpio_types[i]);
                });
                waveform(
                    ui,
                    &dev.gpio_out[i],
                    |v| 55.0 - f32::from(v.min(1)) * 50.0,
                    ui.visuals().warn_fg_color,
                );
            }
        });
    }
}

pub(super) fn about(ui: &mut egui::Ui) {
    const LICENSE: &str = include_str!(concat!(env!("OUT_DIR"), "/license.txt"));
    const THIRD_PARTY: &str = include_str!(concat!(env!("OUT_DIR"), "/third-party.md"));

    ui.heading("AUTD3 Simulator");
    ui.label(format!("version {}", env!("CARGO_PKG_VERSION")));
    ui.hyperlink("https://github.com/shinolab/autd3-sdk");
    ui.separator();
    ui.strong("License (MIT)");
    ui.monospace(LICENSE);
    ui.separator();
    ui.strong("Third-party licenses");
    ui.monospace(THIRD_PARTY);
}
