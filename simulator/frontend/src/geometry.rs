use glam::{EulerRot, Quat, Vec3};

use autd3_rs_simulator_protocol::{DEVICE_WIDTH, DevicePose, MAX_DEVICES, ROTATION_NORM_TOLERANCE};

const ORIGIN_TOLERANCE_MM: f32 = 1e-3;
const ROTATION_TOLERANCE: f32 = 1e-4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DeviceDraft {
    pub origin: [f32; 3],
    pub euler_zyz_deg: [f32; 3],
}

fn quat_of(pose: &DevicePose) -> Quat {
    let [w, x, y, z] = pose.rotation;
    Quat::from_xyzw(x, y, z, w).normalize()
}

fn pose_matches(a: &DevicePose, b: &DevicePose) -> bool {
    let origin_matches = a
        .origin
        .iter()
        .zip(&b.origin)
        .all(|(a, b)| (a - b).abs() < ORIGIN_TOLERANCE_MM);
    let qa = quat_of(a);
    let qb = quat_of(b);
    let qb = if qa.dot(qb) < 0.0 { -qb } else { qb };
    origin_matches && qa.abs_diff_eq(qb, ROTATION_TOLERANCE)
}

impl DeviceDraft {
    pub(crate) fn from_pose(pose: &DevicePose) -> Self {
        let (a, b, c) = quat_of(pose).to_euler(EulerRot::ZYZ);
        Self {
            origin: pose.origin,
            euler_zyz_deg: [a.to_degrees(), b.to_degrees(), c.to_degrees()],
        }
    }

    fn rotation(self) -> Quat {
        let [a, b, c] = self.euler_zyz_deg.map(f32::to_radians);
        Quat::from_euler(EulerRot::ZYZ, a, b, c)
    }

    pub(crate) fn to_pose(self) -> DevicePose {
        let q = self.rotation();
        DevicePose {
            origin: self.origin,
            rotation: [q.w, q.x, q.y, q.z],
        }
    }

    fn matches(self, pose: &DevicePose) -> bool {
        pose_matches(&self.to_pose(), pose)
    }

    pub(crate) fn next_to(self) -> Self {
        let offset = self.rotation() * Vec3::new(DEVICE_WIDTH, 0.0, 0.0);
        Self {
            origin: (Vec3::from_array(self.origin) + offset).to_array(),
            euler_zyz_deg: self.euler_zyz_deg,
        }
    }
}

pub(crate) fn merge(drafts: &[DeviceDraft], poses: &[DevicePose]) -> Vec<DeviceDraft> {
    poses
        .iter()
        .enumerate()
        .map(|(i, pose)| match drafts.get(i) {
            Some(draft) if draft.matches(pose) => *draft,
            _ => DeviceDraft::from_pose(pose),
        })
        .collect()
}

pub(crate) fn to_poses(drafts: &[DeviceDraft]) -> Vec<DevicePose> {
    drafts.iter().copied().map(DeviceDraft::to_pose).collect()
}

pub(crate) fn to_file(drafts: &[DeviceDraft]) -> Result<String, String> {
    serde_json::to_string_pretty(&to_poses(drafts)).map_err(|e| e.to_string())
}

pub(crate) fn from_file(text: &str) -> Result<Vec<DeviceDraft>, String> {
    let poses: Vec<DevicePose> =
        serde_json::from_str(text).map_err(|e| format!("not a geometry file: {e}"))?;
    if !(1..=MAX_DEVICES).contains(&poses.len()) {
        return Err(format!(
            "the number of devices must be 1..={MAX_DEVICES}, but {} were given",
            poses.len()
        ));
    }
    for (device, pose) in poses.iter().enumerate() {
        if pose
            .origin
            .iter()
            .chain(&pose.rotation)
            .any(|v| !v.is_finite())
        {
            return Err(format!(
                "device {device} has a non-finite origin or rotation"
            ));
        }
        let norm = pose.rotation.iter().map(|v| v * v).sum::<f32>().sqrt();
        if (norm - 1.0).abs() > ROTATION_NORM_TOLERANCE {
            return Err(format!(
                "the rotation of device {device} must be a unit quaternion, but its norm is {norm}"
            ));
        }
    }
    Ok(poses.iter().map(DeviceDraft::from_pose).collect())
}

#[derive(Default)]
pub(crate) struct UnansweredGeometries(Vec<Vec<DevicePose>>);

impl UnansweredGeometries {
    pub(crate) fn record(&mut self, sent: Vec<DevicePose>) {
        self.0.push(sent);
    }

    pub(crate) fn consume_broadcast_is_current(&mut self, received: &[DevicePose]) -> bool {
        if let Some(answered) = self.0.iter().position(|sent| {
            sent.len() == received.len()
                && sent.iter().zip(received).all(|(a, b)| pose_matches(a, b))
        }) {
            self.0.drain(..=answered);
            self.0.is_empty()
        } else {
            self.0.clear();
            true
        }
    }
}
