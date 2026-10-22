use super::{Focus, Intensity, Phase};
use crate::geometry::{Device, Point3};
use autd3_cpu_wire::layout::FOCUS_UNIT_MM;

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn to_fixed(mm: f32) -> i32 {
    (mm / FOCUS_UNIT_MM).round() as i32
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

#[derive(Clone, Debug, PartialEq)]
pub struct ControlPoints {
    pub points: Vec<ControlPoint>,
    pub intensity: Intensity,
}

impl ControlPoints {
    #[must_use]
    pub fn new(points: impl Into<Vec<ControlPoint>>, intensity: Intensity) -> Self {
        Self {
            points: points.into(),
            intensity,
        }
    }

    #[doc(hidden)]
    #[must_use]
    pub fn focus(&self, device: &Device, j: usize) -> Focus {
        let cp = self.points[j];
        let p = device.to_local(cp.point);
        Focus {
            x: to_fixed(p.x),
            y: to_fixed(p.y),
            z: to_fixed(p.z),
            intensity_or_offset: if j == 0 {
                self.intensity.0
            } else {
                (cp.phase_offset - self.points[0].phase_offset).0
            },
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
        Self {
            points: vec![point],
            intensity: Intensity::MAX,
        }
    }
}

impl<const N: usize> From<[Point3<f32>; N]> for ControlPoints {
    fn from(points: [Point3<f32>; N]) -> Self {
        Self {
            points: points.map(ControlPoint::from).into(),
            intensity: Intensity::MAX,
        }
    }
}
