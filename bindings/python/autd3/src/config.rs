use core::num::{NonZeroU32, NonZeroUsize};

use autd3_rs::{
    ClientConfig as CoreClientConfig, CoreId, RtPriority as CoreRtPriority,
    RtSchedulePolicy as CoreRtSchedulePolicy,
};
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
        max_resync_rounds = None,
        reset_resend_cycles = None,
        rt_priority = CoreClientConfig::default().rt_priority.map(RtPriority),
        rt_policy = None,
        rt_affinity = None,
        validate_state = None,
        require_supported_firmware = None,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        low_latency: bool,
        timeout_cycles: Option<u32>,
        max_inflight: Option<usize>,
        max_resync_rounds: Option<u32>,
        reset_resend_cycles: Option<u32>,
        rt_priority: Option<RtPriority>,
        rt_policy: Option<RtSchedulePolicy>,
        rt_affinity: Option<usize>,
        validate_state: Option<bool>,
        require_supported_firmware: Option<bool>,
    ) -> PyResult<Self> {
        let mut inner = CoreClientConfig {
            low_latency,
            rt_priority: rt_priority.map(|v| v.0),
            ..CoreClientConfig::default()
        };
        if let Some(v) = timeout_cycles {
            inner.timeout_cycles = NonZeroU32::new(v)
                .ok_or_else(|| PyValueError::new_err("timeout_cycles must be >= 1"))?;
        }
        if let Some(v) = max_inflight {
            inner.max_inflight = NonZeroUsize::new(v)
                .ok_or_else(|| PyValueError::new_err("max_inflight must be >= 1"))?;
        }
        if let Some(v) = max_resync_rounds {
            inner.max_resync_rounds = NonZeroU32::new(v)
                .ok_or_else(|| PyValueError::new_err("max_resync_rounds must be >= 1"))?;
        }
        if let Some(v) = reset_resend_cycles {
            inner.reset_resend_cycles = NonZeroU32::new(v)
                .ok_or_else(|| PyValueError::new_err("reset_resend_cycles must be >= 1"))?;
        }
        if let Some(v) = rt_policy {
            inner.rt_policy = v.0;
        }
        if let Some(v) = rt_affinity {
            inner.rt_affinity = Some(CoreId { id: v });
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

#[pyclass(
    name = "RtSchedulePolicy",
    module = "autd3",
    frozen,
    eq,
    from_py_object
)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct RtSchedulePolicy(pub(crate) CoreRtSchedulePolicy);

#[pymethods]
impl RtSchedulePolicy {
    #[classattr]
    #[pyo3(name = "Normal")]
    fn normal() -> Self {
        Self(CoreRtSchedulePolicy::Normal)
    }

    #[classattr]
    #[pyo3(name = "Fifo")]
    fn fifo() -> Self {
        Self(CoreRtSchedulePolicy::Fifo)
    }

    #[classattr]
    #[pyo3(name = "RoundRobin")]
    fn round_robin() -> Self {
        Self(CoreRtSchedulePolicy::RoundRobin)
    }

    fn __repr__(&self) -> String {
        format!("RtSchedulePolicy.{:?}", self.0)
    }
}

#[pyclass(name = "RtPriority", module = "autd3", frozen, eq, from_py_object)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct RtPriority(pub(crate) CoreRtPriority);

#[pymethods]
impl RtPriority {
    #[new]
    fn new(value: u8) -> PyResult<Self> {
        CoreRtPriority::new(value)
            .map(Self)
            .ok_or_else(|| PyValueError::new_err(format!("invalid rt_priority: {value}")))
    }

    #[classattr]
    #[pyo3(name = "MIN")]
    fn min() -> Self {
        Self(CoreRtPriority::MIN)
    }

    #[classattr]
    #[pyo3(name = "MAX")]
    fn max() -> Self {
        Self(CoreRtPriority::MAX)
    }

    #[getter]
    fn value(&self) -> Option<u8> {
        self.0.value()
    }

    fn __repr__(&self) -> String {
        if let Some(v) = self.0.value() {
            format!("RtPriority({v})")
        } else if self.0 == CoreRtPriority::MIN {
            "RtPriority.MIN".to_owned()
        } else if self.0 == CoreRtPriority::MAX {
            "RtPriority.MAX".to_owned()
        } else {
            format!("{:?}", self.0)
        }
    }
}
