use core::f32::consts::TAU;

use super::{Focus, FocusKind, Intensity, Phase};
use crate::common::{Angle, Length};
use crate::geometry::{Device, Point3, UnitVector3, Vector3};
use autd3_cpu_wire::layout::FOCUS_UNIT_MM;

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn to_fixed(mm: f32) -> i32 {
    if mm.is_nan() {
        i32::MAX
    } else {
        (mm / FOCUS_UNIT_MM).round() as i32
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ControlPoint {
    pub point: Point3<f32>,
    pub phase_offset: Phase,
}

impl ControlPoint {
    #[must_use]
    pub const fn new(point: Point3<f32>, phase_offset: Phase) -> Self {
        Self {
            point,
            phase_offset,
        }
    }
}

impl From<Point3<f32>> for ControlPoint {
    fn from(point: Point3<f32>) -> Self {
        Self {
            point,
            phase_offset: Phase::ZERO,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Beam {
    Focus(ControlPoint),
    Bessel {
        apex: Point3<f32>,
        direction: UnitVector3<f32>,
        theta: Angle,
        wavelength: Length,
        phase_offset: Phase,
    },
    Plane {
        direction: UnitVector3<f32>,
        wavelength: Length,
    },
    Vortex {
        center: Point3<f32>,
        axis: UnitVector3<f32>,
        l: i8,
    },
}

impl Beam {
    #[must_use]
    pub const fn is_additive(&self) -> bool {
        matches!(self, Self::Plane { .. } | Self::Vortex { .. })
    }

    const fn phase_offset(&self) -> Phase {
        match self {
            Self::Focus(cp) => cp.phase_offset,
            Self::Bessel { phase_offset, .. } => *phase_offset,
            Self::Plane { .. } | Self::Vortex { .. } => Phase::ZERO,
        }
    }
}

impl From<ControlPoint> for Beam {
    fn from(cp: ControlPoint) -> Self {
        Self::Focus(cp)
    }
}

impl From<Point3<f32>> for Beam {
    fn from(point: Point3<f32>) -> Self {
        Self::Focus(ControlPoint::from(point))
    }
}

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum BeamError {
    #[error("the {beam} axis must be parallel to the normal of device {device}")]
    AxisNotNormal { beam: &'static str, device: usize },

    #[error(
        "the Bessel beam radial wavelength (wavelength / cos(theta)) must be in (0, {max_mm}] mm, got {got_mm} mm"
    )]
    BesselRadialWavelengthOutOfRange { got_mm: f32, max_mm: f32 },
}

const BESSEL_DIVISOR_MAX: f32 = 65535.0;
const PLANE_FRACTION_BITS: i32 = 15;
const PLANE_CONSTANT_FRACTION_BITS: i32 = 10;
const AXIS_PARALLEL_MIN_COS: f32 = 1.0 - 1.0e-6;

#[allow(clippy::cast_possible_truncation)]
fn to_fixed_scaled(v: f32, scale: f32) -> i32 {
    if v.is_nan() {
        i32::MAX
    } else {
        (v * scale).round() as i32
    }
}

fn phase_units(wavelength: Length) -> f32 {
    256.0 * FOCUS_UNIT_MM / wavelength.mm()
}

#[allow(clippy::cast_possible_truncation)]
fn signed_18_phase(phase: f32) -> i32 {
    let scaled =
        (phase.rem_euclid(256.0) * (1 << PLANE_CONSTANT_FRACTION_BITS) as f32).round() as i32;
    (scaled + (1 << 17)).rem_euclid(1 << 18) - (1 << 17)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn phase_byte(phase: f32) -> u8 {
    (phase.rem_euclid(256.0).round() as u32 & 0xFF) as u8
}

fn normal_sign(
    axis: UnitVector3<f32>,
    device: &Device,
    beam: &'static str,
) -> Result<f32, BeamError> {
    let cos = axis.dot(&device.axial_direction());
    if cos.abs() >= AXIS_PARALLEL_MIN_COS {
        Ok(cos.signum())
    } else {
        Err(BeamError::AxisNotNormal {
            beam,
            device: device.idx(),
        })
    }
}

fn azimuth_origin(axis: UnitVector3<f32>) -> Vector3<f32> {
    let a = axis.into_inner();
    let seed = if a.x.abs() <= a.y.abs() && a.x.abs() <= a.z.abs() {
        Vector3::x()
    } else if a.y.abs() <= a.z.abs() {
        Vector3::y()
    } else {
        Vector3::z()
    };
    (seed - a * a.dot(&seed)).normalize()
}

#[derive(Clone, Debug, PartialEq)]
pub struct ControlPoints {
    pub points: Vec<Beam>,
    pub intensity: Intensity,
}

impl ControlPoints {
    #[must_use]
    pub fn new(points: impl IntoIterator<Item = impl Into<Beam>>, intensity: Intensity) -> Self {
        Self {
            points: points.into_iter().map(Into::into).collect(),
            intensity,
        }
    }

    #[must_use]
    pub fn num_additive(&self) -> usize {
        self.points.iter().filter(|b| b.is_additive()).count()
    }

    #[doc(hidden)]
    pub fn focus(&self, device: &Device, j: usize) -> Result<Focus, BeamError> {
        let beam = self.points[j];
        let byte = if j == 0 {
            self.intensity.0
        } else if beam.is_additive() {
            0
        } else {
            (beam.phase_offset() - self.points[0].phase_offset()).0
        };
        let rotation = device.rotation();
        let focus = |kind, x, y, z| Focus {
            kind,
            x,
            y,
            z,
            intensity_or_offset: byte,
        };
        Ok(match beam {
            Beam::Focus(cp) => {
                let p = device.to_local(cp.point);
                focus(
                    FocusKind::Point,
                    to_fixed(p.x),
                    to_fixed(p.y),
                    to_fixed(p.z),
                )
            }
            Beam::Bessel {
                apex,
                direction,
                theta,
                wavelength,
                ..
            } => {
                normal_sign(direction, device, "Bessel beam")?;
                let radial_mm = wavelength.mm() / theta.rad().cos();
                let divisor = 64.0 * radial_mm / FOCUS_UNIT_MM;
                if !(divisor.round() >= 1.0 && divisor.round() <= BESSEL_DIVISOR_MAX) {
                    return Err(BeamError::BesselRadialWavelengthOutOfRange {
                        got_mm: radial_mm,
                        max_mm: BESSEL_DIVISOR_MAX * FOCUS_UNIT_MM / 64.0,
                    });
                }
                let p = device.to_local(apex);
                focus(
                    FocusKind::Bessel,
                    to_fixed(p.x),
                    to_fixed(p.y),
                    to_fixed_scaled(divisor, 1.0),
                )
            }
            Beam::Plane {
                direction,
                wavelength,
            } => {
                let k = -phase_units(wavelength);
                let local = rotation.inverse() * direction;
                let scale = (1 << PLANE_FRACTION_BITS) as f32;
                let constant = k * direction.dot(&device.position(0).coords) / FOCUS_UNIT_MM + 0.5;
                focus(
                    FocusKind::Plane,
                    to_fixed_scaled(k * local.x, scale),
                    to_fixed_scaled(k * local.y, scale),
                    signed_18_phase(constant),
                )
            }
            Beam::Vortex { center, axis, l } => {
                let sign = normal_sign(axis, device, "vortex")?;
                let u = rotation.inverse() * azimuth_origin(axis);
                let charge = if sign < 0.0 { l.wrapping_neg() } else { l };
                let offset = -f32::from(charge) * u.y.atan2(u.x) / TAU * 256.0;
                let p = device.to_local(center);
                focus(
                    FocusKind::Vortex,
                    to_fixed(p.x),
                    to_fixed(p.y),
                    i32::from(phase_byte(offset)) << 8 | i32::from(charge.cast_unsigned()),
                )
            }
        })
    }
}

impl From<Beam> for ControlPoints {
    fn from(beam: Beam) -> Self {
        Self {
            points: vec![beam],
            intensity: Intensity::MAX,
        }
    }
}

impl From<Point3<f32>> for ControlPoints {
    fn from(point: Point3<f32>) -> Self {
        Self::from(ControlPoint::from(point))
    }
}

impl From<ControlPoint> for ControlPoints {
    fn from(point: ControlPoint) -> Self {
        Self::from(Beam::Focus(point))
    }
}

impl<const N: usize> From<[Point3<f32>; N]> for ControlPoints {
    fn from(points: [Point3<f32>; N]) -> Self {
        Self {
            points: points.map(Beam::from).into(),
            intensity: Intensity::MAX,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Autd3, UnitQuaternion};

    const LAMBDA: Length = Length::from_mm(8.5);

    fn device() -> Device {
        Autd3::default().into()
    }

    fn encode(beams: impl IntoIterator<Item = Beam>, j: usize) -> Result<Focus, BeamError> {
        ControlPoints::new(beams, Intensity(0xA5)).focus(&device(), j)
    }

    #[test]
    fn point_focus_keeps_the_legacy_word() {
        let f = encode([Beam::from(Point3::new(1.0, -2.0, 150.0))], 0).unwrap();
        assert_eq!(
            f,
            Focus {
                kind: FocusKind::Point,
                x: 40,
                y: -80,
                z: 6000,
                intensity_or_offset: 0xA5,
            }
        );
    }

    #[test]
    fn bessel_divides_by_the_radial_wavelength() {
        let theta = Angle::from_rad(0.5);
        let f = encode(
            [Beam::Bessel {
                apex: Point3::new(10.0, 20.0, 99.0),
                direction: Vector3::z_axis(),
                theta,
                wavelength: LAMBDA,
                phase_offset: Phase::ZERO,
            }],
            0,
        )
        .unwrap();
        assert_eq!(f.kind, FocusKind::Bessel);
        assert_eq!((f.x, f.y), (400, 800));
        assert_eq!(f.z, 24795, "64 * 8.5 mm / cos(0.5) in 25 um units");
    }

    #[test]
    fn bessel_rejects_a_radial_wavelength_the_divider_cannot_hold() {
        let bessel = |theta| Beam::Bessel {
            apex: Point3::origin(),
            direction: Vector3::z_axis(),
            theta: Angle::from_rad(theta),
            wavelength: LAMBDA,
            phase_offset: Phase::ZERO,
        };
        assert!(encode([bessel(1.2)], 0).is_ok());
        assert!(matches!(
            encode([bessel(1.5)], 0),
            Err(BeamError::BesselRadialWavelengthOutOfRange { .. })
        ));
        assert!(matches!(
            encode([bessel(2.0)], 0),
            Err(BeamError::BesselRadialWavelengthOutOfRange { .. })
        ));
    }

    #[test]
    fn plane_coefficients_are_phase_per_unit_in_q15() {
        let f = encode(
            [Beam::Plane {
                direction: Vector3::x_axis(),
                wavelength: LAMBDA,
            }],
            0,
        )
        .unwrap();
        assert_eq!(f.kind, FocusKind::Plane);
        assert_eq!(f.x, -24672, "-256 * 0.025 / 8.5 * 2^15");
        assert_eq!(f.y, 0);
        assert_eq!(
            f.z, 512,
            "half a phase step so the floor in the FPGA rounds"
        );
    }

    #[test]
    fn plane_constant_carries_the_device_origin() {
        let dev: Device = Autd3::new(Point3::new(8.5, 0.0, 0.0), UnitQuaternion::identity()).into();
        let f = ControlPoints::from(Beam::Plane {
            direction: Vector3::x_axis(),
            wavelength: LAMBDA,
        })
        .focus(&dev, 0)
        .unwrap();
        assert_eq!(f.z, 512, "one wavelength away wraps back to the same phase");

        let dev: Device =
            Autd3::new(Point3::new(4.25, 0.0, 0.0), UnitQuaternion::identity()).into();
        let f = ControlPoints::from(Beam::Plane {
            direction: Vector3::x_axis(),
            wavelength: LAMBDA,
        })
        .focus(&dev, 0)
        .unwrap();
        assert_eq!(f.z, -128 * 1024 + 512);
    }

    #[test]
    fn vortex_packs_the_charge_and_flips_it_against_the_normal() {
        let vortex = |axis, l| Beam::Vortex {
            center: Point3::new(5.0, 6.0, 100.0),
            axis,
            l,
        };
        let up = encode([vortex(Vector3::z_axis(), 3)], 0).unwrap();
        assert_eq!(up.kind, FocusKind::Vortex);
        assert_eq!((up.x, up.y), (200, 240));
        assert_eq!(up.z & 0xFF, 3);
        let down = encode([vortex(-Vector3::z_axis(), 3)], 0).unwrap();
        assert_eq!(down.z & 0xFF, 0xFD);
        assert!(matches!(
            encode(
                [vortex(
                    UnitVector3::new_normalize(Vector3::new(0.0, 0.1, 1.0)),
                    1
                )],
                0
            ),
            Err(BeamError::AxisNotNormal { device: 0, .. })
        ));
    }

    #[test]
    fn vortex_constant_follows_the_device_rotation_about_the_normal() {
        let rotated: Device = Autd3::new(
            Point3::origin(),
            UnitQuaternion::from_axis_angle(&Vector3::z_axis(), core::f32::consts::FRAC_PI_2),
        )
        .into();
        let f = ControlPoints::from(Beam::Vortex {
            center: Point3::origin(),
            axis: Vector3::z_axis(),
            l: 1,
        })
        .focus(&rotated, 0)
        .unwrap();
        assert_eq!(f.z, 64 << 8 | 1);
    }

    #[test]
    fn the_byte_is_intensity_first_then_offsets_relative_to_the_first_superposed_offset() {
        let beams = [
            Beam::Focus(ControlPoint::new(Point3::origin(), Phase(0x10))),
            Beam::Plane {
                direction: Vector3::z_axis(),
                wavelength: LAMBDA,
            },
            Beam::Bessel {
                apex: Point3::origin(),
                direction: Vector3::z_axis(),
                theta: Angle::ZERO,
                wavelength: LAMBDA,
                phase_offset: Phase(0x30),
            },
        ];
        let bytes: Vec<u8> = (0..3)
            .map(|j| encode(beams, j).unwrap().intensity_or_offset)
            .collect();
        assert_eq!(bytes, [0xA5, 0x00, 0x20]);

        let led_by_plane = [beams[1], beams[0]];
        assert_eq!(encode(led_by_plane, 1).unwrap().intensity_or_offset, 0x10);
    }
}
