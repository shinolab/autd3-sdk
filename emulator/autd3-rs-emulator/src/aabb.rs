use autd3_rs_core::geometry::{Point3, Vector3};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: Point3<f32>,
    pub max: Point3<f32>,
}

impl Aabb {
    #[must_use]
    pub(crate) fn empty() -> Self {
        Self {
            min: Point3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY),
            max: Point3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY),
        }
    }

    #[must_use]
    pub(crate) fn grow(self, other: Point3<f32>) -> Aabb {
        Aabb {
            min: self.min.inf(&other),
            max: self.max.sup(&other),
        }
    }

    #[must_use]
    pub(crate) fn from_points(points: impl IntoIterator<Item = Point3<f32>>) -> Self {
        points.into_iter().fold(Self::empty(), Self::grow)
    }
}

fn corners(aabb: &Aabb) -> Vec<Point3<f32>> {
    [aabb.min.x, aabb.max.x]
        .into_iter()
        .flat_map(move |x| {
            [aabb.min.y, aabb.max.y].into_iter().flat_map(move |y| {
                [aabb.min.z, aabb.max.z]
                    .into_iter()
                    .map(move |z| Point3::new(x, y, z))
            })
        })
        .collect()
}

pub(crate) fn aabb_max_dist(a: &Aabb, b: &Aabb) -> f32 {
    let corners_a = corners(a);
    let corners_b = corners(b);
    corners_a
        .into_iter()
        .flat_map(|a| corners_b.iter().map(move |&b| (a, b)))
        .map(|(a, b)| (a - b).norm())
        .fold(f32::NEG_INFINITY, f32::max)
}

pub(crate) fn aabb_min_dist(a: &Aabb, b: &Aabb) -> f32 {
    let min = Vector3::from_iterator(a.min.iter().zip(b.min.iter()).map(|(a, b)| a.max(*b)));
    let max = Vector3::from_iterator(a.max.iter().zip(b.max.iter()).map(|(a, b)| a.min(*b)));
    min.iter()
        .zip(max.iter())
        .filter(|(min, max)| min > max)
        .map(|(min, max)| (min - max).powi(2))
        .sum::<f32>()
        .sqrt()
}
