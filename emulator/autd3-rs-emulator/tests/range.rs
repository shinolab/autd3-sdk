use autd3_rs::geometry::Point3;

use autd3_rs_emulator::{Range, RangeX, RangeXYZ};

#[test]
fn range_x_points_and_aabb() {
    let r = RangeX {
        x: 0.0..=3.0,
        y: 1.0,
        z: 2.0,
        resolution: 1.0,
    };
    let pts: Vec<_> = r.points().collect();
    assert_eq!(
        pts,
        vec![(0., 1., 2.), (1., 1., 2.), (2., 1., 2.), (3., 1., 2.)]
    );
    assert_eq!(r.aabb().min, Point3::new(0., 1., 2.));
    assert_eq!(r.aabb().max, Point3::new(3., 1., 2.));
}

#[test]
fn range_xyz_iterates_x_inner_z_outer() {
    let r = RangeXYZ {
        x: 0.0..=1.0,
        y: 0.0..=1.0,
        z: 0.0..=0.0,
        resolution: 1.0,
    };
    let pts: Vec<_> = r.points().collect();
    assert_eq!(
        pts,
        vec![(0., 0., 0.), (1., 0., 0.), (0., 1., 0.), (1., 1., 0.),]
    );
    assert_eq!(r.aabb().min, Point3::new(0., 0., 0.));
    assert_eq!(r.aabb().max, Point3::new(1., 1., 0.));
}
