use crate::datagram::{Expand, command_of};
use crate::future::{Completed, future_into_py};
use crate::ops::SysTime;
use autd3_python_capsule::error::client as client_error;
use autd3_python_capsule::extract::geometry_to_py;
use autd3_python_capsule::{capsule_of, geometry_from_capsule};
use autd3_rs::{Frames, Geometry};

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

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
    fn exec_late(&self) -> u32 {
        self.0.exec_late
    }

    #[getter]
    fn sync_resync(&self) -> u8 {
        self.0.sync_resync
    }

    #[getter]
    fn cpu_watchdog(&self) -> u8 {
        self.0.cpu_watchdog
    }

    #[getter]
    fn boot_failure(&self) -> u8 {
        self.0.boot_failure
    }

    fn __repr__(&self) -> String {
        format!(
            "Telemetry(fifo_drop={}, exec_late={}, sync_resync={}, cpu_watchdog={}, boot_failure={})",
            self.0.fifo_drop,
            self.0.exec_late,
            self.0.sync_resync,
            self.0.cpu_watchdog,
            self.0.boot_failure,
        )
    }
}

#[pyclass(name = "DeviceReply", module = "autd3", frozen, eq)]
#[derive(PartialEq, Eq)]
pub struct DeviceReply(autd3_rs::DeviceReply);

#[pymethods]
impl DeviceReply {
    #[getter]
    fn device(&self) -> usize {
        self.0.device
    }

    #[getter]
    fn msg_id(&self) -> u16 {
        self.0.msg_id
    }

    #[getter]
    fn status(&self) -> u8 {
        self.0.status
    }

    #[getter]
    fn flags(&self) -> u8 {
        self.0.flags.bits()
    }

    #[getter]
    fn unit_id(&self) -> u8 {
        self.0.unit_id
    }

    #[getter]
    fn sys_time(&self) -> u64 {
        self.0.sys_time
    }

    #[getter]
    fn telemetry(&self) -> Telemetry {
        Telemetry(self.0.telemetry)
    }

    #[getter]
    fn fpga_state(&self) -> FpgaState {
        FpgaState(self.0.fpga_state)
    }

    #[getter]
    fn data<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.0.data)
    }

    fn __repr__(&self) -> String {
        format!(
            "DeviceReply(device={}, msg_id={}, status={}, flags=0x{:02X}, unit_id={}, sys_time={}, telemetry={}, fpga_state={}, data={:?})",
            self.0.device,
            self.0.msg_id,
            self.0.status,
            self.0.flags.bits(),
            self.0.unit_id,
            self.0.sys_time,
            Telemetry(self.0.telemetry).__repr__(),
            FpgaState(self.0.fpga_state).__repr__(),
            self.0.data,
        )
    }
}

#[pyclass(name = "Client", module = "autd3")]
pub struct Client {
    inner: Option<(autd3_binding_rt::Client, autd3_binding_rt::Heartbeat)>,
    geometry: Geometry,
}

impl Client {
    fn shared(&self) -> PyResult<&autd3_binding_rt::Client> {
        self.inner
            .as_ref()
            .map(|(client, _)| client)
            .ok_or_else(|| PyRuntimeError::new_err("the client has been released"))
    }
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
        let inner =
            crate::runtime::block_on(py, autd3_binding_rt::Client::open(&geometry, option))?
                .map_err(client_error)?;
        Ok(Client {
            inner: Some(inner),
            geometry,
        })
    }

    fn num_devices(&self) -> PyResult<usize> {
        self.shared()?;
        Ok(self.geometry.num_devices())
    }

    fn geometry<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        geometry_to_py(py, &self.geometry)
    }

    fn device_time_now(&self) -> PyResult<SysTime> {
        self.shared()?
            .device_time_now()
            .map(SysTime)
            .map_err(client_error)
    }

    fn read_firmware_version<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = self.shared()?.clone();
        future_into_py(py, async move {
            let versions = client.read_firmware_version().await.map_err(client_error)?;
            Ok(versions
                .into_iter()
                .map(FirmwareVersion)
                .collect::<Vec<_>>())
        })
    }

    fn read_state<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = self.shared()?.clone();
        future_into_py(py, async move {
            client
                .read_state()
                .await
                .map(Response)
                .map_err(client_error)
        })
    }

    fn send<'py>(
        &self,
        py: Python<'py>,
        command: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.shared()?.clone();
        let command = command_of(command, &self.geometry)?;
        future_into_py(py, async move {
            let frames = Frames::encode(client.geometry(), Expand(command.as_ref()))
                .map_err(client_error)?;
            client
                .send(&frames)
                .await
                .map(|()| Completed)
                .map_err(client_error)
        })
    }

    fn send_streaming<'py>(
        &self,
        py: Python<'py>,
        command: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.shared()?.clone();
        let command = command_of(command, &self.geometry)?;
        future_into_py(py, async move {
            let frames = Frames::encode(client.geometry(), Expand(command.as_ref()))
                .map_err(client_error)?;
            client
                .send_streaming(&frames)
                .await
                .map(|()| Completed)
                .map_err(client_error)
        })
    }

    fn __enter__(slf: Bound<'_, Self>) -> Bound<'_, Self> {
        slf
    }

    #[pyo3(signature = (_exc_type = None, _exc_value = None, _traceback = None))]
    fn __exit__(
        &mut self,
        _exc_type: Option<Bound<'_, PyAny>>,
        _exc_value: Option<Bound<'_, PyAny>>,
        _traceback: Option<Bound<'_, PyAny>>,
    ) {
        self.inner = None;
    }
}

#[pyclass(name = "Response", module = "autd3", frozen)]
pub struct Response(autd3_rs::Response);

#[pymethods]
impl Response {
    #[getter]
    fn replies(&self) -> Vec<DeviceReply> {
        self.0.replies().iter().cloned().map(DeviceReply).collect()
    }

    fn device_status(&self) -> Vec<DeviceStatus> {
        self.0
            .device_status()
            .into_iter()
            .map(DeviceStatus)
            .collect()
    }

    fn check(&self) -> PyResult<()> {
        self.0.check().map_err(client_error)
    }
}
