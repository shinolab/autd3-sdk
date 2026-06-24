use crate::geometry::Point3;
use crate::value::{Intensity, Phase};

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
pub struct ControlPoints<const N: usize> {
    pub points: [ControlPoint; N],
    pub intensity: Intensity,
}

impl<const N: usize> ControlPoints<N> {
    #[must_use]
    pub const fn new(points: [ControlPoint; N], intensity: Intensity) -> Self {
        Self { points, intensity }
    }
}

impl From<Point3<f32>> for ControlPoints<1> {
    fn from(point: Point3<f32>) -> Self {
        Self {
            points: [ControlPoint::from(point)],
            intensity: Intensity::MAX,
        }
    }
}

impl From<ControlPoint> for ControlPoints<1> {
    fn from(point: ControlPoint) -> Self {
        Self {
            points: [point],
            intensity: Intensity::MAX,
        }
    }
}

impl<const N: usize> From<[Point3<f32>; N]> for ControlPoints<N> {
    fn from(points: [Point3<f32>; N]) -> Self {
        Self {
            points: points.map(ControlPoint::from),
            intensity: Intensity::MAX,
        }
    }
}
