use std::ops::RangeInclusive;

use autd3_rs::geometry::Point3;

use autd3_rs_emulator::{AxisOrder, Grid, Range};

const ORDERS: [(AxisOrder, [usize; 3]); 6] = [
    (AxisOrder::XYZ, [0, 1, 2]),
    (AxisOrder::XZY, [0, 2, 1]),
    (AxisOrder::YXZ, [1, 0, 2]),
    (AxisOrder::YZX, [1, 2, 0]),
    (AxisOrder::ZXY, [2, 0, 1]),
    (AxisOrder::ZYX, [2, 1, 0]),
];

fn grid(axes: [RangeInclusive<f32>; 3], order: AxisOrder) -> Grid {
    let [x, y, z] = axes;
    Grid {
        x,
        y,
        z,
        resolution: 1.0,
        order,
    }
}

fn nested_loops(values: [&[f32]; 3], [inner, middle, outer]: [usize; 3]) -> Vec<(f32, f32, f32)> {
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
        let g = grid(axes.clone(), order);
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
    let xy = grid([0.0..=1.0, 0.0..=1.0, 0.0..=0.0], AxisOrder::XYZ);
    assert_eq!(
        xy.points().collect::<Vec<_>>(),
        vec![(0., 0., 0.), (1., 0., 0.), (0., 1., 0.), (1., 1., 0.)]
    );
    let yx = grid([0.0..=1.0, 0.0..=1.0, 0.0..=0.0], AxisOrder::YXZ);
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
