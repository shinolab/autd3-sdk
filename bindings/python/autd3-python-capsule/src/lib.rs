pub mod extract;
pub mod numpy;

pub use autd3_python_cell as cell;

use std::ffi::{CStr, c_void};
use std::ptr::NonNull;

use autd3_rs_core::value::{Intensity, Phase, SamplingConfig};
use autd3_rs_core::{Device, Geometry};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyCapsule, PyCapsuleMethods};

use crate::cell::{BufferCell, ExclusiveGuard, SharedGuard};

const GEOMETRY_CAPSULE_NAME: &CStr = c"autd3.geometry.v1";
const DEVICE_CAPSULE_NAME: &CStr = c"autd3.device.v1";
const PHASE_CAPSULE_NAME: &CStr = c"autd3.phase.v3";
const INTENSITY_CAPSULE_NAME: &CStr = c"autd3.intensity.v3";
const MODULATION_CAPSULE_NAME: &CStr = c"autd3.modulation.v1";
const SAMPLING_CONFIG_CAPSULE_NAME: &CStr = c"autd3.sampling_config.v1";

pub mod error_code {
    pub const GENERIC: i32 = -1;
    pub const TIMEOUT: i32 = -2;
    pub const DEVICE: i32 = -3;
    pub const NETWORK: i32 = -4;
    pub const INVALID_ARGUMENT: i32 = -5;
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

pub fn invalid_argument<E: core::fmt::Display>(py: Python<'_>, e: E) -> PyErr {
    to_pyerr_with_code(py, error_code::INVALID_ARGUMENT, e.to_string())
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

#[repr(C)]
struct BorrowedBuffer {
    addr: usize,
    _owner: Py<PyAny>,
}

unsafe fn buffer_capsule<'py, T>(
    py: Python<'py>,
    cell: NonNull<BufferCell<T>>,
    owner: Py<PyAny>,
    name: &'static CStr,
) -> PyResult<Bound<'py, PyCapsule>> {
    PyCapsule::new_with_value(
        py,
        BorrowedBuffer {
            addr: cell.as_ptr() as usize,
            _owner: owner,
        },
        name,
    )
}

fn buffer_cell<'a, T>(
    capsule: &'a Bound<'_, PyCapsule>,
    name: &'static CStr,
) -> PyResult<&'a BufferCell<T>> {
    let ptr: NonNull<c_void> = capsule.pointer_checked(Some(name))?;
    let addr = unsafe { ptr.cast::<BorrowedBuffer>().as_ref() }.addr;
    Ok(unsafe { &*(addr as *const BufferCell<T>) })
}

pub fn phase_buffer_addr(capsule: &Bound<'_, PyCapsule>) -> PyResult<usize> {
    Ok(core::ptr::from_ref(buffer_cell::<Phase>(capsule, PHASE_CAPSULE_NAME)?).addr())
}

pub fn intensity_buffer_addr(capsule: &Bound<'_, PyCapsule>) -> PyResult<usize> {
    Ok(core::ptr::from_ref(buffer_cell::<Intensity>(capsule, INTENSITY_CAPSULE_NAME)?).addr())
}

#[allow(clippy::missing_safety_doc)]
pub unsafe fn phase_capsule(
    py: Python<'_>,
    cell: NonNull<BufferCell<Phase>>,
    owner: Py<PyAny>,
) -> PyResult<Bound<'_, PyCapsule>> {
    unsafe { buffer_capsule(py, cell, owner, PHASE_CAPSULE_NAME) }
}

pub fn phases_from_capsule<'a>(
    capsule: &'a Bound<'_, PyCapsule>,
) -> PyResult<SharedGuard<'a, Phase>> {
    Ok(buffer_cell(capsule, PHASE_CAPSULE_NAME)?.try_read()?)
}

pub fn phases_from_capsule_mut<'a>(
    capsule: &'a Bound<'_, PyCapsule>,
) -> PyResult<ExclusiveGuard<'a, Phase>> {
    Ok(buffer_cell(capsule, PHASE_CAPSULE_NAME)?.try_write()?)
}

#[allow(clippy::missing_safety_doc)]
pub unsafe fn intensity_capsule(
    py: Python<'_>,
    cell: NonNull<BufferCell<Intensity>>,
    owner: Py<PyAny>,
) -> PyResult<Bound<'_, PyCapsule>> {
    unsafe { buffer_capsule(py, cell, owner, INTENSITY_CAPSULE_NAME) }
}

pub fn intensities_from_capsule<'a>(
    capsule: &'a Bound<'_, PyCapsule>,
) -> PyResult<SharedGuard<'a, Intensity>> {
    Ok(buffer_cell(capsule, INTENSITY_CAPSULE_NAME)?.try_read()?)
}

pub fn intensities_from_capsule_mut<'a>(
    capsule: &'a Bound<'_, PyCapsule>,
) -> PyResult<ExclusiveGuard<'a, Intensity>> {
    Ok(buffer_cell(capsule, INTENSITY_CAPSULE_NAME)?.try_write()?)
}

pub fn sampling_config_into_capsule(
    py: Python<'_>,
    config: SamplingConfig,
) -> PyResult<Bound<'_, PyCapsule>> {
    PyCapsule::new_with_value(py, config, SAMPLING_CONFIG_CAPSULE_NAME)
}

pub fn sampling_config_from_capsule(capsule: &Bound<'_, PyCapsule>) -> PyResult<SamplingConfig> {
    let ptr: NonNull<c_void> = capsule.pointer_checked(Some(SAMPLING_CONFIG_CAPSULE_NAME))?;
    Ok(*unsafe { ptr.cast::<SamplingConfig>().as_ref() })
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
            Error::Timeout { .. } => error_code::TIMEOUT,
            Error::DeviceError { .. } | Error::UnexpectedReply { .. } => error_code::DEVICE,
            Error::Network(_) | Error::DeviceLost { .. } => error_code::NETWORK,
            Error::InvalidPayload(_) => error_code::INVALID_ARGUMENT,
            Error::DeviceTimeUnknown | Error::Closed => error_code::GENERIC,
            _ => unreachable!(),
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
