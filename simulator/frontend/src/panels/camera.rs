use dioxus::prelude::*;

use super::common::{PlainNum, Vec3Fields, scalar_handler};
use crate::context::{Ctx, Field, SharedRenderer};
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
                        Vec3Fields { title: "Position (mm)", field: Field::CameraPos, values: cam_pos(), step: "1" }
                        Vec3Fields { title: "Rotation (deg)", field: Field::CameraRot, values: cam_rot(), bounds: [(-180.0, 180.0); 3], step: "1" }
                    }
                }
            }
        }
    }
}
