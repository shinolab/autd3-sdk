use core::num::NonZeroUsize;
use core::time::Duration;

use autd3_python_capsule::extract::extract_duration;

use autd3_rs::ClientConfig as CoreClientConfig;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

#[pyclass(name = "ClientConfig", module = "autd3")]
#[derive(Clone)]
pub struct ClientConfig(pub(crate) CoreClientConfig);

#[pymethods]
impl ClientConfig {
    #[new]
    #[pyo3(signature = (
        ack_timeout = CoreClientConfig::default().ack_timeout,
        max_inflight = CoreClientConfig::default().max_inflight.get(),
    ))]
    fn new(
        #[pyo3(from_py_with = extract_duration)] ack_timeout: Duration,
        max_inflight: usize,
    ) -> PyResult<Self> {
        Ok(Self(CoreClientConfig {
            ack_timeout,
            max_inflight: NonZeroUsize::new(max_inflight)
                .ok_or_else(|| PyValueError::new_err("max_inflight must be >= 1"))?,
        }))
    }

    #[getter]
    fn ack_timeout<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        autd3_python_capsule::extract::duration_to_py(py, self.0.ack_timeout)
    }

    #[getter]
    fn max_inflight(&self) -> NonZeroUsize {
        self.0.max_inflight
    }
}
