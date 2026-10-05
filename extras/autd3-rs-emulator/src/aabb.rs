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

pub(crate) fn aabb_max_dist(a: &Aabb, b: &Aabb) -> f32 {
    (a.max - b.min).sup(&(b.max - a.min)).norm()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn corners(aabb: &Aabb) -> Vec<Point3<f32>> {
        let mut out = Vec::new();
        for x in [aabb.min.x, aabb.max.x] {
            for y in [aabb.min.y, aabb.max.y] {
                for z in [aabb.min.z, aabb.max.z] {
                    out.push(Point3::new(x, y, z));
                }
            }
        }
        out
    }

    fn brute_force_max_dist(a: &Aabb, b: &Aabb) -> f32 {
        corners(a)
            .into_iter()
            .flat_map(|p| corners(b).into_iter().map(move |q| (p - q).norm()))
            .fold(f32::NEG_INFINITY, f32::max)
    }

    fn aabb(min: [f32; 3], max: [f32; 3]) -> Aabb {
        Aabb {
            min: Point3::from(min),
            max: Point3::from(max),
        }
    }

    #[test]
    fn max_dist_of_separated_boxes_matches_the_corner_search() {
        let a = aabb([0.0, 0.0, 0.0], [10.0, 20.0, 1.0]);
        let b = aabb([-30.0, 50.0, 150.0], [-5.0, 70.0, 180.0]);
        assert_eq!(
            aabb_max_dist(&a, &b).to_bits(),
            brute_force_max_dist(&a, &b).to_bits()
        );
        assert_eq!(
            aabb_max_dist(&b, &a).to_bits(),
            brute_force_max_dist(&a, &b).to_bits()
        );
    }

    #[test]
    fn max_dist_of_overlapping_boxes_matches_the_corner_search() {
        let a = aabb([0.0, 0.0, 0.0], [10.0, 10.0, 10.0]);
        let b = aabb([5.0, -5.0, 2.0], [7.0, 30.0, 4.0]);
        assert_eq!(
            aabb_max_dist(&a, &b).to_bits(),
            brute_force_max_dist(&a, &b).to_bits()
        );
        assert_eq!(
            aabb_max_dist(&a, &a).to_bits(),
            brute_force_max_dist(&a, &a).to_bits()
        );
    }

    #[test]
    fn max_dist_of_points_is_their_distance() {
        let a = aabb([1.0, 2.0, 3.0], [1.0, 2.0, 3.0]);
        let b = aabb([4.0, 6.0, 3.0], [4.0, 6.0, 3.0]);
        assert_eq!(aabb_max_dist(&a, &b), 5.0);
        assert_eq!(aabb_max_dist(&a, &a), 0.0);
    }
}
