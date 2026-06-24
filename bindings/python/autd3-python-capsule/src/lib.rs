use std::ffi::{CStr, c_void};
use std::ptr::NonNull;

use autd3_rs_core::Geometry;
use autd3_rs_core::params::NUM_TRANSDUCERS;
use autd3_rs_core::value::Emission;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyCapsule, PyCapsuleMethods};

pub const GEOMETRY_CAPSULE_NAME: &CStr = c"autd3.geometry.v1";
pub const PATTERN_CAPSULE_NAME: &CStr = c"autd3.pattern.v1";
pub const MODULATION_CAPSULE_NAME: &CStr = c"autd3.modulation.v1";

pub type DevicePattern = [Emission; NUM_TRANSDUCERS];

pub fn to_pyerr<E: core::fmt::Display>(py: Python<'_>, e: E) -> PyErr {
    let msg = e.to_string();
    match py
        .import("autd3_core")
        .and_then(|m| m.getattr("Autd3Error"))
        .and_then(|c| c.call1((msg.clone(),)))
    {
        Ok(inst) => PyErr::from_value(inst),
        Err(_) => PyValueError::new_err(msg),
    }
}

pub fn to_pyerr_gil<E: core::fmt::Display>(e: E) -> PyErr {
    Python::attach(|py| to_pyerr(py, e))
}

pub fn capsule_of<'py>(obj: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyCapsule>> {
    if let Ok(capsule) = obj.cast::<PyCapsule>() {
        return Ok(capsule.clone());
    }
    let capsule = obj.call_method0("_capsule")?;
    Ok(capsule.cast_into::<PyCapsule>()?)
}

pub fn geometry_into_capsule(py: Python<'_>, geometry: Geometry) -> PyResult<Bound<'_, PyCapsule>> {
    PyCapsule::new_with_value(py, geometry, GEOMETRY_CAPSULE_NAME)
}

pub fn geometry_from_capsule<'a>(capsule: &'a Bound<'_, PyCapsule>) -> PyResult<&'a Geometry> {
    let ptr: NonNull<c_void> = capsule.pointer_checked(Some(GEOMETRY_CAPSULE_NAME))?;
    Ok(unsafe { ptr.cast::<Geometry>().as_ref() })
}

pub fn pattern_into_capsule(
    py: Python<'_>,
    emissions: Vec<DevicePattern>,
) -> PyResult<Bound<'_, PyCapsule>> {
    PyCapsule::new_with_value(py, emissions, PATTERN_CAPSULE_NAME)
}

pub fn pattern_from_capsule<'a>(
    capsule: &'a Bound<'_, PyCapsule>,
) -> PyResult<&'a [DevicePattern]> {
    let ptr: NonNull<c_void> = capsule.pointer_checked(Some(PATTERN_CAPSULE_NAME))?;
    Ok(unsafe { ptr.cast::<Vec<DevicePattern>>().as_ref() })
}

/// # Safety
// `ptr` must point to a `Vec<DevicePattern>` that stays alive and uniquely borrowed
/// for as long as the returned capsule (and any reference derived from it) is used.
pub unsafe fn pattern_capsule_mut(
    py: Python<'_>,
    ptr: NonNull<Vec<DevicePattern>>,
) -> PyResult<Bound<'_, PyCapsule>> {
    // SAFETY: caller upholds the pointer-validity contract above; no destructor is
    // attached, so the capsule never frees the borrowed `Vec`.
    unsafe { PyCapsule::new_with_pointer(py, ptr.cast::<c_void>(), PATTERN_CAPSULE_NAME) }
}

#[allow(clippy::mut_from_ref)]
pub fn pattern_from_capsule_mut<'a>(
    capsule: &'a Bound<'_, PyCapsule>,
) -> PyResult<&'a mut Vec<DevicePattern>> {
    let ptr: NonNull<c_void> = capsule.pointer_checked(Some(PATTERN_CAPSULE_NAME))?;
    // SAFETY: name-checked above; produced by `pattern_capsule_mut` pointing at a live,
    // uniquely-borrowed `Vec<DevicePattern>` whose owner outlives the returned borrow.
    Ok(unsafe { ptr.cast::<Vec<DevicePattern>>().as_mut() })
}

pub fn modulation_into_capsule(py: Python<'_>, data: Vec<u8>) -> PyResult<Bound<'_, PyCapsule>> {
    PyCapsule::new_with_value(py, data, MODULATION_CAPSULE_NAME)
}

pub fn modulation_from_capsule<'a>(capsule: &'a Bound<'_, PyCapsule>) -> PyResult<&'a [u8]> {
    let ptr: NonNull<c_void> = capsule.pointer_checked(Some(MODULATION_CAPSULE_NAME))?;
    Ok(unsafe { ptr.cast::<Vec<u8>>().as_ref() })
}

#[cfg(feature = "client")]
mod link {
    use std::cell::RefCell;
    use std::ffi::{CStr, c_void};
    use std::future::Future;
    use std::pin::Pin;
    use std::ptr::NonNull;
    use std::sync::Arc;

    use autd3_rs::{ClientConfig, Datagrams};
    use autd3_rs_core::{Error, Geometry};
    use pyo3::exceptions::PyValueError;
    use pyo3::prelude::*;
    use pyo3::types::{PyCapsule, PyCapsuleMethods};

    pub const LINK_CAPSULE_NAME: &CStr = c"autd3.link.v1";

    pub type BoxFuture<T> = Pin<Box<dyn Future<Output = Result<T, Error>> + Send>>;

    pub struct LinkStatusData {
        pub device_states: Vec<String>,
        pub all_op: bool,
        pub any_lost: bool,
        pub recoveries: u64,
    }

    pub trait ClientBackend: Send + Sync {
        fn num_devices(&self) -> usize;
        fn read_firmware_version(&self) -> BoxFuture<Vec<String>>;
        fn read_fpga_state(&self) -> BoxFuture<Vec<u8>>;
        fn read_error_detail(&self) -> BoxFuture<Vec<u8>>;
        fn send_checked(&self, datagrams: Arc<Datagrams>, frame: Option<usize>) -> BoxFuture<()>;
        fn check_status(&self) -> BoxFuture<LinkStatusData>;
        fn stop(&self) -> BoxFuture<()>;
        fn close(&self) -> BoxFuture<()>;
    }

    pub type ClientOpener =
        Box<dyn FnOnce(Geometry, ClientConfig) -> BoxFuture<Box<dyn ClientBackend>> + Send>;

    pub fn client_opener<F, Fut>(f: F) -> ClientOpener
    where
        F: FnOnce(Geometry, ClientConfig) -> Fut + Send + 'static,
        Fut: Future<Output = Result<Box<dyn ClientBackend>, Error>> + Send + 'static,
    {
        Box::new(move |geo, cfg| Box::pin(f(geo, cfg)))
    }

    pub fn link_into_capsule(
        py: Python<'_>,
        opener: ClientOpener,
    ) -> PyResult<Bound<'_, PyCapsule>> {
        PyCapsule::new_with_value(py, RefCell::new(Some(opener)), LINK_CAPSULE_NAME)
    }

    pub fn take_client_opener(capsule: &Bound<'_, PyCapsule>) -> PyResult<ClientOpener> {
        let ptr: NonNull<c_void> = capsule.pointer_checked(Some(LINK_CAPSULE_NAME))?;
        // SAFETY: name-checked above; produced by `link_into_capsule` storing a
        // `RefCell<Option<ClientOpener>>`. Same autd3-rs version across wheels.
        let cell = unsafe { ptr.cast::<RefCell<Option<ClientOpener>>>().as_ref() };
        cell.borrow_mut()
            .take()
            .ok_or_else(|| PyValueError::new_err("link has already been consumed by open()"))
    }
}

#[cfg(feature = "client")]
pub use link::{
    BoxFuture, ClientBackend, ClientOpener, LINK_CAPSULE_NAME, LinkStatusData, client_opener,
    link_into_capsule, take_client_opener,
};
