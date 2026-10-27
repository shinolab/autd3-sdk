use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::datagram::{Expand, Frame, command_of};
use crate::future::{Completed, future_into_py};
use crate::ops::SysTime;
use autd3_python_capsule::extract::{duration_to_py, extract_duration, geometry_to_py};
use autd3_python_capsule::{capsule_of, client_pyerr, client_pyerr_gil, geometry_from_capsule};
use autd3_rs::Geometry;

use pyo3::exceptions::{PyIndexError, PyValueError};
use pyo3::prelude::*;
use pyo3::sync::MutexExt;

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

#[pyclass(name = "DeviceStatus", module = "autd3", eq, hash, frozen)]
#[derive(PartialEq, Eq)]
pub struct DeviceStatus(autd3_rs::DeviceStatus);

impl core::hash::Hash for DeviceStatus {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        DeviceState(self.0.state).hash(state);
        self.0.ptp_locked.hash(state);
        self.0.failsafe_active.hash(state);
    }
}

#[pymethods]
impl DeviceStatus {
    #[getter]
    fn state(&self) -> DeviceState {
        DeviceState(self.0.state)
    }

    #[getter]
    fn ptp_locked(&self) -> bool {
        self.0.ptp_locked
    }

    #[getter]
    fn failsafe_active(&self) -> bool {
        self.0.failsafe_active
    }

    fn __repr__(&self) -> String {
        format!(
            "DeviceStatus(state={}, ptp_locked={}, failsafe_active={})",
            DeviceState(self.0.state).__repr__(),
            self.0.ptp_locked,
            self.0.failsafe_active
        )
    }
}

#[pyclass(name = "Version", module = "autd3", eq, hash, frozen)]
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

#[pyclass(name = "FirmwareVersion", module = "autd3", eq, hash, frozen)]
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

#[pyclass(name = "FpgaState", module = "autd3", eq, hash, frozen)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct FpgaState(pub(crate) autd3_rs::FpgaState);

impl core::hash::Hash for FpgaState {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.0.0.hash(state);
    }
}

#[pymethods]
impl FpgaState {
    fn raw(&self) -> u8 {
        self.0.0
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

    fn is_pattern_stopped(&self) -> bool {
        self.0.is_pattern_stopped()
    }

    fn is_mod_stopped(&self) -> bool {
        self.0.is_mod_stopped()
    }

    fn is_transition_pending(&self) -> bool {
        self.0.is_transition_pending()
    }

    fn __repr__(&self) -> String {
        format!(
            "FpgaState(raw=0x{:02X}, thermal_asserted={}, mod_bank={:?}, pattern_bank={:?}, pattern_mode={}, pattern_stopped={}, mod_stopped={}, transition_pending={})",
            self.0.0,
            self.0.is_thermal_asserted(),
            self.0.current_mod_bank(),
            self.0.current_pattern_bank(),
            self.0.is_pattern_mode(),
            self.0.is_pattern_stopped(),
            self.0.is_mod_stopped(),
            self.0.is_transition_pending()
        )
    }
}

#[pyclass(name = "Telemetry", module = "autd3", frozen, eq)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Telemetry(pub(crate) autd3_rs::Telemetry);

#[pymethods]
impl Telemetry {
    #[getter]
    fn fifo_drop(&self) -> u32 {
        self.0.fifo_drop
    }

    #[getter]
    fn dispatch_error(&self) -> u32 {
        self.0.dispatch_error
    }

    #[getter]
    fn processed(&self) -> u32 {
        self.0.processed
    }

    #[getter]
    fn send_failure(&self) -> u32 {
        self.0.send_failure
    }

    #[getter]
    fn failsafe(&self) -> u16 {
        self.0.failsafe
    }

    #[getter]
    fn ptp_unlock_failsafe(&self) -> u16 {
        self.0.ptp_unlock_failsafe
    }

    #[getter]
    fn sync_resync(&self) -> u8 {
        self.0.sync_resync
    }

    #[getter]
    fn boot_failure(&self) -> u8 {
        self.0.boot_failure
    }

    #[getter]
    fn cpu_watchdog(&self) -> u8 {
        self.0.cpu_watchdog
    }

    #[getter]
    fn exec_late(&self) -> u32 {
        self.0.exec_late
    }

    fn __repr__(&self) -> String {
        format!(
            "Telemetry(fifo_drop={}, dispatch_error={}, processed={}, send_failure={}, failsafe={}, ptp_unlock_failsafe={}, sync_resync={}, boot_failure={}, cpu_watchdog={}, exec_late={})",
            self.0.fifo_drop,
            self.0.dispatch_error,
            self.0.processed,
            self.0.send_failure,
            self.0.failsafe,
            self.0.ptp_unlock_failsafe,
            self.0.sync_resync,
            self.0.boot_failure,
            self.0.cpu_watchdog,
            self.0.exec_late,
        )
    }
}

#[pyclass(name = "ExecAt", module = "autd3", eq, hash, frozen, from_py_object)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExecAt(pub(crate) autd3_rs::ExecAt);

#[pymethods]
impl ExecAt {
    #[classattr]
    #[pyo3(name = "Immediate")]
    fn immediate() -> Self {
        Self(autd3_rs::ExecAt::Immediate)
    }

    #[staticmethod]
    #[pyo3(name = "Lead")]
    fn lead(lead: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self(autd3_rs::ExecAt::Lead(extract_duration(lead)?)))
    }

    #[staticmethod]
    #[pyo3(name = "At")]
    fn at(sys_time: SysTime) -> Self {
        Self(autd3_rs::ExecAt::At(sys_time.0))
    }

    fn __repr__(&self) -> String {
        format!("ExecAt.{:?}", self.0)
    }
}

#[pyclass(name = "Client", module = "autd3")]
pub struct Client {
    inner: Arc<autd3_rs::Client>,
    geometry: Geometry,
    closed: AtomicBool,
}

fn frame_out_of_range() -> PyErr {
    PyIndexError::new_err("frame index out of range")
}

#[pymethods]
impl Client {
    #[staticmethod]
    fn open(
        py: Python<'_>,
        geometry: &Bound<'_, PyAny>,
        option: &crate::udp::TransportOption,
    ) -> PyResult<Client> {
        let geometry = geometry_from_capsule(&capsule_of(geometry)?)?.clone();
        let option = &option.0;
        let client = py
            .detach(|| autd3_rs::Client::open(&geometry, option))
            .map_err(|e| client_pyerr(py, &e))?;
        Ok(Client {
            inner: Arc::new(client),
            geometry,
            closed: AtomicBool::new(false),
        })
    }

    fn num_devices(&self) -> usize {
        self.inner.num_devices()
    }

    fn device_status(&self, py: Python<'_>) -> PyResult<Vec<DeviceStatus>> {
        self.inner
            .device_status()
            .map(|status| status.into_iter().map(DeviceStatus).collect())
            .map_err(|e| client_pyerr(py, &e))
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

    fn ack_timeout<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        duration_to_py(py, self.inner.ack_timeout())
    }

    fn set_ack_timeout(&self, #[pyo3(from_py_with = extract_duration)] timeout: Duration) {
        self.inner.set_ack_timeout(timeout);
    }

    fn lost_timeout<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        duration_to_py(py, self.inner.lost_timeout())
    }

    fn set_lost_timeout(&self, #[pyo3(from_py_with = extract_duration)] timeout: Duration) {
        self.inner.set_lost_timeout(timeout);
    }

    fn send_heartbeat(&self, py: Python<'_>) -> PyResult<()> {
        py.detach(|| self.inner.send_heartbeat())
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
            let telemetry = client.read_telemetry().await.map_err(client_pyerr_gil)?;
            Ok(telemetry.into_iter().map(Telemetry).collect::<Vec<_>>())
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

    fn send_frame_at<'py>(
        &self,
        py: Python<'py>,
        frame: PyRef<'_, Frame>,
        exec_at: ExecAt,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = Arc::clone(&self.inner);
        let datagrams = Arc::clone(&frame.datagrams);
        let index = frame.index;
        future_into_py(py, async move {
            let frame = datagrams.frame(index).ok_or_else(frame_out_of_range)?;
            let fut = client
                .send_frame_at(frame, exec_at.0)
                .await
                .map_err(client_pyerr_gil)?;
            Ok(ResponseFuture(Mutex::new(Some(fut))))
        })
    }

    fn close(&self, py: Python<'_>) -> PyResult<()> {
        let result = py.detach(|| self.inner.close());
        self.closed.store(true, Ordering::Release);
        result.map_err(|e| client_pyerr(py, &e))
    }

    fn __enter__(slf: Bound<'_, Self>) -> Bound<'_, Self> {
        slf
    }

    #[pyo3(signature = (_exc_type = None, _exc_value = None, _traceback = None))]
    fn __exit__<'py>(
        &self,
        py: Python<'py>,
        _exc_type: Option<Bound<'py, PyAny>>,
        _exc_value: Option<Bound<'py, PyAny>>,
        _traceback: Option<Bound<'py, PyAny>>,
    ) -> PyResult<()> {
        if self.closed.load(Ordering::Acquire) {
            return Ok(());
        }
        self.close(py)
    }
}

#[pyclass(name = "Response", module = "autd3")]
pub struct Response(autd3_rs::Response);

#[pymethods]
impl Response {
    #[getter]
    fn error_codes(&self) -> Vec<u8> {
        self.0.error_codes().to_vec()
    }

    #[getter]
    fn reply_data(&self) -> Vec<Vec<u8>> {
        self.0.reply_data().to_vec()
    }

    fn reply_data_of(&self, device: usize) -> Vec<u8> {
        self.0.reply_data_of(device).to_vec()
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
            .lock_py_attached(py)
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
