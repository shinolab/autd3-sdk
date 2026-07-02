use core::num::{NonZeroU32, NonZeroUsize};

use autd3_rs::{ClientConfig as CoreClientConfig, CoreId, ThreadPriority, ThreadPriorityValue};
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
        timeout_cycles = None,
        max_inflight = None,
        send_interval_cycles = None,
        max_resync_rounds = None,
        reset_resend_cycles = None,
        rt_priority = None,
        rt_affinity = None,
        validate_state = None,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        low_latency: bool,
        timeout_cycles: Option<u32>,
        max_inflight: Option<usize>,
        send_interval_cycles: Option<u32>,
        max_resync_rounds: Option<u32>,
        reset_resend_cycles: Option<u32>,
        rt_priority: Option<u8>,
        rt_affinity: Option<usize>,
        validate_state: Option<bool>,
    ) -> PyResult<Self> {
        let mut inner = CoreClientConfig {
            low_latency,
            ..CoreClientConfig::default()
        };
        if let Some(v) = timeout_cycles {
            inner.timeout_cycles = v;
        }
        if let Some(v) = max_inflight {
            inner.max_inflight = NonZeroUsize::new(v)
                .ok_or_else(|| PyValueError::new_err("max_inflight must be >= 1"))?;
        }
        if let Some(v) = send_interval_cycles {
            inner.send_interval_cycles = NonZeroU32::new(v)
                .ok_or_else(|| PyValueError::new_err("send_interval_cycles must be >= 1"))?;
        }
        if let Some(v) = max_resync_rounds {
            inner.max_resync_rounds = NonZeroU32::new(v)
                .ok_or_else(|| PyValueError::new_err("max_resync_rounds must be >= 1"))?;
        }
        if let Some(v) = reset_resend_cycles {
            inner.reset_resend_cycles = v;
        }
        if let Some(v) = rt_priority {
            let value = ThreadPriorityValue::try_from(v)
                .map_err(|e| PyValueError::new_err(format!("invalid rt_priority: {e}")))?;
            inner.rt_priority = Some(ThreadPriority::Crossplatform(value));
        }
        if let Some(v) = rt_affinity {
            inner.rt_affinity = Some(CoreId { id: v });
        }
        if let Some(v) = validate_state {
            inner.validate_state = v;
        }
        Ok(Self { inner })
    }
}
