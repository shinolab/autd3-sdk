use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;
use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};

use autd3_rs_simulator_protocol::{ClientMsg, DeviceState};

use crate::render::Renderer;
use crate::settings::Settings;
use crate::tabs::Tab;

pub type SharedRenderer = Rc<RefCell<Option<Renderer>>>;
pub type ControlRx = Rc<RefCell<Option<UnboundedReceiver<ClientMsg>>>>;

#[derive(Clone)]
pub struct Ctx {
    pub renderer: SharedRenderer,
    pub control_rx: ControlRx,
    pub control_tx: UnboundedSender<ClientMsg>,

    pub connected: Signal<bool>,
    pub error: Signal<Option<String>>,
    pub device_states: Signal<Vec<DeviceState>>,
    pub active_tab: Signal<Tab>,

    pub dragging: Signal<bool>,
    pub gizmo_drag: Signal<bool>,
    pub last_pos: Signal<(f64, f64)>,
    pub num_drag: Signal<Option<(u8, usize, f64, f32)>>,

    pub max_pressure: Signal<f32>,
    pub slice_center: Signal<[f32; 3]>,
    pub slice_rot: Signal<[f32; 3]>,
    pub slice_bounds: Signal<[(f32, f32); 3]>,
    pub cam_pos: Signal<[f32; 3]>,
    pub cam_rot: Signal<[f32; 3]>,
    pub sound_speed: Signal<f32>,
    pub show_markers: Signal<bool>,
    pub playing: Signal<bool>,
    pub mod_enabled: Signal<bool>,
    pub colormap: Signal<u8>,
    pub bg: Signal<[f32; 3]>,
    pub gizmo_on: Signal<bool>,
    pub gizmo_rotate: Signal<bool>,
    pub cam_free: Signal<bool>,
    pub fov: Signal<f32>,
    pub near: Signal<f32>,
    pub far: Signal<f32>,
    pub move_speed: Signal<f32>,
}

pub fn use_app_ctx(saved: Settings) -> Ctx {
    let renderer: SharedRenderer = use_hook(|| Rc::new(RefCell::new(None)));
    let control: (UnboundedSender<ClientMsg>, ControlRx) = use_hook(|| {
        let (tx, rx) = unbounded::<ClientMsg>();
        (tx, Rc::new(RefCell::new(Some(rx))))
    });
    Ctx {
        renderer,
        control_rx: control.1.clone(),
        control_tx: control.0.clone(),
        connected: use_signal(|| false),
        error: use_signal(|| None),
        device_states: use_signal(Vec::new),
        active_tab: use_signal(|| Tab::Home),
        dragging: use_signal(|| false),
        gizmo_drag: use_signal(|| false),
        last_pos: use_signal(|| (0.0, 0.0)),
        num_drag: use_signal(|| None),
        max_pressure: use_signal(|| saved.max_pressure),
        slice_center: use_signal(|| [0.0; 3]),
        slice_rot: use_signal(|| [0.0; 3]),
        slice_bounds: use_signal(|| [(-100.0, 100.0); 3]),
        cam_pos: use_signal(|| [0.0; 3]),
        cam_rot: use_signal(|| [0.0; 3]),
        sound_speed: use_signal(|| saved.sound_speed),
        show_markers: use_signal(|| saved.show_markers),
        playing: use_signal(|| saved.playing),
        mod_enabled: use_signal(|| saved.mod_enabled),
        colormap: use_signal(|| saved.colormap),
        bg: use_signal(|| saved.bg),
        gizmo_on: use_signal(|| saved.gizmo_on),
        gizmo_rotate: use_signal(|| saved.gizmo_rotate),
        cam_free: use_signal(|| saved.cam_free),
        fov: use_signal(|| saved.fov),
        near: use_signal(|| saved.near),
        far: use_signal(|| saved.far),
        move_speed: use_signal(|| saved.move_speed),
    }
}

impl Ctx {
    pub fn with_renderer<R>(&self, f: impl FnOnce(&mut Renderer) -> R) -> Option<R> {
        self.renderer.borrow_mut().as_mut().map(f)
    }

    pub fn apply_field(&self, kind: u8, axis: usize, value: f32) {
        let (mut slice_center, mut slice_rot, mut cam_pos, mut cam_rot) = (
            self.slice_center,
            self.slice_rot,
            self.cam_pos,
            self.cam_rot,
        );
        let mut r = self.renderer.borrow_mut();
        let r = r.as_mut();
        match kind {
            0 => {
                let mut c = slice_center();
                c[axis] = value;
                slice_center.set(c);
                if let Some(r) = r {
                    r.set_slice_coord(axis, value);
                }
            }
            1 => {
                let mut a = slice_rot();
                a[axis] = value;
                slice_rot.set(a);
                if let Some(r) = r {
                    r.set_slice_rot(axis, value);
                }
            }
            2 => {
                if let Some(r) = r {
                    r.set_camera_pos(axis, value);
                    cam_pos.set(r.camera_pos());
                    cam_rot.set(r.camera_rot());
                }
            }
            _ => {
                let mut a = cam_rot();
                a[axis] = value;
                cam_rot.set(a);
                if let Some(r) = r {
                    r.set_camera_rot(axis, value);
                }
            }
        }
    }

    pub fn clamp_field(&self, kind: u8, axis: usize, v: f32) -> f32 {
        let bounds = self.slice_bounds;
        match kind {
            1 | 3 => v.clamp(-180.0, 180.0),
            0 => {
                let (lo, hi) = bounds()[axis];
                v.clamp(lo, hi)
            }
            _ => v,
        }
    }

    pub fn field_handler(&self, kind: u8, axis: usize) -> impl FnMut(Event<FormData>) + 'static {
        let ctx = self.clone();
        move |e: Event<FormData>| {
            if let Ok(v) = e.parsed::<f32>() {
                let v = ctx.clamp_field(kind, axis, v);
                ctx.apply_field(kind, axis, v);
            }
        }
    }

    pub fn num_down(&self, kind: u8, axis: usize) -> impl FnMut(Event<MouseData>) + 'static {
        let ctx = self.clone();
        move |e: Event<MouseData>| {
            let (sc, sr, cp, cr) = (ctx.slice_center, ctx.slice_rot, ctx.cam_pos, ctx.cam_rot);
            let start = match kind {
                0 => sc()[axis],
                1 => sr()[axis],
                2 => cp()[axis],
                _ => cr()[axis],
            };
            let mut num_drag = ctx.num_drag;
            num_drag.set(Some((kind, axis, e.client_coordinates().x, start)));
        }
    }
}
