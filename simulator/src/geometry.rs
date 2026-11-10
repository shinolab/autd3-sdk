use autd3_rs::geometry::{Autd3, Geometry};
use glam::{EulerRot, Quat, Vec3};

use crate::emulator::{DevicePose, geometry_from_poses};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DeviceDraft {
    pub origin: [f32; 3],
    pub euler_zyz_deg: [f32; 3],
}

impl DeviceDraft {
    pub(crate) fn from_pose(pose: &DevicePose) -> Self {
        fn quat_of(pose: &DevicePose) -> Quat {
            let [w, x, y, z] = pose.rotation;
            Quat::from_xyzw(x, y, z, w).normalize()
        }

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

    pub(crate) fn next_to(self) -> Self {
        let offset = self.rotation() * Vec3::new(Autd3::DEVICE_WIDTH, 0.0, 0.0);
        Self {
            origin: (Vec3::from_array(self.origin) + offset).to_array(),
            euler_zyz_deg: self.euler_zyz_deg,
        }
    }
}

pub(crate) fn to_poses(drafts: &[DeviceDraft]) -> Vec<DevicePose> {
    drafts.iter().copied().map(DeviceDraft::to_pose).collect()
}

pub(crate) fn to_file(drafts: &[DeviceDraft]) -> Result<String, String> {
    serde_json::to_string_pretty(&to_poses(drafts)).map_err(|e| e.to_string())
}

pub(crate) fn from_file(text: &str) -> Result<(Vec<DeviceDraft>, Geometry), String> {
    let poses: Vec<DevicePose> =
        serde_json::from_str(text).map_err(|e| format!("not a geometry file: {e}"))?;
    let geometry = geometry_from_poses(&poses).map_err(|e| e.to_string())?;
    Ok((poses.iter().map(DeviceDraft::from_pose).collect(), geometry))
}

#[cfg(test)]
mod tests {
    use super::*;

    use approx::assert_relative_eq;

    #[test]
    fn a_file_saved_by_the_browser_frontend_loads() {
        let text = r#"[
  {
    "origin": [0.0, 0.0, 0.0],
    "rotation": [1.0, 0.0, 0.0, 0.0]
  },
  {
    "origin": [192.0, -10.5, 300.25],
    "rotation": [0.70710677, 0.0, 0.70710677, 0.0]
  }
]"#;
        let (drafts, geometry) = from_file(text).unwrap();
        assert_eq!(geometry.num_devices(), 2);
        assert_eq!(drafts[1].origin, [192.0, -10.5, 300.25]);
        assert_relative_eq!(drafts[1].euler_zyz_deg[1], 90.0, epsilon = 1e-3);
    }

    #[test]
    fn a_saved_file_loads_back() {
        let drafts = vec![
            DeviceDraft {
                origin: [0.0; 3],
                euler_zyz_deg: [0.0; 3],
            },
            DeviceDraft {
                origin: [10.0, 20.0, 30.0],
                euler_zyz_deg: [30.0, 45.0, 60.0],
            },
        ];
        let (loaded, _) = from_file(&to_file(&drafts).unwrap()).unwrap();
        for (expected, actual) in drafts.iter().zip(&loaded) {
            assert_eq!(expected.origin, actual.origin);
            assert_relative_eq!(
                expected.euler_zyz_deg[..],
                actual.euler_zyz_deg[..],
                epsilon = 1e-3
            );
        }
    }

    #[test]
    fn an_invalid_file_is_refused() {
        assert!(from_file("{}").is_err());
        assert!(from_file("[]").is_err());
        assert!(from_file(r#"[{"origin":[0,0,0],"rotation":[2,0,0,0]}]"#).is_err());
    }
}
