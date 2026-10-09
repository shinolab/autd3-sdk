use dioxus::prelude::*;

use crate::context::{Ctx, Field, SharedRenderer};
use crate::render::Renderer;

pub(crate) fn scalar_handler(
    renderer: SharedRenderer,
    mut signal: Signal<f32>,
    setter: fn(&mut Renderer, f32),
) -> impl FnMut(Event<FormData>) + 'static {
    move |e: Event<FormData>| {
        if let Ok(v) = e.parsed::<f32>() {
            signal.set(v);
            if let Some(r) = renderer.borrow_mut().as_mut() {
                setter(r, v);
            }
        }
    }
}

#[component]
pub(crate) fn Vec3Fields(
    title: String,
    field: Field,
    values: [f32; 3],
    #[props(default = [(f32::MIN, f32::MAX); 3])] bounds: [(f32, f32); 3],
    step: String,
) -> Element {
    let ctx = use_context::<Ctx>();
    let labels = match field {
        Field::SliceCenter | Field::CameraPos => ["X", "Y", "Z"],
        Field::SliceRot | Field::CameraRot => ["RX", "RY", "RZ"],
    };
    let accents = ["text-error", "text-success", "text-info"];
    rsx! {
        div { class: "flex flex-col gap-3",
            div { class: "text-sm font-semibold opacity-70", "{title}" }
            for axis in 0..3 {
                NumField {
                    key: "{axis}",
                    label: labels[axis],
                    accent: accents[axis],
                    min: bounds[axis].0,
                    max: bounds[axis].1,
                    step: step.clone(),
                    value: format!("{:.1}", values[axis]),
                    onchange: ctx.field_handler(field, axis),
                    onmousedown: ctx.num_down(field, axis),
                }
            }
        }
    }
}

#[component]
fn NumField(
    label: String,
    accent: String,
    min: f32,
    max: f32,
    step: String,
    value: String,
    onchange: EventHandler<Event<FormData>>,
    onmousedown: EventHandler<Event<MouseData>>,
) -> Element {
    rsx! {
        div {
            label { class: "label py-1",
                span { class: "{accent} font-medium", "{label}" }
            }
            input {
                r#type: "number",
                class: "input input-sm w-full cursor-ew-resize",
                min: "{min}",
                max: "{max}",
                step: "{step}",
                value: "{value}",
                onchange: move |e| onchange.call(e),
                onmousedown: move |e| onmousedown.call(e),
            }
        }
    }
}

#[component]
pub(crate) fn PlainNum(
    label: String,
    min: f32,
    max: f32,
    step: String,
    value: String,
    onchange: EventHandler<Event<FormData>>,
) -> Element {
    rsx! {
        div {
            label { class: "label py-1",
                "{label}"
            }
            input {
                r#type: "number",
                class: "input input-sm w-full",
                min: "{min}",
                max: "{max}",
                step: "{step}",
                value: "{value}",
                onchange: move |e| onchange.call(e),
            }
        }
    }
}
