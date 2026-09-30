use std::sync::{Arc, Mutex};

use crate::config::ClientConfig;
use crate::datagram::{DatagramBuilder, Frame};
use crate::driver::Connector;
use crate::future::{completed_into_py, future_into_py};
use autd3_python_capsule::{
    ClientBackend, ResponseToken, capsule_of, geometry_from_capsule, to_pyerr, to_pyerr_gil,
};
use autd3_rs::Geometry;
use autd3_rs::udp::StateChecker;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

#[pyclass(name = "DeviceStatus", module = "autd3")]
pub struct DeviceStatus {
    #[pyo3(get)]
    device_states: Vec<String>,
    #[pyo3(get)]
    all_ready: bool,
    #[pyo3(get)]
    any_lost: bool,
}

#[pymethods]
impl DeviceStatus {
    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        other
            .extract::<PyRef<'_, Self>>()
            .is_ok_and(|o| self.device_states == o.device_states)
    }

    fn __repr__(&self) -> String {
        format!(
            "DeviceStatus(devices={:?}, all_ready={}, any_lost={})",
            self.device_states, self.all_ready, self.any_lost
        )
    }
}

#[pyclass(name = "FpgaState", module = "autd3")]
pub struct FpgaState(pub(crate) autd3_rs::FpgaState);

#[pymethods]
impl FpgaState {
    fn raw(&self) -> u8 {
        self.0.raw()
    }

    fn is_thermal_asserted(&self) -> bool {
        self.0.is_thermal_asserted()
    }

    fn is_pattern_stopped(&self) -> bool {
        self.0.is_pattern_stopped()
    }

    fn is_mod_stopped(&self) -> bool {
        self.0.is_mod_stopped()
    }

    fn is_transition_pending(&self) -> bool {
        self.0.is_transition_pending()
    }

    fn reads_enabled(&self) -> bool {
        self.0.reads_enabled()
    }

    fn __repr__(&self) -> String {
        format!(
            "FpgaState(raw=0x{:02X}, thermal_asserted={}, pattern_stopped={}, mod_stopped={}, transition_pending={}, reads_enabled={})",
            self.0.raw(),
            self.0.is_thermal_asserted(),
            self.0.is_pattern_stopped(),
            self.0.is_mod_stopped(),
            self.0.is_transition_pending(),
            self.0.reads_enabled()
        )
    }
}

#[pyclass(
    name = "TelemetryCounters",
    module = "autd3",
    frozen,
    eq,
    skip_from_py_object
)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct TelemetryCounters(pub(crate) autd3_rs::TelemetryCounters);

#[pymethods]
impl TelemetryCounters {
    fn get(&self, counter: crate::ops::Telemetry) -> u32 {
        self.0.get(counter.0)
    }

    fn __getitem__(&self, counter: crate::ops::Telemetry) -> u32 {
        self.0.get(counter.0)
    }

    fn as_list(&self) -> Vec<u32> {
        self.0.as_array().to_vec()
    }

    fn __repr__(&self) -> String {
        let fields = autd3_rs::Telemetry::ALL
            .iter()
            .map(|&counter| format!("{counter:?}={}", self.0.get(counter)))
            .collect::<Vec<_>>()
            .join(", ");
        format!("TelemetryCounters({fields})")
    }
}

#[pyclass(name = "Client", module = "autd3")]
pub struct Client {
    backend: Arc<dyn ClientBackend>,
    geometry: Arc<Geometry>,
}

#[pymethods]
impl Client {
    #[staticmethod]
    fn open<'py>(
        py: Python<'py>,
        geometry: &Bound<'py, PyAny>,
        connector: &Connector,
        config: &ClientConfig,
    ) -> PyResult<Bound<'py, PyAny>> {
        let geometry = geometry_from_capsule(&capsule_of(geometry)?)?.clone();
        let connector = connector.take(py)?;
        let config = config.inner;
        future_into_py(py, async move {
            let geometry_for_client = Arc::new(geometry.clone());
            let backend = crate::udp::open(geometry, connector, config)
                .await
                .map_err(to_pyerr_gil)?;
            Ok(Client {
                backend: Arc::from(backend),
                geometry: geometry_for_client,
            })
        })
    }

    fn num_devices(&self) -> usize {
        self.backend.num_devices()
    }

    fn geometry<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let capsule = autd3_python_capsule::geometry_into_capsule(py, (*self.geometry).clone())?;
        py.import("autd3_core")?
            .getattr("Geometry")?
            .call_method1("_from_capsule", (capsule,))
    }

    fn datagram_builder(&self) -> DatagramBuilder {
        DatagramBuilder::with_backend(Arc::clone(&self.geometry), Some(Arc::clone(&self.backend)))
    }

    fn read_firmware_version<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let backend = Arc::clone(&self.backend);
        future_into_py(py, async move {
            backend.read_firmware_version().await.map_err(to_pyerr_gil)
        })
    }

    fn read_fpga_state<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let backend = Arc::clone(&self.backend);
        future_into_py(py, async move {
            let states = backend.read_fpga_state().await.map_err(to_pyerr_gil)?;
            Ok(states
                .into_iter()
                .map(|s| FpgaState(autd3_rs::FpgaState(s)))
                .collect::<Vec<_>>())
        })
    }

    fn read_error_detail<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let backend = Arc::clone(&self.backend);
        future_into_py(py, async move {
            backend.read_error_detail().await.map_err(to_pyerr_gil)
        })
    }

    fn read_telemetry<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let backend = Arc::clone(&self.backend);
        future_into_py(py, async move {
            let counters = backend.read_telemetry().await.map_err(to_pyerr_gil)?;
            Ok(counters
                .into_iter()
                .map(TelemetryCounters)
                .collect::<Vec<_>>())
        })
    }

    fn send<'py>(&self, py: Python<'py>, frame: PyRef<'_, Frame>) -> PyResult<Bound<'py, PyAny>> {
        let backend = Arc::clone(&self.backend);
        let datagrams = Arc::clone(&frame.datagrams);
        let index = frame.index;
        future_into_py(py, async move {
            let token = backend.send(datagrams, index).await.map_err(to_pyerr_gil)?;
            Ok(ResponseFuture {
                token: Mutex::new(Some(token)),
            })
        })
    }

    fn send_checked<'py>(
        &self,
        py: Python<'py>,
        frame: PyRef<'_, Frame>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let backend = Arc::clone(&self.backend);
        let datagrams = Arc::clone(&frame.datagrams);
        let index = frame.index;
        future_into_py(py, async move {
            backend
                .send_checked(datagrams, Some(index))
                .await
                .map_err(to_pyerr_gil)
        })
    }

    fn stop<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let backend = Arc::clone(&self.backend);
        future_into_py(
            py,
            async move { backend.stop().await.map_err(to_pyerr_gil) },
        )
    }

    fn close<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let backend = Arc::clone(&self.backend);
        future_into_py(
            py,
            async move { backend.close().await.map_err(to_pyerr_gil) },
        )
    }

    fn __aenter__(slf: Bound<'_, Self>) -> PyResult<Bound<'_, PyAny>> {
        completed_into_py(slf.py(), slf.clone().into_any())
    }

    #[pyo3(signature = (_exc_type = None, _exc_value = None, _traceback = None))]
    fn __aexit__<'py>(
        &self,
        py: Python<'py>,
        _exc_type: Option<Bound<'py, PyAny>>,
        _exc_value: Option<Bound<'py, PyAny>>,
        _traceback: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.close(py)
    }
}

#[pyclass(name = "Response", module = "autd3")]
pub struct Response {
    inner: autd3_rs::Response,
}

#[pymethods]
impl Response {
    #[getter]
    fn status(&self) -> Vec<u8> {
        self.inner.status().to_vec()
    }

    #[getter]
    fn values(&self) -> Vec<Vec<u8>> {
        self.inner.values().to_vec()
    }

    fn value(&self, device: usize) -> Vec<u8> {
        self.inner.value(device).to_vec()
    }

    fn check(&self, py: Python<'_>) -> PyResult<()> {
        self.inner.check().map_err(|e| to_pyerr(py, e))
    }
}

#[pyclass(name = "ResponseFuture", module = "autd3")]
pub struct ResponseFuture {
    token: Mutex<Option<ResponseToken>>,
}

#[pymethods]
impl ResponseFuture {
    fn __await__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let token = self
            .token
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
            .ok_or_else(|| PyValueError::new_err("ResponseFuture has already been awaited"))?;
        let awaitable = future_into_py(py, async move {
            let inner = token.wait().await.map_err(to_pyerr_gil)?;
            Ok(Response { inner })
        })?;
        awaitable.getattr("__await__")?.call0()
    }
}

#[pyclass(name = "Checker", module = "autd3")]
pub struct Checker {
    pub(crate) inner: Arc<Mutex<StateChecker>>,
}

#[pymethods]
impl Checker {
    fn check(&self, py: Python<'_>) -> PyResult<DeviceStatus> {
        let status = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .check()
            .map_err(|e| to_pyerr(py, e))?;
        Ok(DeviceStatus {
            device_states: status.devices().iter().map(ToString::to_string).collect(),
            all_ready: status.all_ready(),
            any_lost: status.any_lost(),
        })
    }
}
