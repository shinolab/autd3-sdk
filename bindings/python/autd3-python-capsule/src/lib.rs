pub mod extract;
pub mod numpy;

use std::ffi::{CStr, c_void};
use std::ptr::NonNull;

use autd3_rs_core::value::{Intensity, Phase};
use autd3_rs_core::{Device, Geometry};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyCapsule, PyCapsuleMethods};

const GEOMETRY_CAPSULE_NAME: &CStr = c"autd3.geometry.v1";
const DEVICE_CAPSULE_NAME: &CStr = c"autd3.device.v1";
const PHASE_CAPSULE_NAME: &CStr = c"autd3.phase.v2";
const INTENSITY_CAPSULE_NAME: &CStr = c"autd3.intensity.v2";
const MODULATION_CAPSULE_NAME: &CStr = c"autd3.modulation.v1";

pub mod error_code {
    pub const GENERIC: i32 = -1;
    pub const TIMEOUT: i32 = -2;
    pub const DEVICE: i32 = -3;
    pub const NETWORK: i32 = -4;
    pub const INVALID_ARGUMENT: i32 = -5;
    pub const UNSUPPORTED_FIRMWARE: i32 = -6;
}

#[must_use]
pub fn to_pyerr_with_code(py: Python<'_>, code: i32, msg: String) -> PyErr {
    match py
        .import("autd3_core")
        .and_then(|m| m.getattr("Autd3Error"))
        .and_then(|c| c.call1((msg.clone(),)))
        .and_then(|inst| inst.setattr("code", code).map(|()| inst))
    {
        Ok(inst) => PyErr::from_value(inst),
        Err(_) => PyValueError::new_err(msg),
    }
}

pub fn to_pyerr<E: core::fmt::Display>(py: Python<'_>, e: E) -> PyErr {
    to_pyerr_with_code(py, error_code::GENERIC, e.to_string())
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

pub fn device_into_capsule(py: Python<'_>, device: Device) -> PyResult<Bound<'_, PyCapsule>> {
    PyCapsule::new_with_value(py, device, DEVICE_CAPSULE_NAME)
}

pub fn device_from_capsule<'a>(capsule: &'a Bound<'_, PyCapsule>) -> PyResult<&'a Device> {
    let ptr: NonNull<c_void> = capsule.pointer_checked(Some(DEVICE_CAPSULE_NAME))?;
    Ok(unsafe { ptr.cast::<Device>().as_ref() })
}

struct BorrowedBuffer {
    addr: usize,
    _owner: Py<PyAny>,
}

unsafe fn buffer_capsule<'py, T>(
    py: Python<'py>,
    ptr: NonNull<Vec<Vec<T>>>,
    owner: Py<PyAny>,
    name: &'static CStr,
) -> PyResult<Bound<'py, PyCapsule>> {
    PyCapsule::new_with_value(
        py,
        BorrowedBuffer {
            addr: ptr.as_ptr() as usize,
            _owner: owner,
        },
        name,
    )
}

fn buffer_ptr<T>(
    capsule: &Bound<'_, PyCapsule>,
    name: &'static CStr,
) -> PyResult<*mut Vec<Vec<T>>> {
    let ptr: NonNull<c_void> = capsule.pointer_checked(Some(name))?;
    Ok(unsafe { ptr.cast::<BorrowedBuffer>().as_ref() }.addr as *mut Vec<Vec<T>>)
}

pub fn phase_buffer_addr(capsule: &Bound<'_, PyCapsule>) -> PyResult<usize> {
    Ok(buffer_ptr::<Phase>(capsule, PHASE_CAPSULE_NAME)?.addr())
}

pub fn intensity_buffer_addr(capsule: &Bound<'_, PyCapsule>) -> PyResult<usize> {
    Ok(buffer_ptr::<Intensity>(capsule, INTENSITY_CAPSULE_NAME)?.addr())
}

#[allow(clippy::missing_safety_doc)]
pub unsafe fn phase_capsule(
    py: Python<'_>,
    ptr: NonNull<Vec<Vec<Phase>>>,
    owner: Py<PyAny>,
) -> PyResult<Bound<'_, PyCapsule>> {
    unsafe { buffer_capsule(py, ptr, owner, PHASE_CAPSULE_NAME) }
}

pub fn phases_from_capsule<'a>(capsule: &'a Bound<'_, PyCapsule>) -> PyResult<&'a [Vec<Phase>]> {
    Ok(unsafe { &*buffer_ptr(capsule, PHASE_CAPSULE_NAME)? })
}

#[allow(clippy::mut_from_ref)]
pub fn phases_from_capsule_mut<'a>(
    capsule: &'a Bound<'_, PyCapsule>,
) -> PyResult<&'a mut Vec<Vec<Phase>>> {
    Ok(unsafe { &mut *buffer_ptr(capsule, PHASE_CAPSULE_NAME)? })
}

#[allow(clippy::missing_safety_doc)]
pub unsafe fn intensity_capsule(
    py: Python<'_>,
    ptr: NonNull<Vec<Vec<Intensity>>>,
    owner: Py<PyAny>,
) -> PyResult<Bound<'_, PyCapsule>> {
    unsafe { buffer_capsule(py, ptr, owner, INTENSITY_CAPSULE_NAME) }
}

pub fn intensities_from_capsule<'a>(
    capsule: &'a Bound<'_, PyCapsule>,
) -> PyResult<&'a [Vec<Intensity>]> {
    Ok(unsafe { &*buffer_ptr(capsule, INTENSITY_CAPSULE_NAME)? })
}

#[allow(clippy::mut_from_ref)]
pub fn intensities_from_capsule_mut<'a>(
    capsule: &'a Bound<'_, PyCapsule>,
) -> PyResult<&'a mut Vec<Vec<Intensity>>> {
    Ok(unsafe { &mut *buffer_ptr(capsule, INTENSITY_CAPSULE_NAME)? })
}

pub fn modulation_into_capsule(py: Python<'_>, data: Vec<u8>) -> PyResult<Bound<'_, PyCapsule>> {
    PyCapsule::new_with_value(py, data, MODULATION_CAPSULE_NAME)
}

pub fn modulation_from_capsule<'a>(capsule: &'a Bound<'_, PyCapsule>) -> PyResult<&'a [u8]> {
    let ptr: NonNull<c_void> = capsule.pointer_checked(Some(MODULATION_CAPSULE_NAME))?;
    Ok(unsafe { ptr.cast::<Vec<u8>>().as_ref() })
}

#[cfg(feature = "frame")]
mod frame {
    use std::ffi::{CStr, c_void};
    use std::ptr::NonNull;
    use std::sync::Arc;

    use autd3_rs::Frames;

    use pyo3::prelude::*;
    use pyo3::types::{PyCapsule, PyCapsuleMethods};

    const FRAME_CAPSULE_NAME: &CStr = c"autd3.frame.v1";

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
}

#[cfg(feature = "frame")]
pub use frame::{frame_from_capsule, frame_into_capsule};

#[cfg(feature = "frame")]
mod client_error {
    use autd3_rs::Error;
    use pyo3::prelude::*;

    use crate::error_code;

    #[must_use]
    pub fn client_error_code(e: &Error) -> i32 {
        match e {
            Error::Timeout { .. } | Error::SeqMismatch { .. } => error_code::TIMEOUT,
            Error::DeviceError { .. } | Error::UnexpectedReply { .. } => error_code::DEVICE,
            Error::Network(_) | Error::DeviceLost { .. } => error_code::NETWORK,
            Error::UnsupportedFirmware { .. } => error_code::UNSUPPORTED_FIRMWARE,
            Error::InvalidPayload(_) | Error::Encode(_) => error_code::INVALID_ARGUMENT,
            _ => error_code::GENERIC,
        }
    }

    #[must_use]
    pub fn client_pyerr(py: Python<'_>, e: &Error) -> PyErr {
        crate::to_pyerr_with_code(py, client_error_code(e), e.to_string())
    }

    #[allow(clippy::needless_pass_by_value)]
    #[must_use]
    pub fn client_pyerr_gil(e: Error) -> PyErr {
        Python::attach(|py| client_pyerr(py, &e))
    }
}

#[cfg(feature = "frame")]
pub use client_error::{client_error_code, client_pyerr, client_pyerr_gil};
