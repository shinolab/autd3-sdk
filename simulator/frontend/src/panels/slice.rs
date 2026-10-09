use dioxus::prelude::*;

use super::common::{PlainNum, Vec3Fields, scalar_handler};
use crate::context::{Ctx, Field, SharedRenderer};
use crate::render::{RESOLUTION_MAX, RESOLUTION_MIN, Renderer, SLICE_MAX_MM, SLICE_MIN_MM};

#[component]
pub fn SlicePanel() -> Element {
    let ctx = use_context::<Ctx>();
    let renderer = ctx.renderer.clone();
    let mut gizmo_on = ctx.gizmo_on;
    let mut gizmo_rotate = ctx.gizmo_rotate;
    let max_pressure = ctx.max_pressure;
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

    let on_max_pressure =
        scalar_handler(renderer.clone(), max_pressure, Renderer::set_max_pressure);
    let align_plane = {
        let ctx = ctx.clone();
        move |rot: [f32; 3]| {
            for (axis, v) in rot.into_iter().enumerate() {
                ctx.apply_field(Field::SliceRot, axis, v);
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
                            "Show gizmo"
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
                        Vec3Fields { title: "Position (mm)", field: Field::SliceCenter, values: slice_center(), bounds: slice_bounds(), step: "0.5" }
                        Vec3Fields { title: "Rotation (deg)", field: Field::SliceRot, values: slice_rot(), bounds: [(-180.0, 180.0); 3], step: "1" }
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
                                "{pressure_label}"
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
                                "Color map"
                            }
                            select {
                                class: "select select-sm",
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
