use dioxus::prelude::*;

use super::common::{NumField, PlainNum, scalar_handler};
use crate::context::{Ctx, SharedRenderer};
use crate::render::Renderer;

#[component]
pub fn CameraPanel() -> Element {
    let ctx = use_context::<Ctx>();
    let renderer = ctx.renderer.clone();
    let mut cam_free = ctx.cam_free;
    let mut cam_pos = ctx.cam_pos;
    let mut cam_rot = ctx.cam_rot;
    let fov = ctx.fov;
    let near = ctx.near;
    let far = ctx.far;
    let move_speed = ctx.move_speed;

    let on_reset_view = {
        let renderer = renderer.clone();
        move |_| {
            if let Some(r) = renderer.borrow_mut().as_mut() {
                r.reset_camera();
                cam_pos.set(r.camera_pos());
                cam_rot.set(r.camera_rot());
            }
        }
    };
    let cam_mode_btn = |free: bool, renderer: SharedRenderer| {
        move |_: Event<MouseData>| {
            cam_free.set(free);
            if let Some(r) = renderer.borrow_mut().as_mut() {
                r.set_camera_free(free);
                cam_rot.set(r.camera_rot());
            }
        }
    };
    let on_cam_free = cam_mode_btn(true, renderer.clone());
    let on_cam_orbit = cam_mode_btn(false, renderer.clone());
    let on_fov = scalar_handler(renderer.clone(), fov, Renderer::set_fov);
    let on_near = scalar_handler(renderer.clone(), near, Renderer::set_near);
    let on_far = scalar_handler(renderer.clone(), far, Renderer::set_far);
    let on_move_speed = scalar_handler(renderer.clone(), move_speed, Renderer::set_move_speed);
    let on_cam_x = ctx.field_handler(2, 0);
    let on_cam_y = ctx.field_handler(2, 1);
    let on_cam_z = ctx.field_handler(2, 2);
    let on_cam_rx = ctx.field_handler(3, 0);
    let on_cam_ry = ctx.field_handler(3, 1);
    let on_cam_rz = ctx.field_handler(3, 2);
    let on_num_down_cx = ctx.num_down(2, 0);
    let on_num_down_cy = ctx.num_down(2, 1);
    let on_num_down_cz = ctx.num_down(2, 2);
    let on_num_down_crx = ctx.num_down(3, 0);
    let on_num_down_cry = ctx.num_down(3, 1);
    let on_num_down_crz = ctx.num_down(3, 2);

    let [px, py, pz] = cam_pos();
    let [crx, cry, crz] = cam_rot();
    let (fov_v, near_v, far_v, move_speed_v) = (fov(), near(), far(), move_speed());

    rsx! {
        div { class: "px-6 pt-4",
            div { class: "card bg-base-100 shadow",
                div { class: "card-body gap-4",
                    div { class: "flex flex-wrap items-center gap-4",
                        button { class: "btn btn-sm w-fit", onclick: on_reset_view, "Reset view" }
                        div { class: "join",
                            button {
                                class: if cam_free() { "btn btn-sm btn-primary join-item" } else { "btn btn-sm join-item" },
                                onclick: on_cam_free,
                                "Free"
                            }
                            button {
                                class: if cam_free() { "btn btn-sm join-item" } else { "btn btn-sm btn-primary join-item" },
                                onclick: on_cam_orbit,
                                "Orbit"
                            }
                        }
                    }
                    div { class: "grid grid-cols-2 gap-4 sm:grid-cols-4",
                        PlainNum { label: "FoV (deg)", min: 5.0, max: 150.0, step: "1", value: format!("{fov_v:.0}"), onchange: on_fov }
                        PlainNum { label: "Near (mm)", min: 0.01, max: 100_000.0, step: "0.5", value: format!("{near_v:.2}"), onchange: on_near }
                        PlainNum { label: "Far (mm)", min: 0.0, max: 100_000.0, step: "100", value: format!("{far_v:.0}"), onchange: on_far }
                        PlainNum { label: "Move speed", min: 0.1, max: 1000.0, step: "0.1", value: format!("{move_speed_v:.1}"), onchange: on_move_speed }
                    }
                    div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                        div { class: "flex flex-col gap-3",
                            div { class: "text-sm font-semibold opacity-70", "Position (mm)" }
                            NumField { label: "X", accent: "text-error", step: "1", value: format!("{px:.1}"), onchange: on_cam_x, onmousedown: on_num_down_cx }
                            NumField { label: "Y", accent: "text-success", step: "1", value: format!("{py:.1}"), onchange: on_cam_y, onmousedown: on_num_down_cy }
                            NumField { label: "Z", accent: "text-info", step: "1", value: format!("{pz:.1}"), onchange: on_cam_z, onmousedown: on_num_down_cz }
                        }
                        div { class: "flex flex-col gap-3",
                            div { class: "text-sm font-semibold opacity-70", "Rotation (deg)" }
                            NumField { label: "RX", accent: "text-error", min: -180.0, max: 180.0, step: "1", value: format!("{crx:.1}"), onchange: on_cam_rx, onmousedown: on_num_down_crx }
                            NumField { label: "RY", accent: "text-success", min: -180.0, max: 180.0, step: "1", value: format!("{cry:.1}"), onchange: on_cam_ry, onmousedown: on_num_down_cry }
                            NumField { label: "RZ", accent: "text-info", min: -180.0, max: 180.0, step: "1", value: format!("{crz:.1}"), onchange: on_cam_rz, onmousedown: on_num_down_crz }
                        }
                    }
                }
            }
        }
    }
}
