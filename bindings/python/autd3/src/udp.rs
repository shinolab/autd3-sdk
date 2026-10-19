use std::num::NonZeroUsize;
use std::time::Duration;

use autd3_python_capsule::extract::{duration_to_py, extract_duration};
use autd3_rs::udp::UdpOption as CoreOption;
use autd3_rs::{Interface as CoreInterface, Link, ServerLink as CoreServerLink};
use autd3_rs_firmware_emulator::udp::UdpEmulator as CoreEmulator;
use pyo3::exceptions::{PyIndexError, PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::sync::MutexExt;
use std::sync::{Mutex, PoisonError};

pub(crate) fn none_or_duration(obj: &Bound<'_, PyAny>) -> PyResult<Option<Duration>> {
    if obj.is_none() {
        return Ok(None);
    }
    extract_duration(obj).map(Some)
}

#[pyclass(name = "Interface", module = "autd3", eq, hash, frozen, from_py_object)]
#[derive(Clone, PartialEq, Eq)]
pub struct Interface(pub(crate) CoreInterface);

impl core::hash::Hash for Interface {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        format!("{:?}", self.0).hash(state);
    }
}

#[pymethods]
impl Interface {
    #[classattr]
    #[pyo3(name = "Auto")]
    fn auto() -> Self {
        Self(CoreInterface::Auto)
    }

    #[staticmethod]
    #[pyo3(name = "Simulator")]
    fn simulator(port: u16) -> Self {
        Self(CoreInterface::Simulator(port))
    }

    #[staticmethod]
    #[pyo3(name = "Name")]
    fn name(name: String) -> Self {
        Self(CoreInterface::Name(name))
    }

    #[staticmethod]
    #[pyo3(name = "Addr")]
    fn addr(addr: &str) -> PyResult<Self> {
        addr.parse()
            .map(|addr| Self(CoreInterface::Addr(addr)))
            .map_err(|_| {
                PyValueError::new_err(format!(
                    "{addr:?} is not an IPv6 socket address such as \"[::1]:44336\""
                ))
            })
    }

    #[pyo3(name = "name")]
    fn interface_name(&self) -> Option<&str> {
        self.0.name()
    }

    fn simulator_port(&self) -> Option<u16> {
        match self.0 {
            CoreInterface::Simulator(port) => Some(port),
            _ => None,
        }
    }

    fn __repr__(&self) -> String {
        match &self.0 {
            CoreInterface::Name(name) => format!("Interface.Name({name:?})"),
            CoreInterface::Simulator(port) => format!("Interface.Simulator({port})"),
            CoreInterface::Addr(addr) => format!("Interface.Addr({:?})", addr.to_string()),
            _ => "Interface.Auto".to_owned(),
        }
    }
}

fn extract_iface(obj: &Bound<'_, PyAny>) -> PyResult<CoreInterface> {
    if let Ok(iface) = obj.extract::<Interface>() {
        return Ok(iface.0);
    }
    if let Ok(name) = obj.extract::<String>() {
        return Ok(CoreInterface::Name(name));
    }
    Err(PyTypeError::new_err(
        "iface must be an Interface or an interface name",
    ))
}

#[pyclass(name = "UdpOption", module = "autd3", eq)]
#[derive(PartialEq)]
pub struct UdpOption(pub(crate) CoreOption);

#[pymethods]
impl UdpOption {
    #[new]
    #[pyo3(signature = (
        iface = CoreOption::default().iface,
        heartbeat = CoreOption::default().heartbeat,
        reply_timeout = CoreOption::default().reply_timeout,
        lost_timeout = CoreOption::default().lost_timeout,
        response_timeout = CoreOption::default().response_timeout,
        enumeration_timeout = CoreOption::default().enumeration_timeout,
        sync_timeout = CoreOption::default().sync_timeout,
        send_rate_limit = CoreOption::default().send_rate_limit,
        send_buffer = CoreOption::default().send_buffer,
        timer_resolution = CoreOption::default().timer_resolution,
        recovery_interval = CoreOption::default().recovery_interval,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        #[pyo3(from_py_with = extract_iface)] iface: CoreInterface,
        #[pyo3(from_py_with = none_or_duration)] heartbeat: Option<Duration>,
        #[pyo3(from_py_with = extract_duration)] reply_timeout: Duration,
        #[pyo3(from_py_with = extract_duration)] lost_timeout: Duration,
        #[pyo3(from_py_with = extract_duration)] response_timeout: Duration,
        #[pyo3(from_py_with = extract_duration)] enumeration_timeout: Duration,
        #[pyo3(from_py_with = extract_duration)] sync_timeout: Duration,
        send_rate_limit: Option<f32>,
        send_buffer: Option<NonZeroUsize>,
        #[pyo3(from_py_with = none_or_duration)] timer_resolution: Option<Duration>,
        #[pyo3(from_py_with = none_or_duration)] recovery_interval: Option<Duration>,
    ) -> Self {
        Self(CoreOption {
            iface,
            heartbeat,
            reply_timeout,
            lost_timeout,
            recovery_interval,
            response_timeout,
            enumeration_timeout,
            sync_timeout,
            send_rate_limit,
            send_buffer,
            timer_resolution,
        })
    }

    #[getter]
    fn iface(&self) -> Interface {
        Interface(self.0.iface.clone())
    }

    #[getter]
    fn heartbeat<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
        self.0
            .heartbeat
            .map(|interval| duration_to_py(py, interval))
            .transpose()
    }

    #[getter]
    fn reply_timeout<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        duration_to_py(py, self.0.reply_timeout)
    }

    #[getter]
    fn lost_timeout<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        duration_to_py(py, self.0.lost_timeout)
    }

    #[getter]
    fn response_timeout<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        duration_to_py(py, self.0.response_timeout)
    }

    #[getter]
    fn enumeration_timeout<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        duration_to_py(py, self.0.enumeration_timeout)
    }

    #[getter]
    fn sync_timeout<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        duration_to_py(py, self.0.sync_timeout)
    }

    #[getter]
    fn send_rate_limit(&self) -> Option<f32> {
        self.0.send_rate_limit
    }

    #[getter]
    fn recovery_interval<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
        self.0
            .recovery_interval
            .map(|interval| duration_to_py(py, interval))
            .transpose()
    }

    #[getter]
    fn send_buffer(&self) -> Option<NonZeroUsize> {
        self.0.send_buffer
    }

    #[getter]
    fn timer_resolution<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
        self.0
            .timer_resolution
            .map(|resolution| duration_to_py(py, resolution))
            .transpose()
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
}

#[pyclass(name = "ServerLink", module = "autd3", eq, frozen, from_py_object)]
#[derive(Clone, PartialEq)]
pub struct ServerLink(pub(crate) CoreServerLink);

#[pymethods]
impl ServerLink {
    #[new]
    #[pyo3(signature = (port = CoreServerLink::default().port, timeout = CoreServerLink::default().timeout))]
    fn new(port: u16, #[pyo3(from_py_with = extract_duration)] timeout: Duration) -> Self {
        Self(CoreServerLink { port, timeout })
    }

    #[getter]
    fn port(&self) -> u16 {
        self.0.port
    }

    #[getter]
    fn timeout<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        duration_to_py(py, self.0.timeout)
    }

    fn __repr__(&self) -> String {
        format!(
            "ServerLink(port={}, timeout=Duration({:?}))",
            self.0.port, self.0.timeout
        )
    }
}

pub(crate) fn extract_link(obj: &Bound<'_, PyAny>) -> PyResult<Link> {
    if let Ok(option) = obj.cast::<UdpOption>() {
        return Ok(Link::Udp(option.borrow().0.clone()));
    }
    if let Ok(link) = obj.extract::<ServerLink>() {
        return Ok(Link::Server(link.0));
    }
    Err(PyTypeError::new_err(
        "link must be a UdpOption or a ServerLink",
    ))
}

static EMULATORS: Mutex<Vec<std::sync::Weak<Mutex<Option<CoreEmulator>>>>> = Mutex::new(Vec::new());

pub(crate) fn shutdown_emulators() {
    let emulators = std::mem::take(&mut *EMULATORS.lock().unwrap_or_else(PoisonError::into_inner));
    for emulator in emulators.iter().filter_map(std::sync::Weak::upgrade) {
        drop(
            emulator
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take(),
        );
    }
}

#[pyclass(name = "UdpEmulator", module = "autd3")]
pub struct UdpEmulator(std::sync::Arc<Mutex<Option<CoreEmulator>>>);

impl UdpEmulator {
    fn with<R>(&self, py: Python<'_>, f: impl FnOnce(&CoreEmulator) -> R) -> PyResult<R> {
        self.0
            .lock_py_attached(py)
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(f)
            .ok_or_else(|| PyRuntimeError::new_err("the UDP emulator has been shut down"))
    }
}

#[pymethods]
impl UdpEmulator {
    #[new]
    fn new(py: Python<'_>, num_devices: usize) -> PyResult<Self> {
        let emulator = CoreEmulator::spawn(num_devices).map_err(|e| {
            autd3_python_capsule::to_pyerr_with_code(
                py,
                autd3_python_capsule::error_code::NETWORK,
                e.to_string(),
            )
        })?;
        let inner = std::sync::Arc::new(Mutex::new(Some(emulator)));
        let mut registry = EMULATORS
            .lock_py_attached(py)
            .unwrap_or_else(PoisonError::into_inner);
        registry.retain(|weak| weak.strong_count() > 0);
        registry.push(std::sync::Arc::downgrade(&inner));
        Ok(Self(inner))
    }

    fn num_devices(&self, py: Python<'_>) -> PyResult<usize> {
        self.with(py, CoreEmulator::num_devices)
    }

    fn option(&self, py: Python<'_>) -> PyResult<UdpOption> {
        self.with(py, |emulator| {
            UdpOption(CoreOption::for_emulator(emulator.interface()))
        })
    }

    fn reboot(&self, py: Python<'_>, index: usize) -> PyResult<()> {
        let num_devices = self.with(py, CoreEmulator::num_devices)?;
        if index >= num_devices {
            return Err(PyIndexError::new_err(format!(
                "device index {index} is out of range for {num_devices} devices"
            )));
        }
        self.with(py, |emulator| emulator.reboot(index))
    }
}
