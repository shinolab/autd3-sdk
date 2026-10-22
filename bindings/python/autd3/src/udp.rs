use std::num::NonZeroUsize;
use std::time::Duration;

use autd3_python_capsule::extract::{duration_to_py, extract_duration};
use autd3_rs::Interface as CoreInterface;
use autd3_rs::udp::TransportOption as CoreOption;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;

pub(crate) fn none_or_duration(obj: &Bound<'_, PyAny>) -> PyResult<Option<Duration>> {
    if obj.is_none() {
        return Ok(None);
    }
    extract_duration(obj).map(Some)
}

#[pyclass(name = "Interface", module = "autd3", eq, hash, frozen, from_py_object)]
#[derive(Clone, PartialEq, Eq)]
pub struct Interface(pub(crate) CoreInterface);

impl core::hash::Hash for Interface {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        format!("{:?}", self.0).hash(state);
    }
}

#[pymethods]
impl Interface {
    #[classattr]
    #[pyo3(name = "Auto")]
    fn auto() -> Self {
        Self(CoreInterface::Auto)
    }

    #[staticmethod]
    #[pyo3(name = "Name")]
    fn name(name: String) -> Self {
        Self(CoreInterface::Name(name))
    }

    #[staticmethod]
    #[pyo3(name = "Addr")]
    fn addr(addr: &str) -> PyResult<Self> {
        addr.parse()
            .map(|addr| Self(CoreInterface::Addr(addr)))
            .map_err(|_| {
                PyValueError::new_err(format!(
                    "{addr:?} is not an IPv6 socket address such as \"[::1]:44336\""
                ))
            })
    }

    #[pyo3(name = "name")]
    fn interface_name(&self) -> Option<&str> {
        match &self.0 {
            CoreInterface::Name(name) => Some(name),
            CoreInterface::Auto | CoreInterface::Addr(_) => None,
            _ => unreachable!(),
        }
    }

    fn __repr__(&self) -> String {
        match &self.0 {
            CoreInterface::Name(name) => format!("Interface.Name({name:?})"),
            CoreInterface::Addr(addr) => format!("Interface.Addr({:?})", addr.to_string()),
            CoreInterface::Auto => "Interface.Auto".to_owned(),
            _ => unreachable!(),
        }
    }
}

fn extract_iface(obj: &Bound<'_, PyAny>) -> PyResult<CoreInterface> {
    if let Ok(iface) = obj.extract::<Interface>() {
        return Ok(iface.0);
    }
    if let Ok(name) = obj.extract::<String>() {
        return Ok(CoreInterface::Name(name));
    }
    Err(PyTypeError::new_err(
        "iface must be an Interface or an interface name",
    ))
}

#[pyclass(name = "TransportOption", module = "autd3", eq)]
#[derive(PartialEq)]
pub struct TransportOption(pub(crate) CoreOption);

#[pymethods]
impl TransportOption {
    #[new]
    #[pyo3(signature = (
        iface = CoreOption::default().iface,
        open_timeout = CoreOption::default().open_timeout,
        lost_timeout = CoreOption::default().lost_timeout,
        ack_timeout = CoreOption::default().ack_timeout,
        send_buffer_bytes = CoreOption::default().send_buffer_bytes,
    ))]
    fn new(
        #[pyo3(from_py_with = extract_iface)] iface: CoreInterface,
        #[pyo3(from_py_with = extract_duration)] open_timeout: Duration,
        #[pyo3(from_py_with = extract_duration)] lost_timeout: Duration,
        #[pyo3(from_py_with = extract_duration)] ack_timeout: Duration,
        send_buffer_bytes: Option<NonZeroUsize>,
    ) -> Self {
        Self(CoreOption {
            iface,
            open_timeout,
            lost_timeout,
            ack_timeout,
            send_buffer_bytes,
            ..CoreOption::default()
        })
    }

    #[getter]
    fn iface(&self) -> Interface {
        Interface(self.0.iface.clone())
    }

    #[getter]
    fn open_timeout<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        duration_to_py(py, self.0.open_timeout)
    }

    #[getter]
    fn lost_timeout<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        duration_to_py(py, self.0.lost_timeout)
    }

    #[getter]
    fn ack_timeout<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        duration_to_py(py, self.0.ack_timeout)
    }

    #[getter]
    fn send_buffer_bytes(&self) -> Option<NonZeroUsize> {
        self.0.send_buffer_bytes
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
}
