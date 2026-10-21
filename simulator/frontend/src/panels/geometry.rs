use dioxus::prelude::*;
use wasm_bindgen::{JsCast, JsValue};

use autd3_rs_simulator_protocol::{ClientMsg, MAX_DEVICES};

use crate::context::Ctx;
use crate::geometry::{self, DeviceDraft, to_poses};

const GEOMETRY_FILE_NAME: &str = "geometry.json";
const DOWNLOAD_URL_LIFETIME_MS: u32 = 1000;

#[derive(Clone, Copy, PartialEq)]
enum PoseField {
    Origin,
    EulerZyz,
}

fn commit(ctx: &Ctx, next: Vec<DeviceDraft>) {
    let devices = to_poses(&next);
    if ctx
        .control_tx
        .unbounded_send(ClientMsg::SetGeometry {
            devices: devices.clone(),
        })
        .is_err()
    {
        return;
    }
    ctx.unanswered_geometries.borrow_mut().record(devices);
    let mut drafts = ctx.geometry_draft;
    drafts.set(next);
}

fn edit(ctx: &Ctx, f: impl FnOnce(&mut Vec<DeviceDraft>)) {
    let mut next = ctx.geometry_draft.peek().clone();
    f(&mut next);
    commit(ctx, next);
}

fn display(v: f32) -> String {
    let rounded = format!("{v:.3}");
    let trimmed = rounded.trim_end_matches('0').trim_end_matches('.');
    match trimmed {
        "-0" | "" => "0".to_string(),
        s => s.to_string(),
    }
}

fn download(name: &str, content: &str) -> Result<(), JsValue> {
    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or_else(|| JsValue::from_str("no document"))?;
    let parts = js_sys::Array::of1(&JsValue::from_str(content));
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("application/json");
    let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &options)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)?;
    let anchor: web_sys::HtmlAnchorElement = document.create_element("a")?.dyn_into()?;
    anchor.set_href(&url);
    anchor.set_download(name);
    anchor.click();
    wasm_bindgen_futures::spawn_local(async move {
        gloo_timers::future::TimeoutFuture::new(DOWNLOAD_URL_LIFETIME_MS).await;
        let _ = web_sys::Url::revoke_object_url(&url);
    });
    Ok(())
}

#[component]
pub fn GeometryPanel() -> Element {
    let ctx = use_context::<Ctx>();
    let drafts = ctx.geometry_draft;
    let count = drafts().len();
    let editable = (ctx.connected)();
    let mut file_error = use_signal(|| None::<String>);
    let mut file_input_generation = use_signal(|| 0_u32);

    let on_save = move |_| {
        let result = geometry::to_file(&drafts.peek()).and_then(|content| {
            download(GEOMETRY_FILE_NAME, &content)
                .map_err(|e| format!("could not save the file: {e:?}"))
        });
        file_error.set(result.err());
    };
    let on_load = {
        let ctx = ctx.clone();
        move |e: Event<FormData>| {
            let ctx = ctx.clone();
            async move {
                let Some(file) = e.files().into_iter().next() else {
                    return;
                };
                let loaded = match file.read_string().await {
                    Ok(text) => geometry::from_file(&text),
                    Err(e) => Err(format!("could not read {}: {e}", file.name())),
                };
                match loaded {
                    Ok(next) => {
                        file_error.set(None);
                        commit(&ctx, next);
                    }
                    Err(message) => file_error.set(Some(format!("{}: {message}", file.name()))),
                }
                file_input_generation += 1;
            }
        }
    };

    let on_add = {
        let ctx = ctx.clone();
        move |_| {
            edit(&ctx, |next| {
                if let Some(last) = next.last() {
                    next.push(last.next_to());
                }
            });
        }
    };

    rsx! {
        div { class: "px-6 pt-4",
            div { class: "card bg-base-100 shadow",
                div { class: "card-body gap-3",
                    if count == 0 {
                        div { class: "text-sm opacity-70",
                            "Waiting for the geometry from the simulator backend."
                        }
                    } else {
                        div { class: "text-sm opacity-70",
                            "Device n is the n-th device of the daisy chain (geometry[n] of the client). "
                            "Changing the number of devices resets the emulated devices; reopen the client afterwards."
                        }
                        if !editable {
                            div { class: "text-sm text-error",
                                "Disconnected from the simulator backend; the geometry cannot be edited."
                            }
                        }
                        for (index, draft) in drafts().into_iter().enumerate() {
                            DeviceRow { key: "{index}", index, draft, count, editable }
                        }
                        div { class: "flex flex-wrap items-center gap-2",
                            button {
                                class: "btn btn-sm",
                                disabled: !editable || count >= MAX_DEVICES,
                                onclick: on_add,
                                "Add device"
                            }
                            button { class: "btn btn-sm", onclick: on_save, "Save to file" }
                            label {
                                class: if editable { "btn btn-sm" } else { "btn btn-sm btn-disabled" },
                                "Load from file"
                                input {
                                    key: "{file_input_generation}",
                                    r#type: "file",
                                    accept: ".json,application/json",
                                    class: "hidden",
                                    disabled: !editable,
                                    onchange: on_load,
                                }
                            }
                        }
                        if let Some(message) = file_error() {
                            div { class: "text-sm text-error", "{message}" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn DeviceRow(index: usize, draft: DeviceDraft, count: usize, editable: bool) -> Element {
    let ctx = use_context::<Ctx>();
    let on_up = {
        let ctx = ctx.clone();
        move |_| {
            edit(&ctx, |next| {
                if (1..next.len()).contains(&index) {
                    next.swap(index - 1, index);
                }
            });
        }
    };
    let on_down = {
        let ctx = ctx.clone();
        move |_| {
            edit(&ctx, |next| {
                if index + 1 < next.len() {
                    next.swap(index, index + 1);
                }
            });
        }
    };
    let on_remove = {
        let ctx = ctx.clone();
        move |_| {
            edit(&ctx, |next| {
                if index < next.len() && next.len() > 1 {
                    next.remove(index);
                }
            });
        }
    };
    rsx! {
        div { class: "flex flex-wrap items-end gap-2 rounded bg-base-200 p-2",
            div { class: "w-20 pb-1 text-sm font-semibold", "Device {index}" }
            PoseInputs { index, field: PoseField::Origin, values: draft.origin, editable }
            PoseInputs { index, field: PoseField::EulerZyz, values: draft.euler_zyz_deg, editable }
            div { class: "join",
                button {
                    class: "btn btn-sm join-item",
                    disabled: !editable || index == 0,
                    onclick: on_up,
                    "Up"
                }
                button {
                    class: "btn btn-sm join-item",
                    disabled: !editable || index + 1 >= count,
                    onclick: on_down,
                    "Down"
                }
                button {
                    class: "btn btn-sm btn-error join-item",
                    disabled: !editable || count <= 1,
                    onclick: on_remove,
                    "Remove"
                }
            }
        }
    }
}

#[component]
fn PoseInputs(index: usize, field: PoseField, values: [f32; 3], editable: bool) -> Element {
    let ctx = use_context::<Ctx>();
    let (title, labels, step) = match field {
        PoseField::Origin => ("Origin (mm)", ["X", "Y", "Z"], "1"),
        PoseField::EulerZyz => ("Rotation ZYZ (deg)", ["Z", "Y", "Z"], "1"),
    };
    let accents = ["text-error", "text-success", "text-info"];
    rsx! {
        div { class: "flex flex-col gap-1",
            div { class: "text-xs opacity-70", "{title}" }
            div { class: "flex gap-1",
                for axis in 0..3 {
                    label {
                        key: "{axis}",
                        class: "input input-sm w-28",
                        span { class: "{accents[axis]} font-medium", "{labels[axis]}" }
                        input {
                            r#type: "number",
                            step,
                            disabled: !editable,
                            value: display(values[axis]),
                            onchange: {
                                let ctx = ctx.clone();
                                move |e: Event<FormData>| {
                                    let Ok(v) = e.parsed::<f32>() else {
                                        return;
                                    };
                                    if !v.is_finite() {
                                        return;
                                    }
                                    edit(&ctx, |next| {
                                        if let Some(device) = next.get_mut(index) {
                                            match field {
                                                PoseField::Origin => device.origin[axis] = v,
                                                PoseField::EulerZyz => device.euler_zyz_deg[axis] = v,
                                            }
                                        }
                                    });
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}
