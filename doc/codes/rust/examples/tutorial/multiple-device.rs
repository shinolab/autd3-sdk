use std::f32::consts::FRAC_PI_2;

use anyhow::Result;

use autd3_rs::geometry::{Autd3, Geometry, Point3, UnitQuaternion, Vector3};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    // ANCHOR: translation
    Geometry::new(vec![
        Autd3::new(Point3::origin(), UnitQuaternion::identity()),
        Autd3::new(
            Point3::new(Autd3::DEVICE_WIDTH, 0.0, 0.0),
            UnitQuaternion::identity(),
        ),
    ]);
    // ANCHOR_END: translation

    // ANCHOR: global
    Geometry::new(vec![
        Autd3::new(
            Point3::new(-Autd3::DEVICE_WIDTH, 0.0, 0.0),
            UnitQuaternion::identity(),
        ),
        Autd3::new(Point3::origin(), UnitQuaternion::identity()),
    ]);
    // ANCHOR_END: global

    // ANCHOR: rotation
    Geometry::new(vec![
        Autd3::new(Point3::origin(), UnitQuaternion::identity()),
        Autd3::new(
            Point3::new(0.0, 0.0, Autd3::DEVICE_WIDTH),
            UnitQuaternion::from_axis_angle(&Vector3::y_axis(), FRAC_PI_2),
        ),
    ]);
    // ANCHOR_END: rotation

    Ok(())
}
