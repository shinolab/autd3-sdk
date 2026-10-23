#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use std::ops::RangeInclusive;

use autd3_rs_core::geometry::Point3;

use crate::aabb::Aabb;
use crate::error::{EmulatorError, GridAxis, InvalidGrid};

pub(crate) mod sealed {
    use autd3_rs_core::geometry::Point3;

    use crate::error::EmulatorError;

    pub trait Sealed {
        fn points(&self) -> impl Iterator<Item = (f32, f32, f32)>;
        fn bounding_points(&self) -> impl Iterator<Item = Point3<f32>>;

        fn validate(&self) -> Result<(), EmulatorError> {
            Ok(())
        }
    }
}

use sealed::Sealed;

pub trait Range: Sealed {}

impl<T: Sealed> Range for T {}

pub(crate) trait RangeAabb {
    fn aabb(&self) -> Aabb;
}

impl<T: Sealed> RangeAabb for T {
    fn aabb(&self) -> Aabb {
        Aabb::from_points(self.bounding_points())
    }
}

impl Sealed for Point3<f32> {
    fn points(&self) -> impl Iterator<Item = (f32, f32, f32)> {
        std::iter::once((self.x, self.y, self.z))
    }

    fn bounding_points(&self) -> impl Iterator<Item = Point3<f32>> {
        std::iter::once(*self)
    }
}

impl Sealed for Vec<Point3<f32>> {
    fn points(&self) -> impl Iterator<Item = (f32, f32, f32)> {
        self.iter().map(|v| (v.x, v.y, v.z))
    }

    fn bounding_points(&self) -> impl Iterator<Item = Point3<f32>> {
        self.iter().copied()
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

impl Sealed for Grid {
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

    fn bounding_points(&self) -> impl Iterator<Item = Point3<f32>> {
        [
            Point3::new(*self.x.start(), *self.y.start(), *self.z.start()),
            Point3::new(*self.x.end(), *self.y.end(), *self.z.end()),
        ]
        .into_iter()
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
        assert!(Sealed::validate(&Point3::new(f32::NAN, 0.0, 0.0)).is_ok());
        assert!(Sealed::validate(&vec![Point3::new(0.0, 0.0, 0.0)]).is_ok());
    }

    const ORDERS: [(AxisOrder, [usize; 3]); 6] = [
        (AxisOrder::XYZ, [0, 1, 2]),
        (AxisOrder::XZY, [0, 2, 1]),
        (AxisOrder::YXZ, [1, 0, 2]),
        (AxisOrder::YZX, [1, 2, 0]),
        (AxisOrder::ZXY, [2, 0, 1]),
        (AxisOrder::ZYX, [2, 1, 0]),
    ];

    fn grid_of(axes: [RangeInclusive<f32>; 3], order: AxisOrder) -> Grid {
        let [x, y, z] = axes;
        Grid {
            x,
            y,
            z,
            resolution: 1.0,
            order,
        }
    }

    fn nested_loops(
        values: [&[f32]; 3],
        [inner, middle, outer]: [usize; 3],
    ) -> Vec<(f32, f32, f32)> {
        let mut out = Vec::new();
        for &c in values[outer] {
            for &b in values[middle] {
                for &a in values[inner] {
                    let mut p = [0.0; 3];
                    p[inner] = a;
                    p[middle] = b;
                    p[outer] = c;
                    out.push((p[0], p[1], p[2]));
                }
            }
        }
        out
    }

    fn assert_enumerates(axes: &[RangeInclusive<f32>; 3], values: [&[f32]; 3]) {
        for (order, loops) in ORDERS {
            let g = grid_of(axes.clone(), order);
            assert_eq!(
                g.points().collect::<Vec<_>>(),
                nested_loops(values, loops),
                "{order:?}"
            );
            assert_eq!(
                g.aabb().min,
                Point3::new(*axes[0].start(), *axes[1].start(), *axes[2].start())
            );
            assert_eq!(
                g.aabb().max,
                Point3::new(*axes[0].end(), *axes[1].end(), *axes[2].end())
            );
        }
    }

    #[test]
    fn line_has_one_point_on_each_fixed_axis() {
        assert_enumerates(
            &[0.0..=3.0, 1.0..=1.0, 2.0..=2.0],
            [&[0., 1., 2., 3.], &[1.], &[2.]],
        );
        assert_enumerates(
            &[5.0..=5.0, -1.0..=1.0, 2.0..=2.0],
            [&[5.], &[-1., 0., 1.], &[2.]],
        );
        assert_enumerates(
            &[5.0..=5.0, 1.0..=1.0, 10.0..=12.0],
            [&[5.], &[1.], &[10., 11., 12.]],
        );
    }

    #[test]
    fn plane_follows_the_axis_order() {
        assert_enumerates(
            &[0.0..=1.0, 10.0..=12.0, 150.0..=150.0],
            [&[0., 1.], &[10., 11., 12.], &[150.]],
        );
        assert_enumerates(
            &[0.0..=1.0, 7.0..=7.0, 10.0..=12.0],
            [&[0., 1.], &[7.], &[10., 11., 12.]],
        );
        assert_enumerates(
            &[7.0..=7.0, 0.0..=1.0, 10.0..=12.0],
            [&[7.], &[0., 1.], &[10., 11., 12.]],
        );
    }

    #[test]
    fn volume_follows_the_axis_order() {
        assert_enumerates(
            &[0.0..=1.0, 10.0..=12.0, 100.0..=103.0],
            [&[0., 1.], &[10., 11., 12.], &[100., 101., 102., 103.]],
        );
    }

    #[test]
    fn first_axis_of_the_order_varies_fastest() {
        let xy = grid_of([0.0..=1.0, 0.0..=1.0, 0.0..=0.0], AxisOrder::XYZ);
        assert_eq!(
            xy.points().collect::<Vec<_>>(),
            vec![(0., 0., 0.), (1., 0., 0.), (0., 1., 0.), (1., 1., 0.)]
        );
        let yx = grid_of([0.0..=1.0, 0.0..=1.0, 0.0..=0.0], AxisOrder::YXZ);
        assert_eq!(
            yx.points().collect::<Vec<_>>(),
            vec![(0., 0., 0.), (0., 1., 0.), (1., 0., 0.), (1., 1., 0.)]
        );
        assert_eq!(AxisOrder::default(), AxisOrder::XYZ);
    }

    #[test]
    fn resolution_sets_the_step_and_drops_the_remainder() {
        let g = Grid {
            x: 0.0..=1.0,
            y: 3.0..=3.0,
            z: 0.0..=0.0,
            resolution: 0.4,
            order: AxisOrder::XYZ,
        };
        assert_eq!(
            g.points().collect::<Vec<_>>(),
            vec![(0.0, 3., 0.), (0.4, 3., 0.), (0.8, 3., 0.)]
        );
    }

    #[test]
    fn point_list_is_enumerated_as_given() {
        let points = vec![Point3::new(3., 1., 2.), Point3::new(0., 5., -1.)];
        assert_eq!(
            points.points().collect::<Vec<_>>(),
            vec![(3., 1., 2.), (0., 5., -1.)]
        );
        assert_eq!(points.aabb().min, Point3::new(0., 1., -1.));
        assert_eq!(points.aabb().max, Point3::new(3., 5., 2.));
    }
}
