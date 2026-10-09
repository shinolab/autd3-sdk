use std::sync::{Arc, Mutex};

use crate::config::ClientConfig;
use crate::datagram::{Expand, Frame, command_of};
use crate::future::{Completed, completed_into_py, future_into_py};
use crate::ops::SysTime;
use autd3_python_capsule::extract::geometry_to_py;
use autd3_python_capsule::{capsule_of, client_pyerr, client_pyerr_gil, geometry_from_capsule};
use autd3_rs::Geometry;
use autd3_rs::udp::StateChecker as CoreStateChecker;

use crate::udp::TransportOption;
use pyo3::exceptions::{PyIndexError, PyValueError};
use pyo3::prelude::*;

#[pyclass(
    name = "DeviceState",
    module = "autd3",
    eq,
    hash,
    frozen,
    from_py_object
)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DeviceState(pub(crate) autd3_rs::DeviceState);

impl core::hash::Hash for DeviceState {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(&self.0).hash(state);
    }
}

#[pymethods]
impl DeviceState {
    #[classattr]
    #[pyo3(name = "Ready")]
    fn ready() -> Self {
        Self(autd3_rs::DeviceState::Ready)
    }

    #[classattr]
    #[pyo3(name = "Syncing")]
    fn syncing() -> Self {
        Self(autd3_rs::DeviceState::Syncing)
    }

    #[classattr]
    #[pyo3(name = "Lost")]
    fn lost() -> Self {
        Self(autd3_rs::DeviceState::Lost)
    }

    fn __str__(&self) -> String {
        self.0.to_string()
    }

    fn __repr__(&self) -> String {
        format!("DeviceState.{:?}", self.0)
    }
}

#[pyclass(name = "DeviceStatus", module = "autd3", eq, frozen)]
#[derive(PartialEq, Eq)]
pub struct DeviceStatus(autd3_rs::DeviceStatus);

#[pymethods]
impl DeviceStatus {
    #[getter]
    fn devices(&self) -> Vec<DeviceState> {
        self.0.devices().iter().copied().map(DeviceState).collect()
    }

    #[getter]
    fn all_ready(&self) -> bool {
        self.0.all_ready()
    }

    #[getter]
    fn any_lost(&self) -> bool {
        self.0.any_lost()
    }

    fn __repr__(&self) -> String {
        format!(
            "DeviceStatus(devices={:?}, all_ready={}, any_lost={})",
            self.0.devices(),
            self.0.all_ready(),
            self.0.any_lost()
        )
    }
}

#[pyclass(name = "BusStats", module = "autd3", frozen)]
pub struct BusStats(autd3_rs::BusStats);

#[pymethods]
impl BusStats {
    fn frames(&self) -> u64 {
        self.0.frames()
    }

    fn resets(&self) -> u64 {
        self.0.resets()
    }

    fn heartbeats(&self) -> u64 {
        self.0.heartbeats()
    }

    fn missed_replies(&self) -> u64 {
        self.0.missed_replies()
    }

    fn acked_frames(&self) -> u64 {
        self.0.acked_frames()
    }

    fn worst_ack_latency_ns(&self) -> u64 {
        self.0.worst_ack_latency_ns()
    }

    fn mean_ack_latency_ns(&self) -> u64 {
        self.0.mean_ack_latency_ns()
    }

    fn __repr__(&self) -> String {
        format!(
            "BusStats(frames={}, resets={}, heartbeats={}, missed_replies={}, acked_frames={}, worst_ack_latency_ns={}, mean_ack_latency_ns={})",
            self.0.frames(),
            self.0.resets(),
            self.0.heartbeats(),
            self.0.missed_replies(),
            self.0.acked_frames(),
            self.0.worst_ack_latency_ns(),
            self.0.mean_ack_latency_ns()
        )
    }
}

#[pyclass(
    name = "Version",
    module = "autd3",
    eq,
    hash,
    frozen,
    skip_from_py_object
)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Version(autd3_rs::Version);

impl core::hash::Hash for Version {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        (self.0.major, self.0.minor, self.0.patch).hash(state);
    }
}

#[pymethods]
impl Version {
    #[getter]
    fn major(&self) -> u8 {
        self.0.major
    }

    #[getter]
    fn minor(&self) -> u8 {
        self.0.minor
    }

    #[getter]
    fn patch(&self) -> u8 {
        self.0.patch
    }

    fn is_unknown(&self) -> bool {
        self.0.is_unknown()
    }

    fn __str__(&self) -> String {
        self.0.to_string()
    }

    fn __repr__(&self) -> String {
        format!("Version({})", self.0)
    }
}

#[pyclass(
    name = "FirmwareVersion",
    module = "autd3",
    eq,
    hash,
    frozen,
    skip_from_py_object
)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct FirmwareVersion(autd3_rs::FirmwareVersion);

impl core::hash::Hash for FirmwareVersion {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        Version(self.0.cpu).hash(state);
        Version(self.0.fpga).hash(state);
    }
}

#[pymethods]
impl FirmwareVersion {
    #[classattr]
    const SUPPORTED_SERIES: (u8, u8) = autd3_rs::FirmwareVersion::SUPPORTED_SERIES;

    #[getter]
    fn cpu(&self) -> Version {
        Version(self.0.cpu)
    }

    #[getter]
    fn fpga(&self) -> Version {
        Version(self.0.fpga)
    }

    fn is_emulator(&self) -> bool {
        self.0.is_emulator()
    }

    fn is_supported(&self) -> bool {
        self.0.is_supported()
    }

    fn __str__(&self) -> String {
        self.0.to_string()
    }

    fn __repr__(&self) -> String {
        format!("FirmwareVersion({})", self.0)
    }
}

#[pyclass(
    name = "FpgaState",
    module = "autd3",
    eq,
    hash,
    frozen,
    skip_from_py_object
)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct FpgaState(pub(crate) autd3_rs::FpgaState);

impl core::hash::Hash for FpgaState {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.0.raw().hash(state);
    }
}

#[pymethods]
impl FpgaState {
    fn raw(&self) -> u8 {
        self.0.raw()
    }

    fn is_thermal_asserted(&self) -> bool {
        self.0.is_thermal_asserted()
    }

    fn current_mod_bank(&self) -> crate::ops::ModulationBank {
        crate::ops::ModulationBank(self.0.current_mod_bank())
    }

    fn current_pattern_bank(&self) -> crate::ops::PatternBank {
        crate::ops::PatternBank(self.0.current_pattern_bank())
    }

    fn is_pattern_mode(&self) -> bool {
        self.0.is_pattern_mode()
    }

    fn is_stm_mode(&self) -> bool {
        self.0.is_stm_mode()
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

    fn is_failsafe_active(&self) -> bool {
        self.0.is_failsafe_active()
    }

    fn __repr__(&self) -> String {
        format!(
            "FpgaState(raw=0x{:02X}, thermal_asserted={}, mod_bank={:?}, pattern_bank={:?}, pattern_mode={}, pattern_stopped={}, mod_stopped={}, transition_pending={}, failsafe_active={})",
            self.0.raw(),
            self.0.is_thermal_asserted(),
            self.0.current_mod_bank(),
            self.0.current_pattern_bank(),
            self.0.is_pattern_mode(),
            self.0.is_pattern_stopped(),
            self.0.is_mod_stopped(),
            self.0.is_transition_pending(),
            self.0.is_failsafe_active()
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
        self.0.to_vec()
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
    inner: Arc<autd3_rs::Client>,
    geometry: Arc<Geometry>,
}

fn frame_out_of_range() -> PyErr {
    PyIndexError::new_err("frame index out of range")
}

#[pymethods]
impl Client {
    #[staticmethod]
    fn open<'py>(
        py: Python<'py>,
        geometry: &Bound<'py, PyAny>,
        option: &TransportOption,
        config: &ClientConfig,
    ) -> PyResult<Bound<'py, PyAny>> {
        let geometry = geometry_from_capsule(&capsule_of(geometry)?)?.clone();
        let option = option.0.clone();
        let config = config.0;
        future_into_py(py, async move {
            let client = autd3_rs::Client::open(&geometry, &option, config)
                .await
                .map_err(client_pyerr_gil)?;
            Ok(Client {
                inner: Arc::new(client),
                geometry: Arc::new(geometry),
            })
        })
    }

    fn num_devices(&self) -> usize {
        self.inner.num_devices()
    }

    fn state_checker(&self) -> StateChecker {
        StateChecker(self.inner.state_checker())
    }

    fn bus_stats(&self) -> BusStats {
        BusStats(self.inner.bus_stats())
    }

    fn geometry<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        geometry_to_py(py, &self.geometry)
    }

    fn device_time_now(&self, py: Python<'_>) -> PyResult<SysTime> {
        self.inner
            .device_time_now()
            .map(SysTime)
            .map_err(|e| client_pyerr(py, &e))
    }

    fn read_firmware_version<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = Arc::clone(&self.inner);
        future_into_py(py, async move {
            let versions = client
                .read_firmware_version()
                .await
                .map_err(client_pyerr_gil)?;
            Ok(versions
                .into_iter()
                .map(FirmwareVersion)
                .collect::<Vec<_>>())
        })
    }

    fn read_fpga_state<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = Arc::clone(&self.inner);
        future_into_py(py, async move {
            let states = client.read_fpga_state().await.map_err(client_pyerr_gil)?;
            Ok(states.into_iter().map(FpgaState).collect::<Vec<_>>())
        })
    }

    fn read_telemetry<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = Arc::clone(&self.inner);
        future_into_py(py, async move {
            let counters = client.read_telemetry().await.map_err(client_pyerr_gil)?;
            Ok(counters
                .into_iter()
                .map(TelemetryCounters)
                .collect::<Vec<_>>())
        })
    }

    fn send<'py>(
        &self,
        py: Python<'py>,
        command: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = Arc::clone(&self.inner);
        let command = command_of(command, &self.geometry)?;
        future_into_py(py, async move {
            client
                .send(Expand(command.as_ref()))
                .await
                .map(|()| Completed)
                .map_err(client_pyerr_gil)
        })
    }

    fn send_streaming<'py>(
        &self,
        py: Python<'py>,
        command: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = Arc::clone(&self.inner);
        let command = command_of(command, &self.geometry)?;
        future_into_py(py, async move {
            let fut = client
                .send_streaming(Expand(command.as_ref()))
                .await
                .map_err(client_pyerr_gil)?;
            Ok(StreamFuture(Mutex::new(Some(fut))))
        })
    }

    fn send_frame<'py>(
        &self,
        py: Python<'py>,
        frame: PyRef<'_, Frame>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = Arc::clone(&self.inner);
        let datagrams = Arc::clone(&frame.datagrams);
        let index = frame.index;
        future_into_py(py, async move {
            let frame = datagrams.frame(index).ok_or_else(frame_out_of_range)?;
            let fut = client.send_frame(frame).await.map_err(client_pyerr_gil)?;
            Ok(ResponseFuture(Mutex::new(Some(fut))))
        })
    }

    fn silent_stop<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = Arc::clone(&self.inner);
        future_into_py(py, async move {
            client.silent_stop().await.map_err(client_pyerr_gil)
        })
    }

    fn close<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = Arc::clone(&self.inner);
        future_into_py(
            py,
            async move { client.close().await.map_err(client_pyerr_gil) },
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
pub struct Response(autd3_rs::Response);

#[pymethods]
impl Response {
    #[getter]
    fn status(&self) -> Vec<u8> {
        self.0.status().to_vec()
    }

    #[getter]
    fn values(&self) -> Vec<Vec<u8>> {
        self.0.values().to_vec()
    }

    fn value(&self, device: usize) -> Vec<u8> {
        self.0.value(device).to_vec()
    }

    fn check(&self, py: Python<'_>) -> PyResult<()> {
        self.0.check().map_err(|e| client_pyerr(py, &e))
    }
}

#[pyclass(name = "ResponseFuture", module = "autd3")]
pub struct ResponseFuture(Mutex<Option<autd3_rs::ResponseFuture>>);

#[pymethods]
impl ResponseFuture {
    fn __await__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let fut = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
            .ok_or_else(|| PyValueError::new_err("ResponseFuture has already been awaited"))?;
        let awaitable = future_into_py(py, async move {
            let inner = fut.await.map_err(client_pyerr_gil)?;
            Ok(Response(inner))
        })?;
        awaitable.getattr("__await__")?.call0()
    }
}

#[pyclass(name = "StreamFuture", module = "autd3")]
pub struct StreamFuture(Mutex<Option<autd3_rs::StreamFuture>>);

#[pymethods]
impl StreamFuture {
    fn __await__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let fut = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
            .ok_or_else(|| PyValueError::new_err("StreamFuture has already been awaited"))?;
        let awaitable = future_into_py(py, async move {
            fut.await.map(|()| Completed).map_err(client_pyerr_gil)
        })?;
        awaitable.getattr("__await__")?.call0()
    }
}

#[pyclass(name = "StateChecker", module = "autd3", frozen)]
pub struct StateChecker(pub(crate) CoreStateChecker);

#[pymethods]
impl StateChecker {
    fn check(&self, py: Python<'_>) -> PyResult<DeviceStatus> {
        self.0.check().map(DeviceStatus).map_err(|e| {
            autd3_python_capsule::to_pyerr_with_code(
                py,
                autd3_python_capsule::error_code::NETWORK,
                e.to_string(),
            )
        })
    }
}
