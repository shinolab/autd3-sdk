use std::num::NonZeroUsize;
use std::time::Duration;

use autd3_python_capsule::extract::{duration_to_py, extract_duration};
use autd3_rs::Interface as CoreInterface;
use autd3_rs::udp::TransportOption as CoreOption;
use autd3_rs_firmware_emulator::udp::UdpEmulator as CoreEmulator;
use pyo3::exceptions::{PyIndexError, PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use std::sync::{Mutex, PoisonError};

pub(crate) fn opt_duration(obj: Option<&Bound<'_, PyAny>>) -> PyResult<Option<Duration>> {
    obj.map(extract_duration).transpose()
}

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

    #[classattr]
    #[pyo3(name = "Simulator")]
    fn simulator() -> Self {
        Self(CoreInterface::Simulator)
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

    fn __repr__(&self) -> String {
        match &self.0 {
            CoreInterface::Name(name) => format!("Interface.Name({name:?})"),
            CoreInterface::Simulator => "Interface.Simulator".to_owned(),
            CoreInterface::Addr(addr) => format!("Interface.Addr({:?})", addr.to_string()),
            _ => "Interface.Auto".to_owned(),
        }
    }
}

fn extract_iface(obj: &Bound<'_, PyAny>) -> PyResult<CoreInterface> {
    if obj.is_none() {
        return Ok(CoreInterface::Auto);
    }
    if let Ok(iface) = obj.extract::<Interface>() {
        return Ok(iface.0);
    }
    if let Ok(name) = obj.extract::<String>() {
        return Ok(CoreInterface::Name(name));
    }
    Err(PyTypeError::new_err(
        "iface must be an Interface, an interface name or None",
    ))
}

#[pyclass(name = "TransportOption", module = "autd3")]
pub struct TransportOption(pub(crate) CoreOption);

#[pymethods]
impl TransportOption {
    #[new]
    #[pyo3(signature = (
        iface = CoreInterface::Auto,
        heartbeat = CoreOption::default().heartbeat,
        reply_timeout = None,
        lost_timeout = None,
        response_timeout = None,
        enumeration_timeout = None,
        sync_timeout = None,
        send_rate_limit = None,
        send_buffer = CoreOption::default().send_buffer,
        timer_resolution = CoreOption::default().timer_resolution,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        #[pyo3(from_py_with = extract_iface)] iface: CoreInterface,
        #[pyo3(from_py_with = none_or_duration)] heartbeat: Option<Duration>,
        reply_timeout: Option<&Bound<'_, PyAny>>,
        lost_timeout: Option<&Bound<'_, PyAny>>,
        response_timeout: Option<&Bound<'_, PyAny>>,
        enumeration_timeout: Option<&Bound<'_, PyAny>>,
        sync_timeout: Option<&Bound<'_, PyAny>>,
        send_rate_limit: Option<f32>,
        send_buffer: Option<NonZeroUsize>,
        #[pyo3(from_py_with = none_or_duration)] timer_resolution: Option<Duration>,
    ) -> PyResult<Self> {
        let mut inner = CoreOption {
            iface,
            heartbeat,
            send_rate_limit,
            send_buffer,
            timer_resolution,
            ..CoreOption::default()
        };
        if let Some(v) = opt_duration(reply_timeout)? {
            inner.reply_timeout = v;
        }
        if let Some(v) = opt_duration(lost_timeout)? {
            inner.lost_timeout = v;
        }
        if let Some(v) = opt_duration(response_timeout)? {
            inner.response_timeout = v;
        }
        if let Some(v) = opt_duration(enumeration_timeout)? {
            inner.enumeration_timeout = v;
        }
        if let Some(v) = opt_duration(sync_timeout)? {
            inner.sync_timeout = v;
        }
        Ok(Self(inner))
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
    fn with<R>(&self, f: impl FnOnce(&CoreEmulator) -> R) -> PyResult<R> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(f)
            .ok_or_else(|| PyRuntimeError::new_err("the UDP emulator has been shut down"))
    }
}

#[pymethods]
impl UdpEmulator {
    #[new]
    fn new(num_devices: usize) -> PyResult<Self> {
        let emulator = CoreEmulator::spawn(num_devices).map_err(|e| {
            Python::attach(|py| {
                autd3_python_capsule::to_pyerr_with_code(
                    py,
                    autd3_python_capsule::error_code::NETWORK,
                    e.to_string(),
                )
            })
        })?;
        let inner = std::sync::Arc::new(Mutex::new(Some(emulator)));
        let mut registry = EMULATORS.lock().unwrap_or_else(PoisonError::into_inner);
        registry.retain(|weak| weak.strong_count() > 0);
        registry.push(std::sync::Arc::downgrade(&inner));
        Ok(Self(inner))
    }

    #[getter]
    fn num_devices(&self) -> PyResult<usize> {
        self.with(CoreEmulator::num_devices)
    }

    fn option(&self) -> PyResult<TransportOption> {
        self.with(|emulator| {
            TransportOption(CoreOption {
                iface: emulator.interface(),
                reply_timeout: Duration::from_millis(50),
                response_timeout: Duration::from_millis(50),
                enumeration_timeout: Duration::from_secs(1),
                sync_timeout: Duration::from_secs(5),
                ..CoreOption::default()
            })
        })
    }

    fn reboot(&self, index: usize) -> PyResult<()> {
        let num_devices = self.with(CoreEmulator::num_devices)?;
        if index >= num_devices {
            return Err(PyIndexError::new_err(format!(
                "device index {index} is out of range for {num_devices} devices"
            )));
        }
        self.with(|emulator| emulator.reboot(index))
    }
}
