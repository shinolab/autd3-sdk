use core::num::{NonZeroU32, NonZeroUsize};

use autd3_rs::ClientConfig as CoreClientConfig;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

#[pyclass(name = "ClientConfig", module = "autd3", skip_from_py_object)]
#[derive(Clone)]
pub struct ClientConfig {
    pub(crate) inner: CoreClientConfig,
}

#[pymethods]
impl ClientConfig {
    #[new]
    #[pyo3(signature = (
        low_latency = false,
        ack_timeout = None,
        max_inflight = None,
        max_resync_rounds = None,
        validate_state = None,
        require_supported_firmware = None,
    ))]
    fn new(
        low_latency: bool,
        ack_timeout: Option<&Bound<'_, PyAny>>,
        max_inflight: Option<usize>,
        max_resync_rounds: Option<u32>,
        validate_state: Option<bool>,
        require_supported_firmware: Option<bool>,
    ) -> PyResult<Self> {
        let mut inner = CoreClientConfig {
            low_latency,
            ..CoreClientConfig::default()
        };
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
        if let Some(v) = validate_state {
            inner.validate_state = v;
        }
        if let Some(v) = require_supported_firmware {
            inner.require_supported_firmware = v;
        }
        Ok(Self { inner })
    }
}
