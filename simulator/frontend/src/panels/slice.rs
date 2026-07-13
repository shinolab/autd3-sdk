use dioxus::prelude::*;

use super::common::{NumField, PlainNum};
use crate::context::{Ctx, SharedRenderer};
use crate::render::{RESOLUTION_MAX, RESOLUTION_MIN, SLICE_MAX_MM, SLICE_MIN_MM};

#[component]
pub fn SlicePanel() -> Element {
    let ctx = use_context::<Ctx>();
    let renderer = ctx.renderer.clone();
    let mut gizmo_on = ctx.gizmo_on;
    let mut gizmo_rotate = ctx.gizmo_rotate;
    let mut max_pressure = ctx.max_pressure;
    let mut colormap = ctx.colormap;
    let slice_center = ctx.slice_center;
    let slice_rot = ctx.slice_rot;
    let slice_bounds = ctx.slice_bounds;
    let mut slice_size = ctx.slice_size;
    let mut slice_res = ctx.slice_res;
    let mut field_dims = ctx.field_dims;

    let size_handler = |renderer: SharedRenderer, axis: usize| {
        move |e: Event<FormData>| {
            if let Ok(v) = e.parsed::<f32>()
                && let Some(r) = renderer.borrow_mut().as_mut()
            {
                r.set_slice_size(axis, v);
                slice_size.set(r.slice_size());
                field_dims.set(r.field_dims());
            }
        }
    };
    let on_slice_w = size_handler(renderer.clone(), 0);
    let on_slice_h = size_handler(renderer.clone(), 1);
    let on_slice_res = {
        let renderer = renderer.clone();
        move |e: Event<FormData>| {
            if let Ok(v) = e.parsed::<f32>() {
                slice_res.set(v.clamp(RESOLUTION_MIN, RESOLUTION_MAX));
                if let Some(r) = renderer.borrow_mut().as_mut() {
                    r.set_slice_resolution(v);
                    field_dims.set(r.field_dims());
                }
            }
        }
    };

    let on_slice_x = ctx.field_handler(0, 0);
    let on_slice_y = ctx.field_handler(0, 1);
    let on_slice_z = ctx.field_handler(0, 2);
    let on_slice_rx = ctx.field_handler(1, 0);
    let on_slice_ry = ctx.field_handler(1, 1);
    let on_slice_rz = ctx.field_handler(1, 2);
    let on_num_down_x = ctx.num_down(0, 0);
    let on_num_down_y = ctx.num_down(0, 1);
    let on_num_down_z = ctx.num_down(0, 2);
    let on_num_down_rx = ctx.num_down(1, 0);
    let on_num_down_ry = ctx.num_down(1, 1);
    let on_num_down_rz = ctx.num_down(1, 2);
    let on_max_pressure = {
        let renderer = renderer.clone();
        move |e: Event<FormData>| {
            if let Ok(v) = e.parsed::<f32>() {
                max_pressure.set(v);
                if let Some(r) = renderer.borrow_mut().as_mut() {
                    r.set_max_pressure(v);
                }
            }
        }
    };
    let align_plane = {
        let ctx = ctx.clone();
        move |rot: [f32; 3]| {
            for (axis, v) in rot.into_iter().enumerate() {
                ctx.apply_field(1, axis, v);
            }
        }
    };
    let align_xy = {
        let align_plane = align_plane.clone();
        move |_: Event<MouseData>| align_plane([90.0, 0.0, 0.0])
    };
    let align_yz = {
        let align_plane = align_plane.clone();
        move |_: Event<MouseData>| align_plane([0.0, 0.0, 90.0])
    };
    let align_zx = move |_: Event<MouseData>| align_plane([0.0, 0.0, 0.0]);
    let on_colormap = {
        let renderer = renderer.clone();
        move |e: Event<FormData>| {
            let idx: u8 = u8::from(e.value() == "viridis");
            colormap.set(idx);
            if let Some(r) = renderer.borrow_mut().as_mut() {
                r.set_colormap(u32::from(idx));
            }
        }
    };

    let [cx, cy, cz] = slice_center();
    let [rx, ry, rz] = slice_rot();
    let [(x_lo, x_hi), (y_lo, y_hi), (z_lo, z_hi)] = slice_bounds();
    let [sw, sh] = slice_size();
    let [fw, fh] = field_dims();
    let pressure_label = format!("Max pressure: {:.0} Pa", max_pressure());
    let field_label = format!("Field texture: {fw} x {fh} px");

    rsx! {
        div { class: "px-6 pt-4",
            div { class: "card bg-base-100 shadow",
                div { class: "card-body gap-4",
                    div { class: "flex flex-wrap items-center gap-4",
                        label { class: "label cursor-pointer justify-start gap-3",
                            input {
                                r#type: "checkbox",
                                class: "toggle toggle-primary",
                                checked: gizmo_on(),
                                onchange: move |e: Event<FormData>| gizmo_on.set(e.checked()),
                            }
                            span { class: "label-text", "Show gizmo" }
                        }
                        div { class: "join",
                            button {
                                class: if gizmo_rotate() { "btn btn-sm join-item" } else { "btn btn-sm btn-primary join-item" },
                                onclick: move |_| gizmo_rotate.set(false),
                                "Move"
                            }
                            button {
                                class: if gizmo_rotate() { "btn btn-sm btn-primary join-item" } else { "btn btn-sm join-item" },
                                onclick: move |_| gizmo_rotate.set(true),
                                "Rotate"
                            }
                        }
                        div { class: "flex items-center gap-2",
                            span { class: "text-sm opacity-70", "Align" }
                            div { class: "join",
                                button {
                                    class: "btn btn-sm join-item",
                                    onclick: align_xy,
                                    "XY"
                                }
                                button {
                                    class: "btn btn-sm join-item",
                                    onclick: align_yz,
                                    "YZ"
                                }
                                button {
                                    class: "btn btn-sm join-item",
                                    onclick: align_zx,
                                    "ZX"
                                }
                            }
                        }
                    }
                    div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                        div { class: "flex flex-col gap-3",
                            div { class: "text-sm font-semibold opacity-70", "Position (mm)" }
                            NumField { label: "X", accent: "text-error", min: x_lo, max: x_hi, step: "0.5", value: format!("{cx:.1}"), onchange: on_slice_x, onmousedown: on_num_down_x }
                            NumField { label: "Y", accent: "text-success", min: y_lo, max: y_hi, step: "0.5", value: format!("{cy:.1}"), onchange: on_slice_y, onmousedown: on_num_down_y }
                            NumField { label: "Z", accent: "text-info", min: z_lo, max: z_hi, step: "0.5", value: format!("{cz:.1}"), onchange: on_slice_z, onmousedown: on_num_down_z }
                        }
                        div { class: "flex flex-col gap-3",
                            div { class: "text-sm font-semibold opacity-70", "Rotation (deg)" }
                            NumField { label: "RX", accent: "text-error", min: -180.0, max: 180.0, step: "1", value: format!("{rx:.1}"), onchange: on_slice_rx, onmousedown: on_num_down_rx }
                            NumField { label: "RY", accent: "text-success", min: -180.0, max: 180.0, step: "1", value: format!("{ry:.1}"), onchange: on_slice_ry, onmousedown: on_num_down_ry }
                            NumField { label: "RZ", accent: "text-info", min: -180.0, max: 180.0, step: "1", value: format!("{rz:.1}"), onchange: on_slice_rz, onmousedown: on_num_down_rz }
                        }
                    }
                    div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                        div { class: "flex flex-col gap-3",
                            div { class: "text-sm font-semibold opacity-70", "Size (mm)" }
                            div { class: "grid grid-cols-2 gap-3",
                                PlainNum { label: "Width", min: SLICE_MIN_MM, max: SLICE_MAX_MM, step: "1", value: format!("{sw:.0}"), onchange: on_slice_w }
                                PlainNum { label: "Height", min: SLICE_MIN_MM, max: SLICE_MAX_MM, step: "1", value: format!("{sh:.0}"), onchange: on_slice_h }
                            }
                        }
                        div { class: "flex flex-col gap-3",
                            div { class: "text-sm font-semibold opacity-70", "Resolution" }
                            PlainNum { label: "Texels per mm", min: RESOLUTION_MIN, max: RESOLUTION_MAX, step: "0.25", value: format!("{:.2}", slice_res()), onchange: on_slice_res }
                            div { class: "text-xs opacity-60", "{field_label}" }
                        }
                    }
                    div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
                        div {
                            label { class: "label py-1",
                                span { class: "label-text", "{pressure_label}" }
                            }
                            input {
                                r#type: "range",
                                class: "range range-primary range-sm",
                                min: "500",
                                max: "20000",
                                step: "100",
                                value: "{max_pressure}",
                                oninput: on_max_pressure,
                            }
                        }
                        div {
                            label { class: "label py-1",
                                span { class: "label-text", "Color map" }
                            }
                            select {
                                class: "select select-bordered select-sm",
                                value: if colormap() == 1 { "viridis" } else { "inferno" },
                                onchange: on_colormap,
                                option { value: "inferno", "Inferno" }
                                option { value: "viridis", "Viridis" }
                            }
                        }
                    }
                }
            }
        }
    }
}
