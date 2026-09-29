use std::net::SocketAddrV6;
use std::sync::Arc;
use std::time::Duration;

use autd3_python_capsule::{
    BoxFuture, ClientBackend, DeviceStatusData, ResponseToken, network_err,
};
use autd3_rs::udp::emulator::UdpEmulator as CoreEmulator;
use autd3_rs::udp::{StateChecker, TransportOption as CoreOption};
use autd3_rs::{Client, ClientConfig, Error, Frames, Geometry, Interface};
use pyo3::exceptions::{PyIndexError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use std::sync::{Mutex, PoisonError};

fn duration(obj: &Bound<'_, PyAny>) -> PyResult<Duration> {
    let ns: u128 = obj.call_method0("as_nanos")?.extract()?;
    Ok(Duration::from_nanos(u64::try_from(ns).unwrap_or(u64::MAX)))
}

pub(crate) fn opt_duration(obj: Option<&Bound<'_, PyAny>>) -> PyResult<Option<Duration>> {
    obj.map(duration).transpose()
}

fn to_py_duration(py: Python<'_>, d: Duration) -> PyResult<Bound<'_, PyAny>> {
    py.import("autd3_core")?.getattr("Duration")?.call_method1(
        "from_nanos",
        (u64::try_from(d.as_nanos()).unwrap_or(u64::MAX),),
    )
}

fn parse_group(group: Option<&str>) -> PyResult<Option<SocketAddrV6>> {
    group
        .map(|addr| {
            addr.parse::<SocketAddrV6>().map_err(|e| {
                PyValueError::new_err(format!("invalid IPv6 socket address `{addr}`: {e}"))
            })
        })
        .transpose()
}

pub(crate) struct UdpBackend {
    client: Arc<Client>,
    checker: Arc<Mutex<StateChecker>>,
}

impl ClientBackend for UdpBackend {
    fn num_devices(&self) -> usize {
        self.client.num_devices()
    }

    fn clock_offset_ns(&self) -> i64 {
        self.client.clock_offset_ns()
    }

    fn read_firmware_version(&self) -> BoxFuture<Vec<String>> {
        let client = Arc::clone(&self.client);
        Box::pin(async move {
            let versions = client.read_firmware_version().await?;
            Ok::<Vec<String>, Error>(versions.into_iter().map(|v| v.to_string()).collect())
        })
    }

    fn read_fpga_state(&self) -> BoxFuture<Vec<u8>> {
        let client = Arc::clone(&self.client);
        Box::pin(async move {
            let states = client.read_fpga_state().await?;
            Ok::<Vec<u8>, Error>(states.into_iter().map(autd3_rs::FpgaState::raw).collect())
        })
    }

    fn read_error_detail(&self) -> BoxFuture<Vec<u8>> {
        let client = Arc::clone(&self.client);
        Box::pin(async move { client.read_error_detail().await })
    }

    fn read_telemetry(&self) -> BoxFuture<Vec<autd3_rs::TelemetryCounters>> {
        let client = Arc::clone(&self.client);
        Box::pin(async move { client.read_telemetry().await })
    }

    fn send(&self, datagrams: Arc<Frames>, index: usize) -> BoxFuture<ResponseToken> {
        let client = Arc::clone(&self.client);
        Box::pin(async move {
            let fut = async move {
                let frame = datagrams
                    .frame(index)
                    .ok_or_else(|| network_err(format!("frame {index} out of range")))?;
                client.send(frame).await
            }
            .await?;
            Ok(ResponseToken::new(fut))
        })
    }

    fn send_checked(&self, datagrams: Arc<Frames>, frame: Option<usize>) -> BoxFuture<()> {
        let client = Arc::clone(&self.client);
        Box::pin(async move {
            match frame {
                Some(index) => {
                    let frame = datagrams
                        .frame(index)
                        .ok_or_else(|| network_err(format!("frame {index} out of range")))?;
                    client.send_checked(frame).await?;
                }
                None => {
                    for frame in datagrams.iter() {
                        client.send_checked(frame).await?;
                    }
                }
            }
            Ok::<(), Error>(())
        })
    }

    fn check_status(&self) -> Result<DeviceStatusData, Error> {
        let status = self
            .checker
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .check()
            .map_err(Error::from)?;
        Ok(DeviceStatusData {
            device_states: status.devices().iter().map(ToString::to_string).collect(),
            all_ready: status.all_ready(),
            any_lost: status.any_lost(),
        })
    }

    fn stop(&self) -> BoxFuture<()> {
        let client = Arc::clone(&self.client);
        Box::pin(async move { client.stop().await })
    }

    fn close(&self) -> BoxFuture<()> {
        let client = Arc::clone(&self.client);
        Box::pin(async move { client.close().await })
    }
}

#[pyclass(name = "TransportOption", module = "autd3")]
pub struct TransportOption {
    pub(crate) inner: CoreOption,
}

#[pymethods]
impl TransportOption {
    #[new]
    #[pyo3(signature = (
        iface = None,
        group = None,
        heartbeat = None,
        reply_timeout = None,
        lost_timeout = None,
        response_timeout = None,
        enumeration_timeout = None,
        sync_timeout = None,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        iface: Option<String>,
        group: Option<&str>,
        heartbeat: Option<&Bound<'_, PyAny>>,
        reply_timeout: Option<&Bound<'_, PyAny>>,
        lost_timeout: Option<&Bound<'_, PyAny>>,
        response_timeout: Option<&Bound<'_, PyAny>>,
        enumeration_timeout: Option<&Bound<'_, PyAny>>,
        sync_timeout: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let mut inner = CoreOption {
            iface: Interface::from(iface),
            group: parse_group(group)?,
            ..CoreOption::default()
        };
        if let Some(v) = opt_duration(heartbeat)? {
            inner.heartbeat = v;
        }
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
        Ok(Self { inner })
    }

    #[getter]
    fn iface(&self) -> Option<String> {
        self.inner.iface.name().map(str::to_owned)
    }

    #[getter]
    fn group(&self) -> Option<String> {
        self.inner.group.map(|addr| addr.to_string())
    }

    #[getter]
    fn heartbeat<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        to_py_duration(py, self.inner.heartbeat)
    }

    #[getter]
    fn reply_timeout<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        to_py_duration(py, self.inner.reply_timeout)
    }

    #[getter]
    fn lost_timeout<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        to_py_duration(py, self.inner.lost_timeout)
    }

    #[getter]
    fn response_timeout<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        to_py_duration(py, self.inner.response_timeout)
    }

    #[getter]
    fn enumeration_timeout<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        to_py_duration(py, self.inner.enumeration_timeout)
    }

    #[getter]
    fn sync_timeout<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        to_py_duration(py, self.inner.sync_timeout)
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
pub struct UdpEmulator {
    inner: std::sync::Arc<Mutex<Option<CoreEmulator>>>,
}

impl UdpEmulator {
    fn with<R>(&self, f: impl FnOnce(&CoreEmulator) -> R) -> PyResult<R> {
        self.inner
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
        let emulator =
            CoreEmulator::spawn(num_devices).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        let inner = std::sync::Arc::new(Mutex::new(Some(emulator)));
        let mut registry = EMULATORS.lock().unwrap_or_else(PoisonError::into_inner);
        registry.retain(|weak| weak.strong_count() > 0);
        registry.push(std::sync::Arc::downgrade(&inner));
        Ok(Self { inner })
    }

    #[getter]
    fn num_devices(&self) -> PyResult<usize> {
        self.with(CoreEmulator::num_devices)
    }

    fn option(&self) -> PyResult<TransportOption> {
        self.with(|emulator| TransportOption {
            inner: emulator.option(),
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

pub(crate) async fn open(
    geometry: Geometry,
    option: CoreOption,
    config: ClientConfig,
) -> Result<Box<dyn ClientBackend>, Error> {
    let (client, checker) = Client::open_with_checker(&geometry, option, config).await?;
    Ok(Box::new(UdpBackend {
        client: Arc::new(client),
        checker: Arc::new(Mutex::new(checker)),
    }))
}
