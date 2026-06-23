use autd3_rs::ClientConfig as CoreClientConfig;
use pyo3::prelude::*;

#[pyclass(name = "ClientConfig", module = "autd3", skip_from_py_object)]
#[derive(Clone)]
pub struct ClientConfig {
    pub(crate) inner: CoreClientConfig,
}

#[pymethods]
impl ClientConfig {
    #[new]
    #[pyo3(signature = (low_latency = false))]
    fn new(low_latency: bool) -> Self {
        Self {
            inner: CoreClientConfig {
                low_latency,
                ..CoreClientConfig::default()
            },
        }
    }
}
