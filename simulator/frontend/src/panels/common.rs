use dioxus::prelude::*;

use crate::context::SharedRenderer;
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
pub(crate) fn NumField(
    label: String,
    accent: String,
    #[props(default = f32::MIN)] min: f32,
    #[props(default = f32::MAX)] max: f32,
    step: String,
    value: String,
    onchange: EventHandler<Event<FormData>>,
    onmousedown: EventHandler<Event<MouseData>>,
) -> Element {
    rsx! {
        div {
            label { class: "label py-1",
                span { class: "label-text {accent} font-medium", "{label}" }
            }
            input {
                r#type: "number",
                class: "input input-bordered input-sm w-full cursor-ew-resize",
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
                span { class: "label-text", "{label}" }
            }
            input {
                r#type: "number",
                class: "input input-bordered input-sm w-full",
                min: "{min}",
                max: "{max}",
                step: "{step}",
                value: "{value}",
                onchange: move |e| onchange.call(e),
            }
        }
    }
}
