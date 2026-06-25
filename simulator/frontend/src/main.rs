mod context;
mod panels;
mod render;
mod settings;
mod tabs;

use dioxus::prelude::*;
use futures_util::{SinkExt, StreamExt};
use glam::Vec2;
use gloo_net::websocket::{Message, futures::WebSocket};
use wasm_bindgen::JsCast;

use autd3_rs_simulator_protocol::{ClientMsg, ServerMsg};

use crate::context::use_app_ctx;
use crate::panels::{CameraPanel, EnvironmentPanel, SettingsPanel, SlicePanel, StatePanel};
use crate::render::{DragUpdate, GizmoMode, Renderer};
use crate::settings::Settings;
use crate::tabs::Tab;

const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");
const CANVAS_W: f32 = 800.0;
const CANVAS_H: f32 = 600.0;

fn to_ndc(x: f64, y: f64) -> Vec2 {
    Vec2::new(
        x as f32 / CANVAS_W * 2.0 - 1.0,
        1.0 - y as f32 / CANVAS_H * 2.0,
    )
}

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

#[component]
fn App() -> Element {
    let saved = use_hook(Settings::load);
    let ctx = use_app_ctx(saved);
    use_context_provider(|| ctx.clone());

    let mut connected = ctx.connected;
    let mut error = ctx.error;
    let mut active_tab = ctx.active_tab;
    let renderer = ctx.renderer.clone();
    let control_rx = ctx.control_rx.clone();
    let control_tx = ctx.control_tx.clone();
    let mut dragging = ctx.dragging;
    let mut gizmo_drag = ctx.gizmo_drag;
    let mut last_pos = ctx.last_pos;
    let mut num_drag = ctx.num_drag;
    let mut slice_center = ctx.slice_center;
    let mut slice_rot = ctx.slice_rot;
    let mut slice_bounds = ctx.slice_bounds;
    let mut cam_pos = ctx.cam_pos;
    let mut cam_rot = ctx.cam_rot;
    let mut device_states = ctx.device_states;
    let playing = ctx.playing;

    let max_pressure = ctx.max_pressure;
    let show_markers = ctx.show_markers;
    let sound_speed = ctx.sound_speed;
    let mod_enabled = ctx.mod_enabled;
    let colormap = ctx.colormap;
    let bg = ctx.bg;
    let gizmo_on = ctx.gizmo_on;
    let gizmo_rotate = ctx.gizmo_rotate;
    let cam_free = ctx.cam_free;
    let fov = ctx.fov;
    let near = ctx.near;
    let far = ctx.far;
    let move_speed = ctx.move_speed;
    use_effect(move || {
        Settings {
            max_pressure: max_pressure(),
            show_markers: show_markers(),
            sound_speed: sound_speed(),
            mod_enabled: mod_enabled(),
            playing: playing(),
            colormap: colormap(),
            bg: bg(),
            gizmo_on: gizmo_on(),
            gizmo_rotate: gizmo_rotate(),
            cam_free: cam_free(),
            fov: fov(),
            near: near(),
            far: far(),
            move_speed: move_speed(),
        }
        .save();
    });

    use_effect({
        let renderer = renderer.clone();
        move || {
            let visible = active_tab() == Tab::Slice && gizmo_on();
            let mode = if gizmo_rotate() {
                GizmoMode::Rotate
            } else {
                GizmoMode::Move
            };
            if let Some(r) = renderer.borrow_mut().as_mut() {
                r.set_gizmo_visible(visible);
                r.set_gizmo_mode(mode);
            }
        }
    });

    use_future({
        let renderer = renderer.clone();
        let control_rx = control_rx.clone();
        let control_tx = control_tx.clone();
        move || {
            let renderer = renderer.clone();
            let control_rx = control_rx.clone();
            let control_tx = control_tx.clone();
            async move {
                let ws = match WebSocket::open(&ws_url()) {
                    Ok(ws) => ws,
                    Err(e) => {
                        error.set(Some(format!("websocket open failed: {e}")));
                        return;
                    }
                };
                connected.set(true);
                let (mut ws_write, mut ws_read) = ws.split();

                if let Some(mut rx) = control_rx.borrow_mut().take() {
                    wasm_bindgen_futures::spawn_local(async move {
                        while let Some(msg) = rx.next().await {
                            match serde_json::to_string(&msg) {
                                Ok(json) => {
                                    if ws_write.send(Message::Text(json)).await.is_err() {
                                        break;
                                    }
                                }
                                Err(e) => tracing::error!("failed to encode client message: {e}"),
                            }
                        }
                    });
                }

                let _ = control_tx.unbounded_send(ClientMsg::SetModulationEnabled {
                    enabled: saved.mod_enabled,
                });

                match init_renderer().await {
                    Ok(mut r) => {
                        r.set_max_pressure(saved.max_pressure);
                        r.set_sound_speed(saved.sound_speed);
                        r.set_show_markers(saved.show_markers);
                        r.set_colormap(u32::from(saved.colormap));
                        r.set_background(saved.bg);
                        r.set_camera_free(saved.cam_free);
                        r.set_fov(saved.fov);
                        r.set_near(saved.near);
                        r.set_far(saved.far);
                        r.set_move_speed(saved.move_speed);
                        *renderer.borrow_mut() = Some(r);
                    }
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

                while let Some(Ok(Message::Text(text))) = ws_read.next().await {
                    match serde_json::from_str::<ServerMsg>(&text) {
                        Ok(ServerMsg::Geometry { transducers }) => {
                            let positions: Vec<[f32; 4]> = transducers
                                .iter()
                                .map(|t| [t.pos[0], t.pos[1], t.pos[2], 0.0])
                                .collect();
                            let directions: Vec<[f32; 4]> = transducers
                                .iter()
                                .map(|t| [t.dir[0], t.dir[1], t.dir[2], 0.0])
                                .collect();
                            if let Some(r) = renderer.borrow_mut().as_mut() {
                                r.set_geometry(&positions, &directions);
                                slice_center.set(r.slice_center());
                                slice_rot.set(r.slice_rot());
                                slice_bounds.set(r.axis_bounds());
                                cam_pos.set(r.camera_pos());
                                cam_rot.set(r.camera_rot());
                            }
                        }
                        Ok(ServerMsg::State { states }) => {
                            if !playing() {
                                continue;
                            }
                            let buf: Vec<[f32; 4]> = states
                                .iter()
                                .map(|s| [s.amp, s.phase, f32::from(u8::from(s.enable)), 0.0])
                                .collect();
                            if let Some(r) = renderer.borrow().as_ref() {
                                r.set_states(&buf);
                            }
                        }
                        Ok(ServerMsg::DeviceStates { devices }) => {
                            device_states.set(devices);
                        }
                        Err(e) => tracing::error!("failed to decode message: {e}"),
                    }
                }
                connected.set(false);
            }
        }
    });

    let on_down = {
        let renderer = renderer.clone();
        move |e: Event<MouseData>| {
            let el = e.element_coordinates();
            let ndc = to_ndc(el.x, el.y);
            let mut on_gizmo = false;
            if let Some(r) = renderer.borrow_mut().as_mut()
                && let Some(axis) = r.pick_gizmo_axis(ndc)
            {
                r.begin_gizmo_drag(axis, ndc);
                on_gizmo = true;
            }
            if on_gizmo {
                gizmo_drag.set(true);
            } else {
                dragging.set(true);
                let c = e.client_coordinates();
                last_pos.set((c.x, c.y));
            }
        }
    };
    let on_up = {
        let renderer = renderer.clone();
        move |_| {
            dragging.set(false);
            gizmo_drag.set(false);
            if let Some(r) = renderer.borrow_mut().as_mut() {
                r.end_gizmo_drag();
            }
        }
    };
    let on_leave = {
        let renderer = renderer.clone();
        move |_| {
            dragging.set(false);
            gizmo_drag.set(false);
            if let Some(r) = renderer.borrow_mut().as_mut() {
                r.end_gizmo_drag();
            }
        }
    };
    let on_move = {
        let renderer = renderer.clone();
        move |e: Event<MouseData>| {
            let el = e.element_coordinates();
            let ndc = to_ndc(el.x, el.y);
            if gizmo_drag() {
                if let Some(r) = renderer.borrow_mut().as_mut() {
                    match r.update_gizmo_drag(ndc) {
                        Some(DragUpdate::Translate(c)) => slice_center.set(c),
                        Some(DragUpdate::Rotate(r)) => slice_rot.set(r),
                        None => {}
                    }
                }
            } else if dragging() {
                let c = e.client_coordinates();
                let (lx, ly) = last_pos();
                last_pos.set((c.x, c.y));
                if let Some(r) = renderer.borrow_mut().as_mut() {
                    r.orbit((c.x - lx) as f32, (c.y - ly) as f32);
                    cam_pos.set(r.camera_pos());
                    cam_rot.set(r.camera_rot());
                }
            } else if let Some(r) = renderer.borrow_mut().as_mut() {
                r.set_hover(ndc);
            }
        }
    };
    let on_wheel = {
        let renderer = renderer.clone();
        move |e: Event<WheelData>| {
            let delta = e.delta().strip_units().y as f32;
            if let Some(r) = renderer.borrow_mut().as_mut() {
                r.zoom(delta);
                cam_pos.set(r.camera_pos());
            }
        }
    };
    let on_root_move = {
        let ctx = ctx.clone();
        move |e: Event<MouseData>| {
            if let Some((kind, axis, start_x, start_val)) = num_drag() {
                let delta = (e.client_coordinates().x - start_x) as f32 * 0.5;
                let v = ctx.clamp_field(kind, axis, start_val + delta);
                ctx.apply_field(kind, axis, v);
            }
        }
    };
    let on_root_up = move |_: Event<MouseData>| num_drag.set(None);

    rsx! {
        document::Stylesheet { href: TAILWIND_CSS }
        div {
            class: "min-h-screen bg-base-200 text-base-content",
            onmousemove: on_root_move,
            onmouseup: on_root_up,
            div { class: "navbar bg-base-100 shadow gap-2",
                div { class: "px-2 text-lg font-semibold", "AUTD3 Simulator" }
                div { role: "tablist", class: "tabs tabs-boxed flex-1",
                    for tab in Tab::ALL {
                        button {
                            role: "tab",
                            class: if active_tab() == tab { "tab tab-active" } else { "tab" },
                            onclick: move |_| active_tab.set(tab),
                            "{tab.label()}"
                        }
                    }
                }
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
            match active_tab() {
                Tab::Slice => rsx! { SlicePanel {} },
                Tab::Camera => rsx! { CameraPanel {} },
                Tab::Field => rsx! { EnvironmentPanel {} },
                Tab::State => rsx! { StatePanel {} },
                Tab::Settings => rsx! { SettingsPanel {} },
                Tab::Home => rsx! {},
            }
            div { class: "px-6 py-6",
                div { class: "flex justify-center rounded border border-base-300 bg-base-200 p-4",
                    canvas {
                        id: "field",
                        width: "800",
                        height: "600",
                        class: "rounded shadow cursor-grab",
                        onmousedown: on_down,
                        onmouseup: on_up,
                        onmouseleave: on_leave,
                        onmousemove: on_move,
                        onwheel: on_wheel,
                    }
                }
            }
        }
    }
}
