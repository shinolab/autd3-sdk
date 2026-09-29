pub mod numpy;

use std::ffi::{CStr, c_void};
use std::ptr::NonNull;

use autd3_rs_core::Geometry;
use autd3_rs_core::value::{Intensity, Phase};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyCapsule, PyCapsuleMethods};

pub const GEOMETRY_CAPSULE_NAME: &CStr = c"autd3.geometry.v1";
pub const PHASE_CAPSULE_NAME: &CStr = c"autd3.phase.v1";
pub const PHASE_MUT_CAPSULE_NAME: &CStr = c"autd3.phase.mut.v1";
pub const INTENSITY_CAPSULE_NAME: &CStr = c"autd3.intensity.v1";
pub const INTENSITY_MUT_CAPSULE_NAME: &CStr = c"autd3.intensity.mut.v1";
pub const MODULATION_CAPSULE_NAME: &CStr = c"autd3.modulation.v1";

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

fn buffer_into_capsule<'py, T: Send + 'static>(
    py: Python<'py>,
    data: Vec<Vec<T>>,
    name: &'static CStr,
) -> PyResult<Bound<'py, PyCapsule>> {
    PyCapsule::new_with_value(py, data, name)
}

fn buffer_from_capsule<'a, T>(
    capsule: &'a Bound<'_, PyCapsule>,
    name: &'static CStr,
) -> PyResult<&'a [Vec<T>]> {
    let ptr: NonNull<c_void> = capsule.pointer_checked(Some(name))?;
    Ok(unsafe { ptr.cast::<Vec<Vec<T>>>().as_ref() })
}

pub struct BufferMut {
    addr: usize,
    _owner: Py<PyAny>,
}

unsafe fn buffer_capsule_mut<'py, T>(
    py: Python<'py>,
    ptr: NonNull<Vec<Vec<T>>>,
    owner: Py<PyAny>,
    name: &'static CStr,
) -> PyResult<Bound<'py, PyCapsule>> {
    PyCapsule::new_with_value(
        py,
        BufferMut {
            addr: ptr.as_ptr() as usize,
            _owner: owner,
        },
        name,
    )
}

#[allow(clippy::mut_from_ref)]
fn buffer_from_capsule_mut<'a, T>(
    capsule: &'a Bound<'_, PyCapsule>,
    name: &'static CStr,
) -> PyResult<&'a mut Vec<Vec<T>>> {
    let ptr: NonNull<c_void> = capsule.pointer_checked(Some(name))?;
    let addr = unsafe { ptr.cast::<BufferMut>().as_ref() }.addr;
    Ok(unsafe { &mut *(addr as *mut Vec<Vec<T>>) })
}

pub fn phases_into_capsule(
    py: Python<'_>,
    data: Vec<Vec<Phase>>,
) -> PyResult<Bound<'_, PyCapsule>> {
    buffer_into_capsule(py, data, PHASE_CAPSULE_NAME)
}

pub fn phases_from_capsule<'a>(capsule: &'a Bound<'_, PyCapsule>) -> PyResult<&'a [Vec<Phase>]> {
    buffer_from_capsule(capsule, PHASE_CAPSULE_NAME)
}

#[allow(clippy::missing_safety_doc)]
pub unsafe fn phase_capsule_mut(
    py: Python<'_>,
    ptr: NonNull<Vec<Vec<Phase>>>,
    owner: Py<PyAny>,
) -> PyResult<Bound<'_, PyCapsule>> {
    unsafe { buffer_capsule_mut(py, ptr, owner, PHASE_MUT_CAPSULE_NAME) }
}

#[allow(clippy::mut_from_ref)]
pub fn phases_from_capsule_mut<'a>(
    capsule: &'a Bound<'_, PyCapsule>,
) -> PyResult<&'a mut Vec<Vec<Phase>>> {
    buffer_from_capsule_mut(capsule, PHASE_MUT_CAPSULE_NAME)
}

pub fn intensities_into_capsule(
    py: Python<'_>,
    data: Vec<Vec<Intensity>>,
) -> PyResult<Bound<'_, PyCapsule>> {
    buffer_into_capsule(py, data, INTENSITY_CAPSULE_NAME)
}

pub fn intensities_from_capsule<'a>(
    capsule: &'a Bound<'_, PyCapsule>,
) -> PyResult<&'a [Vec<Intensity>]> {
    buffer_from_capsule(capsule, INTENSITY_CAPSULE_NAME)
}

#[allow(clippy::missing_safety_doc)]
pub unsafe fn intensity_capsule_mut(
    py: Python<'_>,
    ptr: NonNull<Vec<Vec<Intensity>>>,
    owner: Py<PyAny>,
) -> PyResult<Bound<'_, PyCapsule>> {
    unsafe { buffer_capsule_mut(py, ptr, owner, INTENSITY_MUT_CAPSULE_NAME) }
}

#[allow(clippy::mut_from_ref)]
pub fn intensities_from_capsule_mut<'a>(
    capsule: &'a Bound<'_, PyCapsule>,
) -> PyResult<&'a mut Vec<Vec<Intensity>>> {
    buffer_from_capsule_mut(capsule, INTENSITY_MUT_CAPSULE_NAME)
}

pub fn modulation_into_capsule(py: Python<'_>, data: Vec<u8>) -> PyResult<Bound<'_, PyCapsule>> {
    PyCapsule::new_with_value(py, data, MODULATION_CAPSULE_NAME)
}

pub fn modulation_from_capsule<'a>(capsule: &'a Bound<'_, PyCapsule>) -> PyResult<&'a [u8]> {
    let ptr: NonNull<c_void> = capsule.pointer_checked(Some(MODULATION_CAPSULE_NAME))?;
    Ok(unsafe { ptr.cast::<Vec<u8>>().as_ref() })
}

#[cfg(feature = "client")]
mod client {
    use std::ffi::{CStr, c_void};
    use std::future::Future;
    use std::pin::Pin;
    use std::ptr::NonNull;
    use std::sync::Arc;

    use autd3_rs::Error;
    use autd3_rs::{Frames, Response, ResponseFuture};
    use pyo3::prelude::*;
    use pyo3::types::{PyCapsule, PyCapsuleMethods};

    pub const FRAME_CAPSULE_NAME: &CStr = c"autd3.frame.v1";

    #[must_use]
    pub fn network_err(message: impl Into<String>) -> Error {
        Error::Network(autd3_rs::NetworkCause::new(std::io::Error::other(
            message.into(),
        )))
    }

    pub fn frame_into_capsule(
        py: Python<'_>,
        frames: Arc<Frames>,
        index: usize,
    ) -> PyResult<Bound<'_, PyCapsule>> {
        PyCapsule::new_with_value(py, (frames, index), FRAME_CAPSULE_NAME)
    }

    pub fn frame_from_capsule(capsule: &Bound<'_, PyCapsule>) -> PyResult<(Arc<Frames>, usize)> {
        let ptr: NonNull<c_void> = capsule.pointer_checked(Some(FRAME_CAPSULE_NAME))?;
        let (frames, index) = unsafe { ptr.cast::<(Arc<Frames>, usize)>().as_ref() };
        Ok((Arc::clone(frames), *index))
    }

    pub type BoxFuture<T> = Pin<Box<dyn Future<Output = Result<T, Error>> + Send>>;

    pub struct DeviceStatusData {
        pub device_states: Vec<String>,
        pub all_op: bool,
        pub any_lost: bool,
        pub recoveries: u64,
    }

    pub struct ResponseToken {
        fut: ResponseFuture,
    }

    impl ResponseToken {
        #[must_use]
        pub fn new(fut: ResponseFuture) -> Self {
            Self { fut }
        }

        #[must_use]
        pub fn wait(self) -> BoxFuture<Response> {
            Box::pin(self.fut)
        }
    }

    pub trait ClientBackend: Send + Sync {
        fn num_devices(&self) -> usize;
        fn dc_offset_ns(&self) -> i64;
        fn read_firmware_version(&self) -> BoxFuture<Vec<String>>;
        fn read_fpga_state(&self) -> BoxFuture<Vec<u8>>;
        fn read_error_detail(&self) -> BoxFuture<Vec<u8>>;
        fn read_telemetry(&self, counter: autd3_rs::Telemetry) -> BoxFuture<Vec<u8>>;
        fn send(&self, datagrams: Arc<Frames>, index: usize) -> BoxFuture<ResponseToken>;
        fn send_checked(&self, datagrams: Arc<Frames>, frame: Option<usize>) -> BoxFuture<()>;
        fn check_status(&self) -> Result<DeviceStatusData, Error>;
        fn stop(&self) -> BoxFuture<()>;
        fn close(&self) -> BoxFuture<()>;
    }
}

#[cfg(feature = "client")]
pub use client::{
    BoxFuture, ClientBackend, DeviceStatusData, FRAME_CAPSULE_NAME, ResponseToken,
    frame_from_capsule, frame_into_capsule, network_err,
};
