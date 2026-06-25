use glam::{EulerRot, Mat3, Mat4, Quat, Vec2, Vec3};

fn quat_to_euler_deg(q: Quat) -> Vec3 {
    let (x, y, z) = q.to_euler(EulerRot::XYZ);
    Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees())
}

fn look_euler(pos: Vec3, pivot: Vec3) -> Vec3 {
    if (pivot - pos).length() < 1e-4 {
        return Vec3::ZERO;
    }
    let view = Mat4::look_at_rh(pos, pivot, Vec3::Z);
    let world_rot = Mat3::from_mat4(view).transpose();
    quat_to_euler_deg(Quat::from_mat3(&world_rot))
}

#[derive(Clone, Copy)]
pub(crate) struct Camera {
    pub(crate) pos: Vec3,
    pub(crate) rot: Vec3,
    pub(crate) pivot: Vec3,
    pub(crate) fov: f32,
    pub(crate) near: f32,
    pub(crate) far: f32,
    pub(crate) move_speed: f32,
    pub(crate) free: bool,
}

impl Camera {
    fn quat(&self) -> Quat {
        Quat::from_euler(
            EulerRot::XYZ,
            self.rot.x.to_radians(),
            self.rot.y.to_radians(),
            self.rot.z.to_radians(),
        )
    }

    pub(crate) fn eye(&self) -> Vec3 {
        self.pos
    }

    fn forward(&self) -> Vec3 {
        (self.quat() * Vec3::NEG_Z).normalize_or_zero()
    }

    pub(crate) fn distance(&self) -> f32 {
        (self.pos - self.pivot).length()
    }

    pub(crate) fn aim_at_pivot(&mut self) {
        self.rot = look_euler(self.pos, self.pivot);
    }

    pub(crate) fn view_proj(&self, aspect: f32) -> Mat4 {
        let view = (Mat4::from_translation(self.pos) * Mat4::from_quat(self.quat())).inverse();
        let proj =
            Mat4::perspective_rh(self.fov.to_radians(), aspect.max(0.01), self.near, self.far);
        proj * view
    }

    pub(crate) fn ray(&self, ndc: Vec2, aspect: f32) -> (Vec3, Vec3) {
        let inv = self.view_proj(aspect).inverse();
        let near = inv.project_point3(Vec3::new(ndc.x, ndc.y, 0.0));
        let far = inv.project_point3(Vec3::new(ndc.x, ndc.y, 1.0));
        (near, (far - near).normalize_or_zero())
    }

    pub(crate) fn orbit(&mut self, dx: f32, dy: f32) {
        if self.free {
            let mut q = Quat::from_axis_angle(Vec3::Z, -dx * 0.005) * self.quat();
            let right = (q * Vec3::X).normalize_or_zero();
            q = Quat::from_axis_angle(right, dy * 0.005) * q;
            self.rot = quat_to_euler_deg(q);
        } else {
            let offset = self.pos - self.pivot;
            let dist = offset.length().max(1.0);
            let mut yaw = offset.y.atan2(offset.x);
            let mut pitch = (offset.z / dist).clamp(-1.0, 1.0).asin();
            yaw -= dx * 0.005;
            pitch = (pitch + dy * 0.005).clamp(-1.4, 1.4);
            let (sp, cp) = pitch.sin_cos();
            let (sy, cy) = yaw.sin_cos();
            self.pos = self.pivot + Vec3::new(cp * cy, cp * sy, sp) * dist;
            self.aim_at_pivot();
        }
    }

    pub(crate) fn dolly(&mut self, delta: f32) {
        let step = -delta * self.move_speed;
        if self.free {
            self.pos += self.forward() * step;
        } else {
            let offset = self.pos - self.pivot;
            let dist = (offset.length() - step).clamp(30.0, 8000.0);
            self.pos = self.pivot + offset.normalize_or_zero() * dist;
            self.aim_at_pivot();
        }
    }
}
