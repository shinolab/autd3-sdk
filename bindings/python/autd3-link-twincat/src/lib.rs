use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use autd3_python_capsule::{
    BoxFuture, ClientBackend, LinkStatusData, ResponseToken, client_opener, legacy_client_opener,
    legacy_link_into_capsule, link_err, link_into_capsule,
};
use autd3_rs::Error;
use autd3_rs::{Client, Frames, StateCheck};
use autd3_rs_link_twincat::{
    AmsNetId, Timeouts, TwinCATLinkOption as CoreOption, TwinCATStateChecker,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyCapsule;
use std::sync::Mutex;

struct TwinCATBackend {
    client: Arc<Client>,
    checker: Arc<Mutex<TwinCATStateChecker>>,
}

impl ClientBackend for TwinCATBackend {
    fn num_devices(&self) -> usize {
        self.client.num_devices()
    }

    fn dc_offset_ns(&self) -> i64 {
        self.client.dc_offset_ns()
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

    fn read_telemetry(&self, counter: autd3_rs::Telemetry) -> BoxFuture<Vec<u8>> {
        let client = Arc::clone(&self.client);
        Box::pin(async move { client.read_telemetry(counter).await })
    }

    fn send(&self, datagrams: Arc<Frames>, index: usize) -> BoxFuture<ResponseToken> {
        let client = Arc::clone(&self.client);
        Box::pin(async move {
            let fut = async move {
                let frame = datagrams
                    .frame(index)
                    .ok_or_else(|| link_err(format!("frame {index} out of range")))?;
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
                        .ok_or_else(|| link_err(format!("frame {index} out of range")))?;
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

    fn check_status(&self) -> Result<LinkStatusData, Error> {
        let status = self
            .checker
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .check()
            .map_err(|e| link_err(e.to_string()))?;
        Ok(LinkStatusData {
            device_states: status.devices().iter().map(ToString::to_string).collect(),
            all_op: status.all_op(),
            any_lost: status.any_lost(),
            recoveries: status.recoveries(),
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

#[derive(Clone, Copy)]
enum ServerSpec {
    Local,
    Remote { addr: IpAddr, ams_net_id: AmsNetId },
}

fn parse_remote(addr: &str, ams_net_id: &str) -> PyResult<ServerSpec> {
    let addr = addr
        .parse::<IpAddr>()
        .map_err(|e| PyValueError::new_err(format!("invalid IP address `{addr}`: {e}")))?;
    let ams_net_id = ams_net_id
        .parse::<AmsNetId>()
        .map_err(|e| PyValueError::new_err(format!("invalid AMS Net Id `{ams_net_id}`: {e}")))?;
    Ok(ServerSpec::Remote { addr, ams_net_id })
}

fn opt_duration(obj: Option<&Bound<'_, PyAny>>) -> PyResult<Option<Duration>> {
    match obj {
        None => Ok(None),
        Some(o) => {
            let ns: u128 = o.call_method0("as_nanos")?.extract()?;
            Ok(Some(Duration::from_nanos(
                u64::try_from(ns).unwrap_or(u64::MAX),
            )))
        }
    }
}

fn build_timeouts(
    connect: Option<&Bound<'_, PyAny>>,
    read: Option<&Bound<'_, PyAny>>,
    write: Option<&Bound<'_, PyAny>>,
) -> PyResult<Timeouts> {
    Ok(Timeouts {
        connect: opt_duration(connect)?,
        read: opt_duration(read)?,
        write: opt_duration(write)?,
    })
}

#[pyclass(name = "TwinCATLinkOption", module = "autd3_link_twincat")]
pub struct TwinCATLinkOption {
    server: ServerSpec,
    timeouts: Timeouts,
}

#[pymethods]
impl TwinCATLinkOption {
    #[staticmethod]
    #[pyo3(signature = ())]
    fn local() -> Self {
        Self {
            server: ServerSpec::Local,
            timeouts: Timeouts::none(),
        }
    }

    #[staticmethod]
    #[pyo3(signature = (connect = None, read = None, write = None))]
    fn local_with_timeouts(
        connect: Option<&Bound<'_, PyAny>>,
        read: Option<&Bound<'_, PyAny>>,
        write: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        Ok(Self {
            server: ServerSpec::Local,
            timeouts: build_timeouts(connect, read, write)?,
        })
    }

    #[staticmethod]
    #[pyo3(signature = (addr, ams_net_id))]
    fn remote(addr: &str, ams_net_id: &str) -> PyResult<Self> {
        Ok(Self {
            server: parse_remote(addr, ams_net_id)?,
            timeouts: Timeouts::none(),
        })
    }

    #[staticmethod]
    #[pyo3(signature = (addr, ams_net_id, connect = None, read = None, write = None))]
    fn remote_with_timeouts(
        addr: &str,
        ams_net_id: &str,
        connect: Option<&Bound<'_, PyAny>>,
        read: Option<&Bound<'_, PyAny>>,
        write: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        Ok(Self {
            server: parse_remote(addr, ams_net_id)?,
            timeouts: build_timeouts(connect, read, write)?,
        })
    }

    fn _capsule<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyCapsule>> {
        let server = self.server;
        let timeouts = self.timeouts;
        let opener = client_opener(move |geometry, config| async move {
            let option = match server {
                ServerSpec::Local => CoreOption::local_with_timeouts(timeouts),
                ServerSpec::Remote { addr, ams_net_id } => {
                    CoreOption::remote_with_timeouts(addr, ams_net_id, timeouts)
                }
            };
            let (client, checker) = Client::open_with_checker(&geometry, option, config).await?;
            let backend: Box<dyn ClientBackend> = Box::new(TwinCATBackend {
                client: Arc::new(client),
                checker: Arc::new(Mutex::new(checker)),
            });
            Ok(backend)
        });
        link_into_capsule(py, opener)
    }

    fn _legacy_capsule<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyCapsule>> {
        let server = self.server;
        let timeouts = self.timeouts;
        legacy_link_into_capsule(
            py,
            legacy_client_opener(move |_| {
                Ok(match server {
                    ServerSpec::Local => CoreOption::local_with_timeouts(timeouts),
                    ServerSpec::Remote { addr, ams_net_id } => {
                        CoreOption::remote_with_timeouts(addr, ams_net_id, timeouts)
                    }
                })
            }),
        )
    }
}

#[pymodule]
fn autd3_link_twincat(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<TwinCATLinkOption>()?;
    Ok(())
}
