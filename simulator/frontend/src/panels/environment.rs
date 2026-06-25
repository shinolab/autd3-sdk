use dioxus::prelude::*;

use crate::context::Ctx;

#[component]
pub fn EnvironmentPanel() -> Element {
    let ctx = use_context::<Ctx>();
    let renderer = ctx.renderer.clone();
    let mut sound_speed = ctx.sound_speed;
    let on_sound_speed = {
        let renderer = renderer.clone();
        move |e: Event<FormData>| {
            if let Ok(v) = e.parsed::<f32>() {
                sound_speed.set(v);
                if let Some(r) = renderer.borrow_mut().as_mut() {
                    r.set_sound_speed(v);
                }
            }
        }
    };
    let sound_speed_label = format!("Sound speed: {:.0} mm/s", sound_speed());

    rsx! {
        div { class: "px-6 pt-4",
            div { class: "card bg-base-100 shadow",
                div { class: "card-body grid grid-cols-1 gap-4 sm:grid-cols-2",
                    div {
                        label { class: "label py-1",
                            span { class: "label-text", "{sound_speed_label}" }
                        }
                        input {
                            r#type: "range",
                            class: "range range-primary range-sm",
                            min: "300000",
                            max: "360000",
                            step: "500",
                            value: "{sound_speed}",
                            oninput: on_sound_speed,
                        }
                    }
                }
            }
        }
    }
}
