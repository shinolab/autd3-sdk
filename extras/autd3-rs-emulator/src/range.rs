#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use std::ops::RangeInclusive;

use autd3_rs_core::geometry::Point3;

use crate::aabb::Aabb;

pub trait Range {
    fn points(&self) -> impl Iterator<Item = (f32, f32, f32)>;
    fn aabb(&self) -> Aabb;
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

impl Range for Grid {
    fn points(&self) -> impl Iterator<Item = (f32, f32, f32)> {
        let res = self.resolution;
        let spec = [&self.x, &self.y, &self.z].map(|r| {
            (
                *r.start(),
                ((*r.end() - *r.start()) / res).floor() as usize + 1,
            )
        });
        let [inner, middle, outer] = self.order.axes();
        (0..spec[outer].1).flat_map(move |i2| {
            (0..spec[middle].1).flat_map(move |i1| {
                (0..spec[inner].1).map(move |i0| {
                    let mut p = [0.0; 3];
                    p[inner] = spec[inner].0 + res * i0 as f32;
                    p[middle] = spec[middle].0 + res * i1 as f32;
                    p[outer] = spec[outer].0 + res * i2 as f32;
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

    #[test]
    fn grid_aabb_is_well_formed_for_a_reversed_axis() {
        let grid = Grid {
            x: 1000.0..=100.0,
            y: 0.0..=0.0,
            z: 150.0..=150.0,
            resolution: 1.0,
            order: AxisOrder::XYZ,
        };

        assert_eq!(grid.points().count(), 1);
        let aabb = grid.aabb();
        assert_eq!(aabb.min, Point3::new(100.0, 0.0, 150.0));
        assert_eq!(aabb.max, Point3::new(1000.0, 0.0, 150.0));
    }
}
