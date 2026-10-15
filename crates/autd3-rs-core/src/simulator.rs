use nalgebra::{Point3, Quaternion, UnitQuaternion};
use thiserror::Error;

use crate::geometry::{Autd3, Geometry};

pub const DEFAULT_PORT: u16 = autd3_cpu_wire::udp::PORT;
pub const VERSION: u8 = 1;
pub const MAX_DEVICES: usize = 128;
pub const REPLY_BYTES: usize = 2;

const DEVICE_BYTES: usize = 28;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Status {
    Ok = 0,
    Invalid = 1,
    Failed = 2,
}

impl Status {
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub const fn from_u8(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::Ok),
            1 => Some(Self::Invalid),
            2 => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum DecodeError {
    #[error("the message is empty")]
    Empty,
    #[error("the message speaks version {got}, but this simulator speaks version {VERSION}")]
    UnsupportedVersion { got: u8 },
    #[error("the message body of {0} bytes is not a whole number of devices")]
    Truncated(usize),
    #[error("the number of devices must be 1..={MAX_DEVICES}, but {0} were given")]
    DeviceCountOutOfRange(usize),
    #[error("device {device} has a non-finite origin or rotation")]
    NonFinite { device: usize },
    #[error("the rotation of device {device} must be a unit quaternion, but its norm is {norm}")]
    RotationNotUnit { device: usize, norm: f32 },
}

#[must_use]
pub fn encode_geometry(geometry: &Geometry) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + DEVICE_BYTES * geometry.num_devices());
    out.push(VERSION);
    for device in geometry {
        let origin = device.position(0);
        let rotation = device.rotation();
        for value in [
            origin.x, origin.y, origin.z, rotation.w, rotation.i, rotation.j, rotation.k,
        ] {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
    out
}

pub fn decode_geometry(bytes: &[u8]) -> Result<Geometry, DecodeError> {
    let (&version, body) = bytes.split_first().ok_or(DecodeError::Empty)?;
    if version != VERSION {
        return Err(DecodeError::UnsupportedVersion { got: version });
    }
    let (devices, rest) = body.as_chunks::<DEVICE_BYTES>();
    if !rest.is_empty() {
        return Err(DecodeError::Truncated(body.len()));
    }
    if !(1..=MAX_DEVICES).contains(&devices.len()) {
        return Err(DecodeError::DeviceCountOutOfRange(devices.len()));
    }
    devices
        .iter()
        .enumerate()
        .map(|(device, raw)| {
            let (values, _) = raw.as_chunks::<4>();
            let v: [f32; 7] = core::array::from_fn(|i| f32::from_le_bytes(values[i]));
            if v.iter().any(|x| !x.is_finite()) {
                return Err(DecodeError::NonFinite { device });
            }
            let rotation = Quaternion::new(v[3], v[4], v[5], v[6]);
            let norm = rotation.norm();
            if (norm - 1.0).abs() > Autd3::ROTATION_NORM_TOLERANCE {
                return Err(DecodeError::RotationNotUnit { device, norm });
            }
            Ok(Autd3::new(
                Point3::new(v[0], v[1], v[2]),
                UnitQuaternion::from_quaternion(rotation),
            ))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Geometry::new)
}

#[must_use]
pub const fn control_port(port: u16) -> Option<u16> {
    match port {
        0 => None,
        _ => port.checked_add(1),
    }
}

#[must_use]
pub const fn encode_reply(status: Status) -> [u8; REPLY_BYTES] {
    [VERSION, status.as_u8()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Geometry {
        Geometry::new(vec![
            Autd3::default(),
            Autd3::new(
                Point3::new(192.0, -10.5, 300.0),
                UnitQuaternion::from_axis_angle(
                    &nalgebra::Vector3::y_axis(),
                    core::f32::consts::FRAC_PI_2,
                ),
            ),
        ])
    }

    #[test]
    fn the_control_port_follows_the_device_port() {
        assert_eq!(control_port(DEFAULT_PORT), Some(44337));
        assert_eq!(control_port(50000), Some(50001));
        assert_eq!(control_port(0), None);
        assert_eq!(control_port(u16::MAX), None);
    }

    #[test]
    fn a_geometry_round_trips() {
        let geometry = sample();
        let restored = decode_geometry(&encode_geometry(&geometry)).unwrap();
        assert_eq!(restored.num_devices(), geometry.num_devices());
        for (expected, actual) in geometry.iter().zip(restored.iter()) {
            approx::assert_abs_diff_eq!(expected.position(0), actual.position(0), epsilon = 1e-4);
            approx::assert_abs_diff_eq!(
                expected.rotation().angle_to(&actual.rotation()),
                0.0,
                epsilon = 1e-4
            );
            assert_eq!(expected.num_transducers(), actual.num_transducers());
        }
    }

    #[test]
    fn the_message_is_one_version_byte_and_28_bytes_per_device() {
        let bytes = encode_geometry(&sample());
        assert_eq!(bytes.len(), 1 + 2 * 28);
        assert_eq!(bytes[0], VERSION);
        assert_eq!(bytes[1..13], [0; 12]);
        assert_eq!(bytes[13..17], 1.0_f32.to_le_bytes());
    }

    #[test]
    fn an_empty_message_is_rejected() {
        assert_eq!(decode_geometry(&[]).unwrap_err(), DecodeError::Empty);
    }

    #[test]
    fn another_version_is_rejected() {
        let mut bytes = encode_geometry(&sample());
        bytes[0] = VERSION + 1;
        assert_eq!(
            decode_geometry(&bytes).unwrap_err(),
            DecodeError::UnsupportedVersion { got: VERSION + 1 }
        );
    }

    #[test]
    fn a_partial_device_is_rejected() {
        let mut bytes = encode_geometry(&sample());
        bytes.pop();
        assert_eq!(
            decode_geometry(&bytes).unwrap_err(),
            DecodeError::Truncated(2 * 28 - 1)
        );
    }

    #[test]
    fn no_device_is_rejected() {
        assert_eq!(
            decode_geometry(&[VERSION]).unwrap_err(),
            DecodeError::DeviceCountOutOfRange(0)
        );
    }

    #[test]
    fn too_many_devices_are_rejected() {
        let geometry = Geometry::new(vec![Autd3::default(); MAX_DEVICES + 1]);
        assert_eq!(
            decode_geometry(&encode_geometry(&geometry)).unwrap_err(),
            DecodeError::DeviceCountOutOfRange(MAX_DEVICES + 1)
        );
    }

    #[test]
    fn the_most_devices_fit() {
        let geometry = Geometry::new(vec![Autd3::default(); MAX_DEVICES]);
        assert_eq!(
            decode_geometry(&encode_geometry(&geometry))
                .unwrap()
                .num_devices(),
            MAX_DEVICES
        );
    }

    #[test]
    fn a_non_unit_rotation_is_rejected() {
        let mut bytes = encode_geometry(&sample());
        bytes[13..17].copy_from_slice(&2.0_f32.to_le_bytes());
        assert!(matches!(
            decode_geometry(&bytes).unwrap_err(),
            DecodeError::RotationNotUnit { device: 0, .. }
        ));
    }

    #[test]
    fn a_non_finite_value_is_rejected() {
        let mut bytes = encode_geometry(&sample());
        bytes[29..33].copy_from_slice(&f32::NAN.to_le_bytes());
        assert_eq!(
            decode_geometry(&bytes).unwrap_err(),
            DecodeError::NonFinite { device: 1 }
        );
    }

    #[test]
    fn every_status_round_trips() {
        for status in [Status::Ok, Status::Invalid, Status::Failed] {
            assert_eq!(Status::from_u8(status.as_u8()), Some(status));
            assert_eq!(encode_reply(status), [VERSION, status.as_u8()]);
        }
        assert_eq!(Status::from_u8(3), None);
    }
}
