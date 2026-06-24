mod render;

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;
use futures_util::StreamExt;
use gloo_net::websocket::{Message, futures::WebSocket};
use wasm_bindgen::JsCast;

use autd3_rs_simulator_protocol::ServerMsg;

use crate::render::Renderer;

const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

fn ws_url() -> String {
    let location = web_sys::window().expect("no window").location();
    let scheme = match location.protocol().as_deref() {
        Ok("https:") => "wss",
        _ => "ws",
    };
    let host = location
        .host()
        .unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    format!("{scheme}://{host}/ws")
}

fn main() {
    console_error_panic_hook::set_once();
    dioxus::logger::initialize_default();
    dioxus::launch(App);
}

async fn init_renderer() -> Result<Renderer, String> {
    let canvas = canvas_element("field")
        .await
        .ok_or_else(|| "canvas element not found".to_string())?;
    Renderer::new(canvas)
        .await
        .map_err(|e| format!("WebGPU init failed: {e}"))
}

async fn canvas_element(id: &str) -> Option<web_sys::HtmlCanvasElement> {
    for _ in 0..100 {
        if let Some(canvas) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id(id))
            .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
        {
            return Some(canvas);
        }
        gloo_timers::future::TimeoutFuture::new(20).await;
    }
    None
}

type SharedRenderer = Rc<RefCell<Option<Renderer>>>;

#[component]
fn App() -> Element {
    let mut connected = use_signal(|| false);
    let mut num_transducers = use_signal(|| 0usize);
    let mut updates = use_signal(|| 0u64);
    let mut max_amp = use_signal(|| 0.0f32);
    let mut error = use_signal(|| Option::<String>::None);
    let renderer: SharedRenderer = use_hook(|| Rc::new(RefCell::new(None)));

    let mut dragging = use_signal(|| false);
    let mut last_pos = use_signal(|| (0.0f64, 0.0f64));

    let mut max_pressure = use_signal(|| 8000.0f32);
    let mut slice_pos = use_signal(|| 0.5f32);
    let mut marker_size = use_signal(|| 4.5f32);
    let mut show_markers = use_signal(|| true);

    use_future({
        let renderer = renderer.clone();
        move || {
            let renderer = renderer.clone();
            async move {
                let mut ws = match WebSocket::open(&ws_url()) {
                    Ok(ws) => ws,
                    Err(e) => {
                        error.set(Some(format!("websocket open failed: {e}")));
                        return;
                    }
                };
                connected.set(true);

                match init_renderer().await {
                    Ok(r) => *renderer.borrow_mut() = Some(r),
                    Err(e) => error.set(Some(e)),
                }

                {
                    let renderer = renderer.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        loop {
                            if let Some(r) = renderer.borrow_mut().as_mut() {
                                r.render();
                            }
                            gloo_timers::future::TimeoutFuture::new(33).await;
                        }
                    });
                }

                while let Some(Ok(Message::Text(text))) = ws.next().await {
                    match serde_json::from_str::<ServerMsg>(&text) {
                        Ok(ServerMsg::Geometry { transducers }) => {
                            num_transducers.set(transducers.len());
                            let positions: Vec<[f32; 4]> = transducers
                                .iter()
                                .map(|t| [t.pos[0], t.pos[1], t.pos[2], 0.0])
                                .collect();
                            if let Some(r) = renderer.borrow_mut().as_mut() {
                                r.set_geometry(&positions);
                            }
                        }
                        Ok(ServerMsg::State { states }) => {
                            updates.set(updates() + 1);
                            max_amp.set(states.iter().map(|s| s.amp).fold(0.0, f32::max));
                            let buf: Vec<[f32; 4]> = states
                                .iter()
                                .map(|s| [s.amp, s.phase, f32::from(u8::from(s.enable)), 0.0])
                                .collect();
                            if let Some(r) = renderer.borrow().as_ref() {
                                r.set_states(&buf);
                            }
                        }
                        Err(e) => tracing::error!("failed to decode message: {e}"),
                    }
                }
                connected.set(false);
            }
        }
    });

    let on_move = {
        let renderer = renderer.clone();
        move |e: Event<MouseData>| {
            if dragging() {
                let c = e.client_coordinates();
                let (lx, ly) = last_pos();
                last_pos.set((c.x, c.y));
                if let Some(r) = renderer.borrow_mut().as_mut() {
                    r.orbit((c.x - lx) as f32, (c.y - ly) as f32);
                }
            }
        }
    };
    let on_wheel = {
        let renderer = renderer.clone();
        move |e: Event<WheelData>| {
            let delta = e.delta().strip_units().y as f32;
            if let Some(r) = renderer.borrow_mut().as_mut() {
                r.zoom(delta);
            }
        }
    };
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
    let on_slice = {
        let renderer = renderer.clone();
        move |e: Event<FormData>| {
            if let Ok(v) = e.parsed::<f32>() {
                slice_pos.set(v);
                if let Some(r) = renderer.borrow_mut().as_mut() {
                    r.set_slice_pos(v);
                }
            }
        }
    };
    let on_marker = {
        let renderer = renderer.clone();
        move |e: Event<FormData>| {
            if let Ok(v) = e.parsed::<f32>() {
                marker_size.set(v);
                if let Some(r) = renderer.borrow_mut().as_mut() {
                    r.set_marker_size(v);
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

    let pressure_label = format!("Max pressure: {:.0} Pa", max_pressure());
    let slice_label = format!("Slice Y: {:.2}", slice_pos());
    let marker_label = format!("Marker size: {:.1} mm", marker_size());

    rsx! {
        document::Stylesheet { href: TAILWIND_CSS }
        div { class: "min-h-screen bg-base-200 text-base-content",
            div { class: "navbar bg-base-100 shadow",
                div { class: "flex-1 px-2 text-lg font-semibold", "AUTD3 Simulator" }
                div { class: "flex-none px-2",
                    if connected() {
                        span { class: "badge badge-success", "connected" }
                    } else {
                        span { class: "badge badge-error", "disconnected" }
                    }
                }
            }
            if let Some(message) = error() {
                div { class: "mx-6 mt-4 alert alert-error", "{message}" }
            }
            div { class: "p-6 grid grid-cols-1 gap-4 sm:grid-cols-3",
                StatCard { title: "Transducers", value: num_transducers().to_string() }
                StatCard { title: "Updates", value: updates().to_string() }
                StatCard { title: "Max amp", value: format!("{:.3}", max_amp()) }
            }
            div { class: "px-6",
                div { class: "card bg-base-100 shadow",
                    div { class: "card-body grid grid-cols-1 gap-4 sm:grid-cols-2",
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
                                span { class: "label-text", "{slice_label}" }
                            }
                            input {
                                r#type: "range",
                                class: "range range-primary range-sm",
                                min: "0",
                                max: "1",
                                step: "0.01",
                                value: "{slice_pos}",
                                oninput: on_slice,
                            }
                        }
                        div {
                            label { class: "label py-1",
                                span { class: "label-text", "{marker_label}" }
                            }
                            input {
                                r#type: "range",
                                class: "range range-primary range-sm",
                                min: "0",
                                max: "10",
                                step: "0.5",
                                value: "{marker_size}",
                                oninput: on_marker,
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
                                span { class: "label-text", "Show transducers" }
                            }
                        }
                    }
                }
            }
            div { class: "px-6 py-6",
                div { class: "mockup-window border border-base-300 bg-base-300",
                    div { class: "flex justify-center bg-base-200 p-4",
                        canvas {
                            id: "field",
                            width: "800",
                            height: "600",
                            class: "rounded shadow cursor-grab",
                            onmousedown: move |e: Event<MouseData>| {
                                dragging.set(true);
                                let c = e.client_coordinates();
                                last_pos.set((c.x, c.y));
                            },
                            onmouseup: move |_| dragging.set(false),
                            onmouseleave: move |_| dragging.set(false),
                            onmousemove: on_move,
                            onwheel: on_wheel,
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn StatCard(title: String, value: String) -> Element {
    rsx! {
        div { class: "stats bg-base-100 shadow",
            div { class: "stat",
                div { class: "stat-title", "{title}" }
                div { class: "stat-value text-primary", "{value}" }
            }
        }
    }
}
