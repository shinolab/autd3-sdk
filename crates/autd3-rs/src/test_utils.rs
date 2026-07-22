use std::sync::Arc;

use crate::geometry::{Autd3, Device, Geometry};

pub(crate) fn test_geometry(num_devices: usize) -> Geometry {
    Geometry::new((0..num_devices).map(|_| Autd3::default()).collect())
}

pub(crate) fn test_device(idx: usize) -> Device {
    test_geometry(idx + 1)[idx].clone()
}

pub(crate) fn test_geometry_arc(num_devices: usize) -> Arc<Geometry> {
    Arc::new(test_geometry(num_devices))
}
