use autd3_rs::geometry::{Autd3, Geometry, Point3, UnitQuaternion};

fn main() {
    // ANCHOR: api
    Geometry::new(vec![Autd3::new(
        Point3::origin(),
        UnitQuaternion::identity(),
    )]);
    // ANCHOR_END: api

    let geometry = Geometry::new(vec![Autd3::new(
        Point3::origin(),
        UnitQuaternion::identity(),
    )]);

    // ANCHOR: access
    let num_devices = geometry.num_devices();
    let total_transducers = geometry.num_transducers();
    let array_center = geometry.center();

    for device in &geometry {
        let _ = device;
    }

    let first = &geometry[0];
    // ANCHOR_END: access

    let device = &geometry[0];
    // ANCHOR: device
    let idx = device.idx();
    let num_transducers = device.num_transducers();
    let center = device.center();
    let rotation = device.rotation();
    let x = device.x_direction();
    let y = device.y_direction();
    let axial = device.axial_direction();
    let positions = device.positions();
    let directions = device.directions();
    let pos0 = device.position(0);
    let dir0 = device.direction(0);
    // ANCHOR_END: device

    let _ = (num_devices, total_transducers, array_center, first);
    let _ = (
        idx,
        num_transducers,
        center,
        rotation,
        x,
        y,
        axial,
        positions,
        directions,
        pos0,
        dir0,
    );
}
