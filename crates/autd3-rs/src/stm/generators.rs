use core::f32::consts::PI;

use super::{ControlPoint, ControlPoints};
use crate::Length;
use crate::geometry::{Point3, UnitVector3, Vector3};
use crate::value::Intensity;

#[must_use]
pub fn circle(
    center: Point3<f32>,
    radius: Length,
    num_points: usize,
    n: UnitVector3<f32>,
    intensity: Intensity,
) -> Vec<ControlPoints<1>> {
    let z = Vector3::z();
    let v0 = if n.dot(&z).abs() < 0.9 {
        z
    } else {
        Vector3::y()
    };
    let u = n.cross(&v0).normalize();
    let v = n.cross(&u).normalize();
    let radius = radius.mm();
    (0..num_points)
        .map(|i| {
            let theta = 2.0 * PI * i as f32 / num_points as f32;
            let point = center + (u * theta.cos() + v * theta.sin()) * radius;
            ControlPoints::new([ControlPoint::from(point)], intensity)
        })
        .collect()
}

#[must_use]
pub fn line(
    start: Point3<f32>,
    end: Point3<f32>,
    num_points: usize,
    intensity: Intensity,
) -> Vec<ControlPoints<1>> {
    let dir = end - start;
    let denom = (num_points.max(2) - 1) as f32;
    (0..num_points)
        .map(|i| {
            let point = start + dir * (i as f32 / denom);
            ControlPoints::new([ControlPoint::from(point)], intensity)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::mm;

    fn approx(a: Point3<f32>, b: Point3<f32>) {
        assert!((a - b).norm() < 1e-3, "{a:?} != {b:?}");
    }

    #[test]
    fn circle_in_xy_plane_walks_around_center() {
        let pts = circle(
            Point3::origin(),
            30.0 * mm,
            4,
            Vector3::z_axis(),
            Intensity::MAX,
        );
        assert_eq!(pts.len(), 4);
        approx(pts[0].points[0].point, Point3::new(-30.0, 0.0, 0.0));
        approx(pts[1].points[0].point, Point3::new(0.0, -30.0, 0.0));
        approx(pts[2].points[0].point, Point3::new(30.0, 0.0, 0.0));
        approx(pts[3].points[0].point, Point3::new(0.0, 30.0, 0.0));
        assert!(pts.iter().all(|p| p.intensity == Intensity::MAX));
    }

    #[test]
    fn line_interpolates_endpoints_inclusive() {
        let pts = line(
            Point3::new(0.0, -15.0, 0.0),
            Point3::new(0.0, 15.0, 0.0),
            3,
            Intensity(0x40),
        );
        assert_eq!(pts.len(), 3);
        approx(pts[0].points[0].point, Point3::new(0.0, -15.0, 0.0));
        approx(pts[1].points[0].point, Point3::new(0.0, 0.0, 0.0));
        approx(pts[2].points[0].point, Point3::new(0.0, 15.0, 0.0));
        assert!(pts.iter().all(|p| p.intensity == Intensity(0x40)));
    }
}
