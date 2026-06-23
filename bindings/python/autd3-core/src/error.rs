use pyo3::create_exception;
use pyo3::exceptions::PyException;

create_exception!(autd3_core, Autd3Error, PyException);

pub(crate) fn to_pyerr<E: core::fmt::Display>(e: E) -> pyo3::PyErr {
    Autd3Error::new_err(e.to_string())
}
