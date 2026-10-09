use autd3_python_capsule::error_code;
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;

create_exception!(autd3_core, Autd3Error, PyException);

pub(crate) fn to_pyerr<E: core::fmt::Display>(e: E) -> pyo3::PyErr {
    Autd3Error::new_err(e.to_string())
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let ty = m.py().get_type::<Autd3Error>();
    ty.setattr("code", error_code::GENERIC)?;
    ty.setattr("GENERIC", error_code::GENERIC)?;
    ty.setattr("TIMEOUT", error_code::TIMEOUT)?;
    ty.setattr("DEVICE", error_code::DEVICE)?;
    ty.setattr("NETWORK", error_code::NETWORK)?;
    ty.setattr("INVALID_ARGUMENT", error_code::INVALID_ARGUMENT)?;
    ty.setattr("UNSUPPORTED_FIRMWARE", error_code::UNSUPPORTED_FIRMWARE)?;
    m.add("Autd3Error", ty)
}
