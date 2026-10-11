#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use std::ops::RangeInclusive;

use autd3_rs_core::geometry::Point3;

use crate::aabb::Aabb;
use crate::error::{EmulatorError, GridAxis, InvalidGrid};

pub trait Range {
    fn points(&self) -> impl Iterator<Item = (f32, f32, f32)>;
    fn aabb(&self) -> Aabb;

    fn validate(&self) -> Result<(), EmulatorError> {
        Ok(())
    }
}

impl Range for Point3<f32> {
    fn points(&self) -> impl Iterator<Item = (f32, f32, f32)> {
        std::iter::once((self.x, self.y, self.z))
    }

    fn aabb(&self) -> Aabb {
        Aabb {
            min: *self,
            max: *self,
        }
    }
}

impl Range for Vec<Point3<f32>> {
    fn points(&self) -> impl Iterator<Item = (f32, f32, f32)> {
        self.iter().map(|v| (v.x, v.y, v.z))
    }

    fn aabb(&self) -> Aabb {
        Aabb::from_points(self.iter().copied())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AxisOrder {
    #[default]
    XYZ,
    XZY,
    YXZ,
    YZX,
    ZXY,
    ZYX,
}

impl AxisOrder {
    fn axes(self) -> [usize; 3] {
        match self {
            Self::XYZ => [0, 1, 2],
            Self::XZY => [0, 2, 1],
            Self::YXZ => [1, 0, 2],
            Self::YZX => [1, 2, 0],
            Self::ZXY => [2, 0, 1],
            Self::ZYX => [2, 1, 0],
        }
    }
}

#[derive(Clone, Debug)]
pub struct Grid {
    pub x: RangeInclusive<f32>,
    pub y: RangeInclusive<f32>,
    pub z: RangeInclusive<f32>,
    pub resolution: f32,
    pub order: AxisOrder,
}

impl Grid {
    fn axis_len(axis: &RangeInclusive<f32>, resolution: f32) -> Option<usize> {
        (((*axis.end() - *axis.start()) / resolution).floor() as usize).checked_add(1)
    }

    pub fn validate(&self) -> Result<(), InvalidGrid> {
        if !(self.resolution.is_finite() && self.resolution > 0.0) {
            return Err(InvalidGrid::Resolution);
        }
        let mut len = 1usize;
        for (axis, r) in [
            (GridAxis::X, &self.x),
            (GridAxis::Y, &self.y),
            (GridAxis::Z, &self.z),
        ] {
            if !(r.start().is_finite() && r.end().is_finite()) {
                return Err(InvalidGrid::NonFiniteBound(axis));
            }
            if r.start() > r.end() {
                return Err(InvalidGrid::Reversed(axis));
            }
            len = Self::axis_len(r, self.resolution)
                .and_then(|n| len.checked_mul(n))
                .ok_or(InvalidGrid::TooManyPoints)?;
        }
        Ok(())
    }
}

impl Range for Grid {
    fn validate(&self) -> Result<(), EmulatorError> {
        Ok(Grid::validate(self)?)
    }

    fn points(&self) -> impl Iterator<Item = (f32, f32, f32)> {
        let res = self.resolution;
        let spec = [&self.x, &self.y, &self.z].map(|r| {
            (
                *r.start(),
                *r.end(),
                Self::axis_len(r, res).unwrap_or_default(),
            )
        });
        let [inner, middle, outer] = self.order.axes();
        (0..spec[outer].2).flat_map(move |i2| {
            (0..spec[middle].2).flat_map(move |i1| {
                (0..spec[inner].2).map(move |i0| {
                    let mut p = [0.0; 3];
                    p[inner] = (spec[inner].0 + res * i0 as f32).min(spec[inner].1);
                    p[middle] = (spec[middle].0 + res * i1 as f32).min(spec[middle].1);
                    p[outer] = (spec[outer].0 + res * i2 as f32).min(spec[outer].1);
                    (p[0], p[1], p[2])
                })
            })
        })
    }

    fn aabb(&self) -> Aabb {
        Aabb::from_points([
            Point3::new(*self.x.start(), *self.y.start(), *self.z.start()),
            Point3::new(*self.x.end(), *self.y.end(), *self.z.end()),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(x: RangeInclusive<f32>, resolution: f32) -> Grid {
        Grid {
            x,
            y: 0.0..=0.0,
            z: 150.0..=150.0,
            resolution,
            order: AxisOrder::XYZ,
        }
    }

    #[test]
    fn rejects_a_resolution_that_is_not_finite_and_positive() {
        for resolution in [0.0, -0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                grid(0.0..=1.0, resolution).validate(),
                Err(InvalidGrid::Resolution),
                "{resolution}"
            );
            assert_eq!(
                grid(1.0..=1.0, resolution).validate(),
                Err(InvalidGrid::Resolution),
                "{resolution}"
            );
        }
    }

    #[test]
    fn rejects_a_reversed_axis() {
        assert_eq!(
            grid(1000.0..=100.0, 1.0).validate(),
            Err(InvalidGrid::Reversed(GridAxis::X))
        );
        let y = Grid {
            y: 1.0..=0.0,
            ..grid(0.0..=1.0, 1.0)
        };
        assert_eq!(y.validate(), Err(InvalidGrid::Reversed(GridAxis::Y)));
        let z = Grid {
            z: 1.0..=0.0,
            ..grid(0.0..=1.0, 1.0)
        };
        assert_eq!(z.validate(), Err(InvalidGrid::Reversed(GridAxis::Z)));
    }

    #[test]
    fn rejects_a_bound_that_is_not_finite() {
        for x in [
            f32::NAN..=1.0,
            0.0..=f32::NAN,
            f32::NEG_INFINITY..=1.0,
            0.0..=f32::INFINITY,
            f32::INFINITY..=1.0,
        ] {
            assert_eq!(
                grid(x.clone(), 1.0).validate(),
                Err(InvalidGrid::NonFiniteBound(GridAxis::X)),
                "{x:?}"
            );
        }
    }

    #[test]
    fn rejects_a_point_count_that_does_not_fit() {
        assert_eq!(
            grid(0.0..=f32::MAX, f32::MIN_POSITIVE).validate(),
            Err(InvalidGrid::TooManyPoints)
        );
        let volume = Grid {
            x: 0.0..=1e30,
            y: 0.0..=1e30,
            z: 0.0..=1e30,
            resolution: 1e20,
            order: AxisOrder::XYZ,
        };
        assert_eq!(volume.validate(), Err(InvalidGrid::TooManyPoints));
        assert_eq!(grid(0.0..=f32::MAX, f32::MIN_POSITIVE).points().count(), 0);
    }

    #[test]
    fn a_valid_grid_spans_its_aabb() {
        for (x, resolution, n) in [
            (100.0..=1000.0, 1.0, 901),
            (-1.0..=1.0, 0.5, 5),
            (7.0..=7.0, 1.0, 1),
            (7.0..=7.0, 1e-3, 1),
        ] {
            let grid = grid(x.clone(), resolution);
            assert_eq!(grid.validate(), Ok(()));
            let points: Vec<_> = grid.points().collect();
            assert_eq!(points.len(), n);
            let aabb = grid.aabb();
            assert_eq!(aabb.min, Point3::new(*x.start(), 0.0, 150.0));
            assert_eq!(aabb.max, Point3::new(*x.end(), 0.0, 150.0));
            assert_eq!(points[0], (*x.start(), 0.0, 150.0));
            assert_eq!(points[n - 1], (*x.end(), 0.0, 150.0));
        }
    }

    #[test]
    fn rejects_a_span_that_overflows() {
        assert_eq!(
            grid(-f32::MAX..=f32::MAX, 1.0).validate(),
            Err(InvalidGrid::TooManyPoints)
        );
    }

    #[test]
    fn points_stay_inside_the_aabb_when_the_step_rounds_up() {
        let grid = grid(-20.0..=-13.3, 0.1);
        assert_eq!(grid.validate(), Ok(()));
        let aabb = grid.aabb();
        let points: Vec<_> = grid.points().collect();
        assert_eq!(points.len(), 68);
        assert!(
            points
                .iter()
                .all(|&(x, _, _)| aabb.min.x <= x && x <= aabb.max.x)
        );
    }

    #[test]
    fn point_ranges_are_always_valid() {
        assert!(Range::validate(&Point3::new(f32::NAN, 0.0, 0.0)).is_ok());
        assert!(Range::validate(&vec![Point3::new(0.0, 0.0, 0.0)]).is_ok());
    }
}
