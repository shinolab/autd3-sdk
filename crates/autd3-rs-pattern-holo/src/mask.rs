use autd3_rs_core::geometry::Geometry;
pub(crate) use autd3_rs_core::geometry::TransducerMask;

use crate::error::HoloError;

pub(crate) fn validate_dst_len(dst: usize, geometry: &Geometry) -> Result<(), HoloError> {
    if dst != geometry.num_devices() {
        return Err(HoloError::DstDeviceCountMismatch {
            got: dst,
            expected: geometry.num_devices(),
        });
    }
    Ok(())
}
