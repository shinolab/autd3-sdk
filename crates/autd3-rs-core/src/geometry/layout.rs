use nalgebra::{Point3, Quaternion, UnitQuaternion};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

use super::{Autd3, Device, Geometry};

const ROTATION_NORM_TOLERANCE: f32 = 1e-3;

#[derive(Debug, Error)]
#[error("failed to convert the geometry layout: {0}")]
pub struct LayoutError(#[from] serde_json::Error);

const fn identity_rotation() -> [f32; 4] {
    [1.0, 0.0, 0.0, 0.0]
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Autd3Repr {
    origin: [f32; 3],
    #[serde(default = "identity_rotation")]
    rotation: [f32; 4],
}

impl Serialize for Autd3 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Autd3Repr {
            origin: [self.origin.x, self.origin.y, self.origin.z],
            rotation: [
                self.rotation.w,
                self.rotation.i,
                self.rotation.j,
                self.rotation.k,
            ],
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Autd3 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let repr = Autd3Repr::deserialize(deserializer)?;
        let rotation = Quaternion::new(
            repr.rotation[0],
            repr.rotation[1],
            repr.rotation[2],
            repr.rotation[3],
        );
        let norm = rotation.norm();
        if (norm - 1.0).abs() > ROTATION_NORM_TOLERANCE {
            return Err(D::Error::custom(format!(
                "`rotation` must be a unit quaternion [w, x, y, z], but its norm is {norm}"
            )));
        }
        Ok(Self::new(
            Point3::new(repr.origin[0], repr.origin[1], repr.origin[2]),
            UnitQuaternion::from_quaternion(rotation),
        ))
    }
}

impl From<&Device> for Autd3 {
    fn from(device: &Device) -> Self {
        Self::new(device.position(0), device.rotation())
    }
}

impl Geometry {
    pub fn from_json(json: &str) -> Result<Self, LayoutError> {
        Ok(Self::new(serde_json::from_str::<Vec<Autd3>>(json)?))
    }

    pub fn to_json(&self) -> Result<String, LayoutError> {
        let devices: Vec<Autd3> = self.iter().map(Autd3::from).collect();
        Ok(serde_json::to_string_pretty(&devices)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CROSS_LANGUAGE_FIXTURE: &str = r#"[
  { "origin": [0.0, 0.0, 0.0] },
  { "origin": [192.0, 0.0, 0.0], "rotation": [0.7071068, 0.0, 0.7071068, 0.0] }
]"#;

    fn sample() -> Geometry {
        Geometry::new(vec![
            Autd3::default(),
            Autd3::new(
                Point3::new(192.0, 0.0, 0.0),
                UnitQuaternion::from_axis_angle(
                    &nalgebra::Vector3::y_axis(),
                    core::f32::consts::FRAC_PI_2,
                ),
            ),
        ])
    }

    fn assert_geometry_eq(expected: &Geometry, actual: &Geometry) {
        assert_eq!(expected.num_devices(), actual.num_devices());
        for (expected, actual) in expected.iter().zip(actual.iter()) {
            approx::assert_abs_diff_eq!(expected.position(0), actual.position(0), epsilon = 1e-6);
            approx::assert_abs_diff_eq!(
                expected.rotation().angle_to(&actual.rotation()),
                0.0,
                epsilon = 1e-6
            );
        }
    }

    #[test]
    fn a_geometry_serializes_to_the_documented_shape() {
        let geometry = Geometry::new(vec![Autd3::new(
            Point3::new(1.0, 2.0, 3.0),
            UnitQuaternion::identity(),
        )]);
        assert_eq!(
            serde_json::json!([
                { "origin": [1.0, 2.0, 3.0], "rotation": [1.0, 0.0, 0.0, 0.0] }
            ]),
            serde_json::from_str::<serde_json::Value>(&geometry.to_json().unwrap()).unwrap()
        );
    }

    #[test]
    fn json_round_trip_preserves_the_placement() {
        let geometry = sample();
        let restored = Geometry::from_json(&geometry.to_json().unwrap()).unwrap();
        assert_geometry_eq(&geometry, &restored);
    }

    #[test]
    fn rotation_defaults_to_the_identity_quaternion() {
        let geometry = Geometry::from_json(r#"[{"origin":[0,0,0]}]"#).unwrap();
        assert_eq!(UnitQuaternion::identity(), geometry[0].rotation());
    }

    #[test]
    fn the_cross_language_fixture_places_the_devices_identically() {
        let geometry = Geometry::from_json(CROSS_LANGUAGE_FIXTURE).unwrap();
        assert_eq!(2, geometry.num_devices());
        assert_eq!(0, geometry[0].idx());
        assert_eq!(1, geometry[1].idx());
        assert_eq!(Point3::origin(), geometry[0].position(0));
        assert_eq!(Point3::new(192.0, 0.0, 0.0), geometry[1].position(0));
        approx::assert_abs_diff_eq!(
            Point3::new(192.0, 0.0, -Autd3::PITCH_MM),
            geometry[1].position(1),
            epsilon = 1e-3
        );
        approx::assert_abs_diff_eq!(
            nalgebra::Vector3::x(),
            geometry[1].axial_direction().into_inner(),
            epsilon = 1e-3
        );
    }

    #[test]
    fn an_unknown_field_is_rejected() {
        let err = Geometry::from_json(r#"[{"origin":[0,0,0],"rotaton":[1,0,0,0]}]"#)
            .expect_err("must fail");
        assert!(err.to_string().contains("rotaton"), "{err}");
    }

    #[test]
    fn a_non_normalized_rotation_is_rejected() {
        let err = Geometry::from_json(r#"[{"origin":[0,0,0],"rotation":[1,1,0,0]}]"#)
            .expect_err("must fail");
        assert!(err.to_string().contains("unit quaternion"), "{err}");
    }

    #[test]
    fn a_zero_rotation_is_rejected() {
        let err = Geometry::from_json(r#"[{"origin":[0,0,0],"rotation":[0,0,0,0]}]"#)
            .expect_err("must fail");
        assert!(err.to_string().contains("unit quaternion"), "{err}");
    }

    #[test]
    fn a_wrong_length_origin_is_rejected() {
        let err = Geometry::from_json(r#"[{"origin":[0,0]}]"#).expect_err("must fail");
        assert!(err.to_string().contains("length"), "{err}");
    }

    #[test]
    fn a_wrong_length_rotation_is_rejected() {
        let err = Geometry::from_json(r#"[{"origin":[0,0,0],"rotation":[1,0,0]}]"#)
            .expect_err("must fail");
        assert!(err.to_string().contains("length"), "{err}");
    }
}
