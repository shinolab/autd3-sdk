use dioxus::prelude::*;

use autd3_rs_simulator_protocol::DeviceState;

use crate::context::Ctx;

#[component]
pub fn StatePanel() -> Element {
    let ctx = use_context::<Ctx>();
    let device_states = ctx.device_states;
    rsx! {
        div { class: "px-6 pt-4",
            div { class: "card bg-base-100 shadow",
                div { class: "card-body",
                    if device_states().is_empty() {
                        div { class: "text-sm opacity-70",
                            "No device state yet — connect a client to populate this view."
                        }
                    } else {
                        div { class: "flex flex-col gap-2",
                            for (i, d) in device_states().into_iter().enumerate() {
                                DeviceNode { idx: i, dev: d }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn mod_buffer_points(buffer: &[u8]) -> String {
    let n = buffer.len();
    if n < 2 {
        return String::new();
    }
    let denom = (n - 1) as f32;
    buffer
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            let x = i as f32 / denom * 300.0;
            let y = 59.0 - (f32::from(v) / 255.0) * 58.0;
            format!("{x:.1},{y:.1}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[component]
fn KvRow(k: String, v: String) -> Element {
    rsx! {
        div { class: "flex gap-3 py-0.5",
            span { class: "opacity-60 w-40 shrink-0", "{k}" }
            span { class: "font-mono", "{v}" }
        }
    }
}

#[component]
fn DeviceNode(idx: usize, dev: DeviceState) -> Element {
    let silencer_mode = if dev.silencer_fixed_update_rate {
        "Fixed update rate"
    } else {
        "Completion steps"
    };
    let mod_points = mod_buffer_points(&dev.mod_buffer);
    rsx! {
        details { class: "collapse collapse-arrow bg-base-200 rounded", open: true,
            summary { class: "collapse-title text-sm font-semibold py-2 min-h-0",
                "Device {idx} · {dev.num_transducers} transducers"
            }
            div { class: "collapse-content text-sm",
                div { class: "font-semibold opacity-70 pb-1", "Silencer" }
                KvRow { k: "Mode", v: silencer_mode.to_string() }
                KvRow { k: "Intensity", v: dev.silencer_intensity.to_string() }
                KvRow { k: "Phase", v: dev.silencer_phase.to_string() }
                div { class: "font-semibold opacity-70 pt-3 pb-1", "Modulation" }
                KvRow { k: "Freq division", v: dev.mod_freq_div.to_string() }
                KvRow { k: "Cycle (samples)", v: dev.mod_cycle.to_string() }
                KvRow { k: "Index", v: dev.mod_idx.to_string() }
                if !mod_points.is_empty() {
                    svg {
                        class: "w-full text-primary mt-1 rounded bg-base-100",
                        height: "60",
                        "viewBox": "0 0 300 60",
                        "preserveAspectRatio": "none",
                        polyline {
                            points: "{mod_points}",
                            fill: "none",
                            stroke: "currentColor",
                            "stroke-width": "1.5",
                            "vector-effect": "non-scaling-stroke",
                        }
                    }
                }
                div { class: "font-semibold opacity-70 pt-3 pb-1", "STM" }
                if dev.stm_cycle <= 1 {
                    div { class: "opacity-60 py-0.5", "Static (single pattern)" }
                } else {
                    KvRow { k: "Freq division", v: dev.stm_freq_div.to_string() }
                    KvRow { k: "Cycle (patterns)", v: dev.stm_cycle.to_string() }
                    KvRow { k: "Index", v: dev.stm_idx.to_string() }
                }
            }
        }
    }
}
