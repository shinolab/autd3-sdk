use dioxus::prelude::*;

use super::common::scalar_handler;
use crate::context::Ctx;
use crate::render::Renderer;

#[component]
pub fn EnvironmentPanel() -> Element {
    let ctx = use_context::<Ctx>();
    let sound_speed = ctx.sound_speed;
    let on_sound_speed =
        scalar_handler(ctx.renderer.clone(), sound_speed, Renderer::set_sound_speed);
    let sound_speed_label = format!("Sound speed: {:.0} mm/s", sound_speed());

    rsx! {
        div { class: "px-6 pt-4",
            div { class: "card bg-base-100 shadow",
                div { class: "card-body grid grid-cols-1 gap-4 sm:grid-cols-2",
                    div {
                        label { class: "label py-1",
                            "{sound_speed_label}"
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
