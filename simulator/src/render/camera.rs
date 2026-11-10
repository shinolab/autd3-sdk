use glam::{
    EulerRot, Mat3, Mat4, Quat, Vec2, Vec3,
    camera::rh::{proj::directx::perspective, view::look_at_mat4},
};

fn quat_to_euler_deg(q: Quat) -> Vec3 {
    let (x, y, z) = q.to_euler(EulerRot::XYZ);
    Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees())
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

    pub(crate) fn aim_at_pivot(&mut self) {
        fn look_euler(pos: Vec3, pivot: Vec3) -> Vec3 {
            if (pivot - pos).length() < 1e-4 {
                return Vec3::ZERO;
            }
            let view = look_at_mat4(pos, pivot, Vec3::Z);
            let world_rot = Mat3::from_mat4(view).transpose();
            quat_to_euler_deg(Quat::from_mat3(&world_rot))
        }

        self.rot = look_euler(self.pos, self.pivot);
    }

    pub(crate) fn view_proj(&self, aspect: f32) -> Mat4 {
        let view = (Mat4::from_translation(self.pos) * Mat4::from_quat(self.quat())).inverse();
        let proj = perspective(self.fov.to_radians(), aspect.max(0.01), self.near, self.far);
        proj * view
    }

    pub(crate) fn ray(&self, ndc: Vec2, aspect: f32) -> (Vec3, Vec3) {
        let inv = self.view_proj(aspect).inverse();
        let near = inv.project_point3(Vec3::new(ndc.x, ndc.y, 0.0));
        let far = inv.project_point3(Vec3::new(ndc.x, ndc.y, 1.0));
        (near, (far - near).normalize_or_zero())
    }

    pub(crate) fn orbit(&mut self, dx: f32, dy: f32) {
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

    pub(crate) fn pan(&mut self, dx: f32, dy: f32) {
        let q = self.quat();
        let scale = (self.pos - self.pivot).length() * 2.0 * (self.fov.to_radians() * 0.5).tan();
        let shift = (q * Vec3::NEG_X * dx + q * Vec3::Y * dy) * scale;
        self.pos += shift;
        self.pivot += shift;
    }

    pub(crate) fn dolly(&mut self, delta: f32) {
        let step = -delta * self.move_speed;
        let offset = self.pos - self.pivot;
        let dist = (offset.length() - step).clamp(30.0, 8000.0);
        self.pos = self.pivot + offset.normalize_or_zero() * dist;
        self.aim_at_pivot();
    }

    pub(crate) fn set_pos(&mut self, pos: Vec3) {
        self.pivot += pos - self.pos;
        self.pos = pos;
    }

    pub(crate) fn set_rot(&mut self, rot: Vec3) {
        let dist = (self.pos - self.pivot).length();
        self.rot = rot;
        self.pivot = self.pos + self.quat() * Vec3::NEG_Z * dist;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera() -> Camera {
        let mut c = Camera {
            pos: Vec3::new(0.0, -600.0, 180.0),
            rot: Vec3::ZERO,
            pivot: Vec3::new(10.0, 20.0, 30.0),
            fov: 60.0,
            near: 1.0,
            far: 5000.0,
            move_speed: 1.5,
        };
        c.aim_at_pivot();
        c
    }

    fn pivot_on_view_axis(c: &Camera) -> bool {
        let (_, dir) = c.ray(Vec2::ZERO, 1.0);
        dir.dot((c.pivot - c.pos).normalize()) > 0.9999
    }

    #[test]
    fn orbit_keeps_distance_and_aims_at_pivot() {
        let mut c = camera();
        let dist = (c.pos - c.pivot).length();
        c.orbit(120.0, -80.0);
        assert!(((c.pos - c.pivot).length() - dist).abs() < 1e-2);
        assert!(pivot_on_view_axis(&c));
    }

    #[test]
    fn pan_moves_pivot_with_camera() {
        let mut c = camera();
        let offset = c.pos - c.pivot;
        let rot = c.rot;
        c.pan(0.1, 0.05);
        assert!((c.pos - c.pivot - offset).length() < 1e-3);
        assert!((c.rot - rot).length() < 1e-4);
        assert!(pivot_on_view_axis(&c));
    }

    #[test]
    fn set_rot_keeps_pivot_in_front() {
        let mut c = camera();
        let dist = (c.pos - c.pivot).length();
        c.set_rot(Vec3::new(70.0, 10.0, -30.0));
        assert!(((c.pos - c.pivot).length() - dist).abs() < 1e-2);
        assert!(pivot_on_view_axis(&c));
        c.orbit(5.0, 5.0);
        assert!(pivot_on_view_axis(&c));
    }
}
