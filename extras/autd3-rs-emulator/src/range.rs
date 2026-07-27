#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use std::ops::RangeInclusive;

use autd3_rs_core::geometry::{Point3, Vector3};

use crate::aabb::Aabb;

pub trait Range {
    fn points(&self) -> impl Iterator<Item = (f32, f32, f32)>;
    fn aabb(&self) -> Aabb;
}

fn n(start: f32, end: f32, resolution: f32) -> usize {
    ((end - start) / resolution).floor() as usize + 1
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
        self.iter().fold(Aabb::empty(), |aabb, v| aabb.grow(*v))
    }
}

impl Range for Vec<Vector3<f32>> {
    fn points(&self) -> impl Iterator<Item = (f32, f32, f32)> {
        self.iter().map(|v| (v.x, v.y, v.z))
    }

    fn aabb(&self) -> Aabb {
        self.iter()
            .fold(Aabb::empty(), |aabb, v| aabb.grow(Point3::from(*v)))
    }
}

#[derive(Clone, Debug)]
pub struct RangeX {
    pub x: RangeInclusive<f32>,
    pub y: f32,
    pub z: f32,
    pub resolution: f32,
}

impl Range for RangeX {
    fn points(&self) -> impl Iterator<Item = (f32, f32, f32)> {
        let nx = n(*self.x.start(), *self.x.end(), self.resolution);
        let x_start = *self.x.start();
        let (y, z, res) = (self.y, self.z, self.resolution);
        (0..nx).map(move |ix| (x_start + res * ix as f32, y, z))
    }

    fn aabb(&self) -> Aabb {
        Aabb {
            min: Vector3::new(*self.x.start(), self.y, self.z).into(),
            max: Vector3::new(*self.x.end(), self.y, self.z).into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RangeY {
    pub x: f32,
    pub y: RangeInclusive<f32>,
    pub z: f32,
    pub resolution: f32,
}

impl Range for RangeY {
    fn points(&self) -> impl Iterator<Item = (f32, f32, f32)> {
        let ny = n(*self.y.start(), *self.y.end(), self.resolution);
        let y_start = *self.y.start();
        let (x, z, res) = (self.x, self.z, self.resolution);
        (0..ny).map(move |iy| (x, y_start + res * iy as f32, z))
    }

    fn aabb(&self) -> Aabb {
        Aabb {
            min: Vector3::new(self.x, *self.y.start(), self.z).into(),
            max: Vector3::new(self.x, *self.y.end(), self.z).into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RangeZ {
    pub x: f32,
    pub y: f32,
    pub z: RangeInclusive<f32>,
    pub resolution: f32,
}

impl Range for RangeZ {
    fn points(&self) -> impl Iterator<Item = (f32, f32, f32)> {
        let nz = n(*self.z.start(), *self.z.end(), self.resolution);
        let z_start = *self.z.start();
        let (x, y, res) = (self.x, self.y, self.resolution);
        (0..nz).map(move |iz| (x, y, z_start + res * iz as f32))
    }

    fn aabb(&self) -> Aabb {
        Aabb {
            min: Vector3::new(self.x, self.y, *self.z.start()).into(),
            max: Vector3::new(self.x, self.y, *self.z.end()).into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RangeXY {
    pub x: RangeInclusive<f32>,
    pub y: RangeInclusive<f32>,
    pub z: f32,
    pub resolution: f32,
}

impl Range for RangeXY {
    fn points(&self) -> impl Iterator<Item = (f32, f32, f32)> {
        let nx = n(*self.x.start(), *self.x.end(), self.resolution);
        let ny = n(*self.y.start(), *self.y.end(), self.resolution);
        let (x0, y0, z, res) = (*self.x.start(), *self.y.start(), self.z, self.resolution);
        (0..ny).flat_map(move |iy| {
            let py = y0 + res * iy as f32;
            (0..nx).map(move |ix| (x0 + res * ix as f32, py, z))
        })
    }

    fn aabb(&self) -> Aabb {
        Aabb {
            min: Vector3::new(*self.x.start(), *self.y.start(), self.z).into(),
            max: Vector3::new(*self.x.end(), *self.y.end(), self.z).into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RangeXZ {
    pub x: RangeInclusive<f32>,
    pub y: f32,
    pub z: RangeInclusive<f32>,
    pub resolution: f32,
}

impl Range for RangeXZ {
    fn points(&self) -> impl Iterator<Item = (f32, f32, f32)> {
        let nx = n(*self.x.start(), *self.x.end(), self.resolution);
        let nz = n(*self.z.start(), *self.z.end(), self.resolution);
        let (x0, y, z0, res) = (*self.x.start(), self.y, *self.z.start(), self.resolution);
        (0..nz).flat_map(move |iz| {
            let pz = z0 + res * iz as f32;
            (0..nx).map(move |ix| (x0 + res * ix as f32, y, pz))
        })
    }

    fn aabb(&self) -> Aabb {
        Aabb {
            min: Vector3::new(*self.x.start(), self.y, *self.z.start()).into(),
            max: Vector3::new(*self.x.end(), self.y, *self.z.end()).into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RangeYX {
    pub x: RangeInclusive<f32>,
    pub y: RangeInclusive<f32>,
    pub z: f32,
    pub resolution: f32,
}

impl Range for RangeYX {
    fn points(&self) -> impl Iterator<Item = (f32, f32, f32)> {
        let nx = n(*self.x.start(), *self.x.end(), self.resolution);
        let ny = n(*self.y.start(), *self.y.end(), self.resolution);
        let (x0, y0, z, res) = (*self.x.start(), *self.y.start(), self.z, self.resolution);
        (0..nx).flat_map(move |ix| {
            let px = x0 + res * ix as f32;
            (0..ny).map(move |iy| (px, y0 + res * iy as f32, z))
        })
    }

    fn aabb(&self) -> Aabb {
        Aabb {
            min: Vector3::new(*self.x.start(), *self.y.start(), self.z).into(),
            max: Vector3::new(*self.x.end(), *self.y.end(), self.z).into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RangeYZ {
    pub x: f32,
    pub y: RangeInclusive<f32>,
    pub z: RangeInclusive<f32>,
    pub resolution: f32,
}

impl Range for RangeYZ {
    fn points(&self) -> impl Iterator<Item = (f32, f32, f32)> {
        let ny = n(*self.y.start(), *self.y.end(), self.resolution);
        let nz = n(*self.z.start(), *self.z.end(), self.resolution);
        let (x, y0, z0, res) = (self.x, *self.y.start(), *self.z.start(), self.resolution);
        (0..nz).flat_map(move |iz| {
            let pz = z0 + res * iz as f32;
            (0..ny).map(move |iy| (x, y0 + res * iy as f32, pz))
        })
    }

    fn aabb(&self) -> Aabb {
        Aabb {
            min: Vector3::new(self.x, *self.y.start(), *self.z.start()).into(),
            max: Vector3::new(self.x, *self.y.end(), *self.z.end()).into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RangeZX {
    pub x: RangeInclusive<f32>,
    pub y: f32,
    pub z: RangeInclusive<f32>,
    pub resolution: f32,
}

impl Range for RangeZX {
    fn points(&self) -> impl Iterator<Item = (f32, f32, f32)> {
        let nx = n(*self.x.start(), *self.x.end(), self.resolution);
        let nz = n(*self.z.start(), *self.z.end(), self.resolution);
        let (x0, y, z0, res) = (*self.x.start(), self.y, *self.z.start(), self.resolution);
        (0..nx).flat_map(move |ix| {
            let px = x0 + res * ix as f32;
            (0..nz).map(move |iz| (px, y, z0 + res * iz as f32))
        })
    }

    fn aabb(&self) -> Aabb {
        Aabb {
            min: Vector3::new(*self.x.start(), self.y, *self.z.start()).into(),
            max: Vector3::new(*self.x.end(), self.y, *self.z.end()).into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RangeZY {
    pub x: f32,
    pub y: RangeInclusive<f32>,
    pub z: RangeInclusive<f32>,
    pub resolution: f32,
}

impl Range for RangeZY {
    fn points(&self) -> impl Iterator<Item = (f32, f32, f32)> {
        let ny = n(*self.y.start(), *self.y.end(), self.resolution);
        let nz = n(*self.z.start(), *self.z.end(), self.resolution);
        let (x, y0, z0, res) = (self.x, *self.y.start(), *self.z.start(), self.resolution);
        (0..ny).flat_map(move |iy| {
            let py = y0 + res * iy as f32;
            (0..nz).map(move |iz| (x, py, z0 + res * iz as f32))
        })
    }

    fn aabb(&self) -> Aabb {
        Aabb {
            min: Vector3::new(self.x, *self.y.start(), *self.z.start()).into(),
            max: Vector3::new(self.x, *self.y.end(), *self.z.end()).into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RangeXYZ {
    pub x: RangeInclusive<f32>,
    pub y: RangeInclusive<f32>,
    pub z: RangeInclusive<f32>,
    pub resolution: f32,
}

#[derive(Clone, Debug)]
pub struct RangeXZY {
    pub x: RangeInclusive<f32>,
    pub y: RangeInclusive<f32>,
    pub z: RangeInclusive<f32>,
    pub resolution: f32,
}

#[derive(Clone, Debug)]
pub struct RangeYXZ {
    pub x: RangeInclusive<f32>,
    pub y: RangeInclusive<f32>,
    pub z: RangeInclusive<f32>,
    pub resolution: f32,
}

#[derive(Clone, Debug)]
pub struct RangeYZX {
    pub x: RangeInclusive<f32>,
    pub y: RangeInclusive<f32>,
    pub z: RangeInclusive<f32>,
    pub resolution: f32,
}

#[derive(Clone, Debug)]
pub struct RangeZXY {
    pub x: RangeInclusive<f32>,
    pub y: RangeInclusive<f32>,
    pub z: RangeInclusive<f32>,
    pub resolution: f32,
}

#[derive(Clone, Debug)]
pub struct RangeZYX {
    pub x: RangeInclusive<f32>,
    pub y: RangeInclusive<f32>,
    pub z: RangeInclusive<f32>,
    pub resolution: f32,
}

fn aabb_3d(x: &RangeInclusive<f32>, y: &RangeInclusive<f32>, z: &RangeInclusive<f32>) -> Aabb {
    Aabb {
        min: Vector3::new(*x.start(), *y.start(), *z.start()).into(),
        max: Vector3::new(*x.end(), *y.end(), *z.end()).into(),
    }
}

impl Range for RangeXYZ {
    fn points(&self) -> impl Iterator<Item = (f32, f32, f32)> {
        let nx = n(*self.x.start(), *self.x.end(), self.resolution);
        let ny = n(*self.y.start(), *self.y.end(), self.resolution);
        let nz = n(*self.z.start(), *self.z.end(), self.resolution);
        let (x0, y0, z0, res) = (
            *self.x.start(),
            *self.y.start(),
            *self.z.start(),
            self.resolution,
        );
        (0..nz).flat_map(move |iz| {
            let pz = z0 + res * iz as f32;
            (0..ny).flat_map(move |iy| {
                let py = y0 + res * iy as f32;
                (0..nx).map(move |ix| (x0 + res * ix as f32, py, pz))
            })
        })
    }

    fn aabb(&self) -> Aabb {
        aabb_3d(&self.x, &self.y, &self.z)
    }
}

impl Range for RangeXZY {
    fn points(&self) -> impl Iterator<Item = (f32, f32, f32)> {
        let nx = n(*self.x.start(), *self.x.end(), self.resolution);
        let ny = n(*self.y.start(), *self.y.end(), self.resolution);
        let nz = n(*self.z.start(), *self.z.end(), self.resolution);
        let (x0, y0, z0, res) = (
            *self.x.start(),
            *self.y.start(),
            *self.z.start(),
            self.resolution,
        );
        (0..ny).flat_map(move |iy| {
            let py = y0 + res * iy as f32;
            (0..nz).flat_map(move |iz| {
                let pz = z0 + res * iz as f32;
                (0..nx).map(move |ix| (x0 + res * ix as f32, py, pz))
            })
        })
    }

    fn aabb(&self) -> Aabb {
        aabb_3d(&self.x, &self.y, &self.z)
    }
}

impl Range for RangeYXZ {
    fn points(&self) -> impl Iterator<Item = (f32, f32, f32)> {
        let nx = n(*self.x.start(), *self.x.end(), self.resolution);
        let ny = n(*self.y.start(), *self.y.end(), self.resolution);
        let nz = n(*self.z.start(), *self.z.end(), self.resolution);
        let (x0, y0, z0, res) = (
            *self.x.start(),
            *self.y.start(),
            *self.z.start(),
            self.resolution,
        );
        (0..nz).flat_map(move |iz| {
            let pz = z0 + res * iz as f32;
            (0..nx).flat_map(move |ix| {
                let px = x0 + res * ix as f32;
                (0..ny).map(move |iy| (px, y0 + res * iy as f32, pz))
            })
        })
    }

    fn aabb(&self) -> Aabb {
        aabb_3d(&self.x, &self.y, &self.z)
    }
}

impl Range for RangeYZX {
    fn points(&self) -> impl Iterator<Item = (f32, f32, f32)> {
        let nx = n(*self.x.start(), *self.x.end(), self.resolution);
        let ny = n(*self.y.start(), *self.y.end(), self.resolution);
        let nz = n(*self.z.start(), *self.z.end(), self.resolution);
        let (x0, y0, z0, res) = (
            *self.x.start(),
            *self.y.start(),
            *self.z.start(),
            self.resolution,
        );
        (0..nx).flat_map(move |ix| {
            let px = x0 + res * ix as f32;
            (0..nz).flat_map(move |iz| {
                let pz = z0 + res * iz as f32;
                (0..ny).map(move |iy| (px, y0 + res * iy as f32, pz))
            })
        })
    }

    fn aabb(&self) -> Aabb {
        aabb_3d(&self.x, &self.y, &self.z)
    }
}

impl Range for RangeZXY {
    fn points(&self) -> impl Iterator<Item = (f32, f32, f32)> {
        let nx = n(*self.x.start(), *self.x.end(), self.resolution);
        let ny = n(*self.y.start(), *self.y.end(), self.resolution);
        let nz = n(*self.z.start(), *self.z.end(), self.resolution);
        let (x0, y0, z0, res) = (
            *self.x.start(),
            *self.y.start(),
            *self.z.start(),
            self.resolution,
        );
        (0..ny).flat_map(move |iy| {
            let py = y0 + res * iy as f32;
            (0..nx).flat_map(move |ix| {
                let px = x0 + res * ix as f32;
                (0..nz).map(move |iz| (px, py, z0 + res * iz as f32))
            })
        })
    }

    fn aabb(&self) -> Aabb {
        aabb_3d(&self.x, &self.y, &self.z)
    }
}

impl Range for RangeZYX {
    fn points(&self) -> impl Iterator<Item = (f32, f32, f32)> {
        let nx = n(*self.x.start(), *self.x.end(), self.resolution);
        let ny = n(*self.y.start(), *self.y.end(), self.resolution);
        let nz = n(*self.z.start(), *self.z.end(), self.resolution);
        let (x0, y0, z0, res) = (
            *self.x.start(),
            *self.y.start(),
            *self.z.start(),
            self.resolution,
        );
        (0..nx).flat_map(move |ix| {
            let px = x0 + res * ix as f32;
            (0..ny).flat_map(move |iy| {
                let py = y0 + res * iy as f32;
                (0..nz).map(move |iz| (px, py, z0 + res * iz as f32))
            })
        })
    }

    fn aabb(&self) -> Aabb {
        aabb_3d(&self.x, &self.y, &self.z)
    }
}
