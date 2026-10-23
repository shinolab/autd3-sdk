use autd3_rs_core::geometry::{Geometry, Point3};

#[inline]
pub(crate) fn assert_one_slot_per_device<T>(geometry: &Geometry, dst: &[Vec<T>]) {
    assert_eq!(
        dst.len(),
        geometry.num_devices(),
        "dst must have one slot per device"
    );
}

#[inline]
pub(crate) fn fill_each_transducer<T>(
    geometry: &Geometry,
    dst: &mut [Vec<T>],
    f: impl Fn(Point3<f32>) -> T,
) {
    assert_one_slot_per_device(geometry, dst);
    for (slot, device) in dst.iter_mut().zip(geometry.iter()) {
        for (value, &position) in slot.iter_mut().zip(device.positions()) {
            *value = f(position);
        }
    }
}
