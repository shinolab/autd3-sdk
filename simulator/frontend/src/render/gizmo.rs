use glam::Vec3;

pub(crate) const RING_SEGMENTS: u32 = 64;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GizmoMode {
    Move,
    Rotate,
}

pub enum DragUpdate {
    Translate([f32; 3]),
    Rotate([f32; 3]),
}

fn axis_unit(axis: usize) -> Vec3 {
    match axis {
        0 => Vec3::X,
        1 => Vec3::Y,
        _ => Vec3::Z,
    }
}

fn ring_basis(axis: usize) -> (Vec3, Vec3) {
    match axis {
        0 => (Vec3::Y, Vec3::Z),
        1 => (Vec3::Z, Vec3::X),
        _ => (Vec3::X, Vec3::Y),
    }
}

fn axis_param(anchor: Vec3, dir: Vec3, ray_o: Vec3, ray_d: Vec3) -> f32 {
    let delta = anchor - ray_o;
    let dir_dot_ray = dir.dot(ray_d);
    let dir_dot_delta = dir.dot(delta);
    let ray_dot_delta = ray_d.dot(delta);
    let denom = 1.0 - dir_dot_ray * dir_dot_ray;
    if denom.abs() < 1e-6 {
        0.0
    } else {
        (dir_dot_ray * ray_dot_delta - dir_dot_delta) / denom
    }
}

fn ring_angle(center: Vec3, axis: usize, ray_o: Vec3, ray_d: Vec3) -> f32 {
    let n = axis_unit(axis);
    let denom = ray_d.dot(n);
    if denom.abs() < 1e-6 {
        return 0.0;
    }
    let t = (center - ray_o).dot(n) / denom;
    let hit = ray_o + ray_d * t - center;
    let (e0, e1) = ring_basis(axis);
    hit.dot(e1).atan2(hit.dot(e0))
}

pub(crate) struct Gizmo {
    pub(crate) len: f32,
    pub(crate) visible: bool,
    pub(crate) mode: GizmoMode,
    active_axis: Option<usize>,
    drag: Option<(GizmoMode, usize)>,
    drag_start_center: Vec3,
    drag_start_t: f32,
    drag_start_rot: Vec3,
    drag_start_angle: f32,
}

impl Gizmo {
    pub(crate) fn new() -> Self {
        Self {
            len: 0.0,
            visible: false,
            mode: GizmoMode::Move,
            active_axis: None,
            drag: None,
            drag_start_center: Vec3::ZERO,
            drag_start_t: 0.0,
            drag_start_rot: Vec3::ZERO,
            drag_start_angle: 0.0,
        }
    }

    pub(crate) fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
        if !visible {
            self.active_axis = None;
            self.drag = None;
        }
    }

    pub(crate) fn set_mode(&mut self, mode: GizmoMode) {
        self.mode = mode;
        self.active_axis = None;
        self.drag = None;
    }

    pub(crate) fn active_axis_i32(&self) -> i32 {
        self.drag
            .map(|(_, axis)| axis)
            .or(self.active_axis)
            .and_then(|a| i32::try_from(a).ok())
            .unwrap_or(-1)
    }

    fn pick_move(&self, center: Vec3, o: Vec3, rd: Vec3) -> Option<usize> {
        let pick_radius = self.len * 0.14;
        let mut best: Option<(usize, f32)> = None;
        for axis in 0..3 {
            let dir = axis_unit(axis);
            let t = axis_param(center, dir, o, rd).clamp(0.0, self.len * 1.1);
            let pa = center + dir * t;
            let s = (pa - o).dot(rd).max(0.0);
            let dist = (pa - (o + rd * s)).length();
            if dist < pick_radius && best.is_none_or(|(_, bd)| dist < bd) {
                best = Some((axis, dist));
            }
        }
        best.map(|(axis, _)| axis)
    }

    fn pick_rotate(&self, center: Vec3, o: Vec3, rd: Vec3) -> Option<usize> {
        let tol = self.len * 0.12;
        let mut best: Option<(usize, f32)> = None;
        for axis in 0..3 {
            let n = axis_unit(axis);
            let denom = rd.dot(n);
            if denom.abs() < 1e-6 {
                continue;
            }
            let t = (center - o).dot(n) / denom;
            if t < 0.0 {
                continue;
            }
            let off = ((o + rd * t) - center).length() - self.len;
            if off.abs() < tol && best.is_none_or(|(_, bd)| off.abs() < bd) {
                best = Some((axis, off.abs()));
            }
        }
        best.map(|(axis, _)| axis)
    }

    pub(crate) fn pick(&self, center: Vec3, o: Vec3, rd: Vec3) -> Option<usize> {
        if !self.visible || self.len <= 0.0 {
            return None;
        }
        match self.mode {
            GizmoMode::Move => self.pick_move(center, o, rd),
            GizmoMode::Rotate => self.pick_rotate(center, o, rd),
        }
    }

    pub(crate) fn set_hover(&mut self, center: Vec3, o: Vec3, rd: Vec3) {
        self.active_axis = self.pick(center, o, rd);
    }

    pub(crate) fn begin_drag(
        &mut self,
        axis: usize,
        slice_center: Vec3,
        slice_rot: Vec3,
        o: Vec3,
        rd: Vec3,
    ) {
        self.drag = Some((self.mode, axis));
        self.active_axis = Some(axis);
        self.drag_start_center = slice_center;
        match self.mode {
            GizmoMode::Move => {
                self.drag_start_t = axis_param(slice_center, axis_unit(axis), o, rd);
            }
            GizmoMode::Rotate => {
                self.drag_start_rot = slice_rot;
                self.drag_start_angle = ring_angle(slice_center, axis, o, rd);
            }
        }
    }

    pub(crate) fn update_drag(&self, o: Vec3, rd: Vec3) -> Option<DragUpdate> {
        let (mode, axis) = self.drag?;
        match mode {
            GizmoMode::Move => {
                let dir = axis_unit(axis);
                let t = axis_param(self.drag_start_center, dir, o, rd);
                let center = self.drag_start_center + dir * (t - self.drag_start_t);
                Some(DragUpdate::Translate(center.to_array()))
            }
            GizmoMode::Rotate => {
                let delta = (ring_angle(self.drag_start_center, axis, o, rd)
                    - self.drag_start_angle)
                    .to_degrees();
                let mut rot = self.drag_start_rot;
                rot[axis] = self.drag_start_rot[axis] + delta;
                Some(DragUpdate::Rotate(rot.to_array()))
            }
        }
    }

    pub(crate) fn end_drag(&mut self) {
        self.drag = None;
    }
}
