use autd3_rs_core::geometry::Point3;

use crate::amp::Amplitude;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ControlPoint {
    pub point: Point3<f32>,
    pub amplitude: Amplitude,
}
