use std::sync::{Arc, OnceLock};
use std::time::Duration;

use autd3_python_capsule::{
    BoxFuture, ClientBackend, LinkStatusData, ResponseToken, client_opener, link_into_capsule,
};
use autd3_rs::{Client, Frames};
use autd3_rs_core::{Error, Interface};
use autd3_rs_link_ethercrab::{EtherCrabLinkOption as CoreOption, StateChecker};
use pyo3::prelude::*;
use pyo3::types::PyCapsule;
use tokio::sync::Mutex;

fn link_runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to build ethercrab tokio runtime")
    })
}

fn join_err(e: tokio::task::JoinError) -> Error {
    Error::Link(e.to_string())
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

struct EtherCrabBackend {
    client: Arc<Client>,
    checker: Arc<Mutex<StateChecker>>,
}

impl ClientBackend for EtherCrabBackend {
    fn num_devices(&self) -> usize {
        self.client.num_devices()
    }

    fn read_firmware_version(&self) -> BoxFuture<Vec<String>> {
        let client = Arc::clone(&self.client);
        Box::pin(async move {
            link_runtime()
                .spawn(async move {
                    let versions = client.read_firmware_version().await?;
                    Ok::<Vec<String>, Error>(versions.into_iter().map(|v| v.to_string()).collect())
                })
                .await
                .map_err(join_err)?
        })
    }

    fn read_fpga_state(&self) -> BoxFuture<Vec<u8>> {
        let client = Arc::clone(&self.client);
        Box::pin(async move {
            link_runtime()
                .spawn(async move {
                    let states = client.read_fpga_state().await?;
                    Ok::<Vec<u8>, Error>(states.into_iter().map(autd3_rs::FpgaState::raw).collect())
                })
                .await
                .map_err(join_err)?
        })
    }

    fn read_error_detail(&self) -> BoxFuture<Vec<u8>> {
        let client = Arc::clone(&self.client);
        Box::pin(async move {
            link_runtime()
                .spawn(async move { client.read_error_detail().await })
                .await
                .map_err(join_err)?
        })
    }

    fn send(&self, datagrams: Arc<Frames>, index: usize) -> BoxFuture<ResponseToken> {
        let client = Arc::clone(&self.client);
        Box::pin(async move {
            let fut = link_runtime()
                .spawn(async move {
                    let frame = datagrams
                        .frame(index)
                        .ok_or_else(|| Error::Link(format!("frame {index} out of range")))?;
                    client.send(frame).await
                })
                .await
                .map_err(join_err)??;
            Ok(ResponseToken::new(fut, link_runtime().handle().clone()))
        })
    }

    fn send_checked(&self, datagrams: Arc<Frames>, frame: Option<usize>) -> BoxFuture<()> {
        let client = Arc::clone(&self.client);
        Box::pin(async move {
            link_runtime()
                .spawn(async move {
                    match frame {
                        Some(index) => {
                            let frame = datagrams.frame(index).ok_or_else(|| {
                                Error::Link(format!("frame {index} out of range"))
                            })?;
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
                .await
                .map_err(join_err)?
        })
    }

    fn check_status(&self) -> BoxFuture<LinkStatusData> {
        let checker = Arc::clone(&self.checker);
        Box::pin(async move {
            link_runtime()
                .spawn(async move {
                    let status = checker
                        .lock()
                        .await
                        .check()
                        .await
                        .map_err(|e| Error::Link(e.to_string()))?;
                    Ok::<LinkStatusData, Error>(LinkStatusData {
                        device_states: status.devices.iter().map(ToString::to_string).collect(),
                        all_op: status.all_op(),
                        any_lost: status.any_lost(),
                        recoveries: status.recoveries,
                    })
                })
                .await
                .map_err(join_err)?
        })
    }

    fn stop(&self) -> BoxFuture<()> {
        let client = Arc::clone(&self.client);
        Box::pin(async move {
            link_runtime()
                .spawn(async move { client.stop().await })
                .await
                .map_err(join_err)?
        })
    }

    fn close(&self) -> BoxFuture<()> {
        let client = Arc::clone(&self.client);
        Box::pin(async move {
            link_runtime()
                .spawn(async move { client.close().await })
                .await
                .map_err(join_err)?
        })
    }
}

#[pyclass(name = "EtherCrabLinkOption", module = "autd3_link_ethercrab")]
pub struct EtherCrabLinkOption {
    inner: CoreOption,
}

#[pymethods]
impl EtherCrabLinkOption {
    #[new]
    #[pyo3(signature = (
        interface = None,
        sync0_period = None,
        sync0_shift = None,
        sync_tolerance = None,
        sync_timeout = None,
    ))]
    fn new(
        interface: Option<String>,
        sync0_period: Option<&Bound<'_, PyAny>>,
        sync0_shift: Option<&Bound<'_, PyAny>>,
        sync_tolerance: Option<&Bound<'_, PyAny>>,
        sync_timeout: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let mut inner = CoreOption {
            interface: Interface::from(interface),
            ..CoreOption::default()
        };
        if let Some(v) = opt_duration(sync0_period)? {
            inner.sync0_period = v;
        }
        if let Some(v) = opt_duration(sync0_shift)? {
            inner.sync0_shift = v;
        }
        if let Some(v) = opt_duration(sync_tolerance)? {
            inner.sync_tolerance = v;
        }
        if let Some(v) = opt_duration(sync_timeout)? {
            inner.sync_timeout = v;
        }
        Ok(Self { inner })
    }

    #[staticmethod]
    fn safe_default() -> Self {
        Self {
            inner: CoreOption::safe_default(),
        }
    }

    #[staticmethod]
    fn performance_default() -> Self {
        Self {
            inner: CoreOption::performance_default(),
        }
    }

    fn _capsule<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyCapsule>> {
        let option = self.inner.clone();
        let opener = client_opener(move |geometry, config| async move {
            let (client, checker) = link_runtime()
                .spawn(async move { Client::open_with_checker(&geometry, option, config).await })
                .await
                .map_err(join_err)??;
            let backend: Box<dyn ClientBackend> = Box::new(EtherCrabBackend {
                client: Arc::new(client),
                checker: Arc::new(Mutex::new(checker)),
            });
            Ok(backend)
        });
        link_into_capsule(py, opener)
    }
}

#[pymodule]
fn autd3_link_ethercrab(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<EtherCrabLinkOption>()?;
    Ok(())
}
