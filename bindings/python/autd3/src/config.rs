use core::num::{NonZeroU32, NonZeroUsize};

use autd3_rs::ClientConfig as CoreClientConfig;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

#[pyclass(name = "ClientConfig", module = "autd3", skip_from_py_object)]
#[derive(Clone)]
pub struct ClientConfig(pub(crate) CoreClientConfig);

#[pymethods]
impl ClientConfig {
    #[new]
    #[pyo3(signature = (
        ack_timeout = None,
        max_inflight = None,
        max_resync_rounds = None,
        require_supported_firmware = None,
    ))]
    fn new(
        ack_timeout: Option<&Bound<'_, PyAny>>,
        max_inflight: Option<usize>,
        max_resync_rounds: Option<u32>,
        require_supported_firmware: Option<bool>,
    ) -> PyResult<Self> {
        let mut inner = CoreClientConfig::default();
        if let Some(v) = crate::udp::opt_duration(ack_timeout)? {
            if v.is_zero() {
                return Err(PyValueError::new_err(
                    "ack_timeout must be longer than zero",
                ));
            }
            inner.ack_timeout = v;
        }
        if let Some(v) = max_inflight {
            inner.max_inflight = NonZeroUsize::new(v)
                .ok_or_else(|| PyValueError::new_err("max_inflight must be >= 1"))?;
        }
        if let Some(v) = max_resync_rounds {
            inner.max_resync_rounds = NonZeroU32::new(v)
                .ok_or_else(|| PyValueError::new_err("max_resync_rounds must be >= 1"))?;
        }
        if let Some(v) = require_supported_firmware {
            inner.require_supported_firmware = v;
        }
        Ok(Self(inner))
    }

    #[getter]
    fn ack_timeout<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        autd3_python_capsule::extract::duration_to_py(py, self.0.ack_timeout)
    }

    #[getter]
    fn max_inflight(&self) -> NonZeroUsize {
        self.0.max_inflight
    }

    #[getter]
    fn max_resync_rounds(&self) -> NonZeroU32 {
        self.0.max_resync_rounds
    }

    #[getter]
    fn require_supported_firmware(&self) -> bool {
        self.0.require_supported_firmware
    }
}
