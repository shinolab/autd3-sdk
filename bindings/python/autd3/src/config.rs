use core::num::{NonZeroU32, NonZeroUsize};
use core::time::Duration;

use autd3_python_capsule::extract::extract_duration;

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
        ack_timeout = CoreClientConfig::default().ack_timeout,
        max_inflight = CoreClientConfig::default().max_inflight.get(),
        max_resync_rounds = CoreClientConfig::default().max_resync_rounds.get(),
        require_supported_firmware = CoreClientConfig::default().require_supported_firmware,
        resynchronize_on_recovery = CoreClientConfig::default().resynchronize_on_recovery,
        release_failsafe_on_recovery = CoreClientConfig::default().release_failsafe_on_recovery,
    ))]
    fn new(
        #[pyo3(from_py_with = extract_duration)] ack_timeout: Duration,
        max_inflight: usize,
        max_resync_rounds: u32,
        require_supported_firmware: bool,
        resynchronize_on_recovery: Option<NonZeroU32>,
        release_failsafe_on_recovery: Option<NonZeroU32>,
    ) -> PyResult<Self> {
        Ok(Self(CoreClientConfig {
            ack_timeout,
            max_inflight: NonZeroUsize::new(max_inflight)
                .ok_or_else(|| PyValueError::new_err("max_inflight must be >= 1"))?,
            max_resync_rounds: NonZeroU32::new(max_resync_rounds)
                .ok_or_else(|| PyValueError::new_err("max_resync_rounds must be >= 1"))?,
            require_supported_firmware,
            resynchronize_on_recovery,
            release_failsafe_on_recovery,
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

    #[getter]
    fn max_resync_rounds(&self) -> NonZeroU32 {
        self.0.max_resync_rounds
    }

    #[getter]
    fn require_supported_firmware(&self) -> bool {
        self.0.require_supported_firmware
    }

    #[getter]
    fn resynchronize_on_recovery(&self) -> Option<NonZeroU32> {
        self.0.resynchronize_on_recovery
    }

    #[getter]
    fn release_failsafe_on_recovery(&self) -> Option<NonZeroU32> {
        self.0.release_failsafe_on_recovery
    }
}
