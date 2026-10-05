use autd3_rs_core::geometry::{Device, Geometry, Point3};

#[inline]
pub(crate) fn assert_one_slot_per_device<T>(geometry: &Geometry, dst: &[Vec<T>]) {
    assert_eq!(
        dst.len(),
        geometry.num_devices(),
        "dst must have one slot per device"
    );
}

#[inline]
pub(crate) fn for_each_device<T>(
    geometry: &Geometry,
    dst: &mut [Vec<T>],
    mut f: impl FnMut(&Device, &mut [T]),
) {
    assert_one_slot_per_device(geometry, dst);
    for (slot, device) in dst.iter_mut().zip(geometry.iter()) {
        f(device, slot);
    }
}

#[inline]
pub(crate) fn fill_from_positions<T>(device: &Device, dst: &mut [T], f: impl Fn(Point3<f32>) -> T) {
    for (slot, &position) in dst.iter_mut().zip(device.positions()) {
        *slot = f(position);
    }
}
