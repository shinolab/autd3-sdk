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

macro_rules! impl_range {
    (@ty range) => { RangeInclusive<f32> };
    (@ty scalar) => { f32 };
    (@spec range, $v:expr, $res:expr) => { (*$v.start(), n(*$v.start(), *$v.end(), $res)) };
    (@spec scalar, $v:expr, $res:expr) => { ($v, 1usize) };
    (@min range, $v:expr) => { *$v.start() };
    (@min scalar, $v:expr) => { $v };
    (@max range, $v:expr) => { *$v.end() };
    (@max scalar, $v:expr) => { $v };
    (
        $name:ident {
            $ax:ident: $kx:ident,
            $ay:ident: $ky:ident,
            $az:ident: $kz:ident $(,)?
        },
        order: [$a0:ident, $a1:ident, $a2:ident] $(,)?
    ) => {
        #[derive(Clone, Debug)]
        pub struct $name {
            pub $ax: impl_range!(@ty $kx),
            pub $ay: impl_range!(@ty $ky),
            pub $az: impl_range!(@ty $kz),
            pub resolution: f32,
        }

        impl Range for $name {
            fn points(&self) -> impl Iterator<Item = (f32, f32, f32)> {
                let res = self.resolution;
                let $ax = impl_range!(@spec $kx, self.$ax, res);
                let $ay = impl_range!(@spec $ky, self.$ay, res);
                let $az = impl_range!(@spec $kz, self.$az, res);
                (0..$a2.1).flat_map(move |i2| {
                    let $a2 = $a2.0 + res * i2 as f32;
                    (0..$a1.1).flat_map(move |i1| {
                        let $a1 = $a1.0 + res * i1 as f32;
                        (0..$a0.1).map(move |i0| {
                            let $a0 = $a0.0 + res * i0 as f32;
                            ($ax, $ay, $az)
                        })
                    })
                })
            }

            fn aabb(&self) -> Aabb {
                Aabb {
                    min: Vector3::new(
                        impl_range!(@min $kx, self.$ax),
                        impl_range!(@min $ky, self.$ay),
                        impl_range!(@min $kz, self.$az),
                    )
                    .into(),
                    max: Vector3::new(
                        impl_range!(@max $kx, self.$ax),
                        impl_range!(@max $ky, self.$ay),
                        impl_range!(@max $kz, self.$az),
                    )
                    .into(),
                }
            }
        }
    };
}

impl_range!(RangeX { x: range, y: scalar, z: scalar }, order: [x, y, z]);
impl_range!(RangeY { x: scalar, y: range, z: scalar }, order: [y, x, z]);
impl_range!(RangeZ { x: scalar, y: scalar, z: range }, order: [z, x, y]);
impl_range!(RangeXY { x: range, y: range, z: scalar }, order: [x, y, z]);
impl_range!(RangeXZ { x: range, y: scalar, z: range }, order: [x, z, y]);
impl_range!(RangeYX { x: range, y: range, z: scalar }, order: [y, x, z]);
impl_range!(RangeYZ { x: scalar, y: range, z: range }, order: [y, z, x]);
impl_range!(RangeZX { x: range, y: scalar, z: range }, order: [z, x, y]);
impl_range!(RangeZY { x: scalar, y: range, z: range }, order: [z, y, x]);
impl_range!(RangeXYZ { x: range, y: range, z: range }, order: [x, y, z]);
impl_range!(RangeXZY { x: range, y: range, z: range }, order: [x, z, y]);
impl_range!(RangeYXZ { x: range, y: range, z: range }, order: [y, x, z]);
impl_range!(RangeYZX { x: range, y: range, z: range }, order: [y, z, x]);
impl_range!(RangeZXY { x: range, y: range, z: range }, order: [z, x, y]);
impl_range!(RangeZYX { x: range, y: range, z: range }, order: [z, y, x]);
