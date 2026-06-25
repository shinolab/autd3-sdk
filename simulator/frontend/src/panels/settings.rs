use dioxus::prelude::*;

use autd3_rs_simulator_protocol::ClientMsg;

use crate::context::Ctx;
use crate::settings::{self, Settings};

#[component]
pub fn SettingsPanel() -> Element {
    let ctx = use_context::<Ctx>();
    let renderer = ctx.renderer.clone();
    let control_tx = ctx.control_tx.clone();
    let mut bg = ctx.bg;
    let mut playing = ctx.playing;
    let mut show_markers = ctx.show_markers;
    let mut mod_enabled = ctx.mod_enabled;

    let on_bg = {
        let renderer = renderer.clone();
        move |e: Event<FormData>| {
            if let Some(rgb) = settings::hex_to_rgb(&e.value()) {
                bg.set(rgb);
                if let Some(r) = renderer.borrow_mut().as_mut() {
                    r.set_background(rgb);
                }
            }
        }
    };
    let on_show = {
        let renderer = renderer.clone();
        move |e: Event<FormData>| {
            let b = e.checked();
            show_markers.set(b);
            if let Some(r) = renderer.borrow_mut().as_mut() {
                r.set_show_markers(b);
            }
        }
    };
    let on_mod = {
        let control_tx = control_tx.clone();
        move |e: Event<FormData>| {
            let enabled = e.checked();
            mod_enabled.set(enabled);
            let _ = control_tx.unbounded_send(ClientMsg::SetModulationEnabled { enabled });
        }
    };
    let on_reset_settings = {
        let ctx = ctx.clone();
        let control_tx = control_tx.clone();
        move |_| {
            let d = Settings::default();
            let (
                mut max_pressure,
                mut show_markers,
                mut sound_speed,
                mut playing,
                mut mod_enabled,
                mut colormap,
                mut bg,
            ) = (
                ctx.max_pressure,
                ctx.show_markers,
                ctx.sound_speed,
                ctx.playing,
                ctx.mod_enabled,
                ctx.colormap,
                ctx.bg,
            );
            let (
                mut gizmo_on,
                mut gizmo_rotate,
                mut cam_free,
                mut fov,
                mut near,
                mut far,
                mut move_speed,
            ) = (
                ctx.gizmo_on,
                ctx.gizmo_rotate,
                ctx.cam_free,
                ctx.fov,
                ctx.near,
                ctx.far,
                ctx.move_speed,
            );
            max_pressure.set(d.max_pressure);
            show_markers.set(d.show_markers);
            sound_speed.set(d.sound_speed);
            playing.set(d.playing);
            mod_enabled.set(d.mod_enabled);
            colormap.set(d.colormap);
            bg.set(d.bg);
            gizmo_on.set(d.gizmo_on);
            gizmo_rotate.set(d.gizmo_rotate);
            cam_free.set(d.cam_free);
            fov.set(d.fov);
            near.set(d.near);
            far.set(d.far);
            move_speed.set(d.move_speed);
            ctx.with_renderer(|r| {
                r.set_max_pressure(d.max_pressure);
                r.set_sound_speed(d.sound_speed);
                r.set_show_markers(d.show_markers);
                r.set_colormap(u32::from(d.colormap));
                r.set_background(d.bg);
                r.set_camera_free(d.cam_free);
                r.set_fov(d.fov);
                r.set_near(d.near);
                r.set_far(d.far);
                r.set_move_speed(d.move_speed);
            });
            let _ = control_tx.unbounded_send(ClientMsg::SetModulationEnabled {
                enabled: d.mod_enabled,
            });
        }
    };

    let bg_hex = settings::rgb_to_hex(bg());

    rsx! {
        div { class: "px-6 pt-4",
            div { class: "card bg-base-100 shadow",
                div { class: "card-body gap-4",
                    div { class: "flex items-center gap-4",
                        label { class: "label-text font-medium", "Background color" }
                        input {
                            r#type: "color",
                            class: "h-9 w-16 rounded border border-base-300 bg-base-100",
                            value: "{bg_hex}",
                            oninput: on_bg,
                        }
                        span { class: "text-sm opacity-70", "{bg_hex}" }
                    }
                    div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
                        div { class: "form-control",
                            label { class: "label py-1",
                                span { class: "label-text", "Animation" }
                            }
                            button {
                                class: if playing() { "btn btn-sm btn-primary w-fit" } else { "btn btn-sm w-fit" },
                                onclick: move |_| {
                                    let p = !playing();
                                    playing.set(p);
                                },
                                if playing() { "Pause" } else { "Play" }
                            }
                        }
                        div { class: "form-control",
                            label { class: "label cursor-pointer justify-start gap-3",
                                input {
                                    r#type: "checkbox",
                                    class: "toggle toggle-primary",
                                    checked: show_markers(),
                                    onchange: on_show,
                                }
                                span { class: "label-text", "Show devices" }
                            }
                        }
                        div { class: "form-control",
                            label { class: "label cursor-pointer justify-start gap-3",
                                input {
                                    r#type: "checkbox",
                                    class: "toggle toggle-primary",
                                    checked: mod_enabled(),
                                    onchange: on_mod,
                                }
                                span { class: "label-text", "Apply modulation" }
                            }
                        }
                    }
                    button {
                        class: "btn btn-sm btn-outline w-fit",
                        onclick: on_reset_settings,
                        "Reset to defaults"
                    }
                }
            }
        }
    }
}
