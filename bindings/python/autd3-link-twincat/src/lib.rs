use std::net::IpAddr;
use std::sync::{Arc, OnceLock};

use autd3_python_capsule::{
    BoxFuture, ClientBackend, LinkStatusData, client_opener, link_into_capsule,
};
use autd3_rs::{Client, Datagrams, StateCheck};
use autd3_rs_core::Error;
use autd3_rs_link_twincat::{
    AmsNetId, TwinCATLinkOption as CoreOption, TwinCATRoute as CoreRoute, TwinCATStateChecker,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyCapsule;
use tokio::sync::Mutex;

fn link_runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to build twincat tokio runtime")
    })
}

fn join_err(e: tokio::task::JoinError) -> Error {
    Error::Link(e.to_string())
}

struct TwinCATBackend {
    client: Arc<Client>,
    checker: Arc<Mutex<TwinCATStateChecker>>,
}

impl ClientBackend for TwinCATBackend {
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

    fn send_checked(&self, datagrams: Arc<Datagrams>, frame: Option<usize>) -> BoxFuture<()> {
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

#[pyclass(name = "TwinCATRoute", module = "autd3_link_twincat", from_py_object)]
#[derive(Clone, Copy)]
pub struct TwinCATRoute(pub(crate) CoreRoute);

#[pymethods]
impl TwinCATRoute {
    #[classattr]
    #[pyo3(name = "Auto")]
    fn auto() -> Self {
        Self(CoreRoute::Auto)
    }

    #[classattr]
    #[pyo3(name = "Notify")]
    fn notify() -> Self {
        Self(CoreRoute::Notify)
    }

    #[classattr]
    #[pyo3(name = "Ads")]
    fn ads() -> Self {
        Self(CoreRoute::Ads)
    }
}

#[derive(Clone, Copy)]
enum ServerSpec {
    Local,
    Remote { addr: IpAddr, ams_net_id: AmsNetId },
}

#[pyclass(name = "TwinCATLinkOption", module = "autd3_link_twincat")]
pub struct TwinCATLinkOption {
    server: ServerSpec,
    route: CoreRoute,
}

#[pymethods]
impl TwinCATLinkOption {
    #[staticmethod]
    #[pyo3(signature = (route = None))]
    fn local(route: Option<TwinCATRoute>) -> Self {
        Self {
            server: ServerSpec::Local,
            route: route.map_or(CoreRoute::Auto, |r| r.0),
        }
    }

    #[staticmethod]
    #[pyo3(signature = (addr, ams_net_id, route = None))]
    fn remote(addr: &str, ams_net_id: &str, route: Option<TwinCATRoute>) -> PyResult<Self> {
        let addr = addr
            .parse::<IpAddr>()
            .map_err(|e| PyValueError::new_err(format!("invalid IP address `{addr}`: {e}")))?;
        let ams_net_id = ams_net_id.parse::<AmsNetId>().map_err(|e| {
            PyValueError::new_err(format!("invalid AMS Net Id `{ams_net_id}`: {e}"))
        })?;
        Ok(Self {
            server: ServerSpec::Remote { addr, ams_net_id },
            route: route.map_or(CoreRoute::Auto, |r| r.0),
        })
    }

    fn _capsule<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyCapsule>> {
        let server = self.server;
        let route = self.route;
        let opener = client_opener(move |geometry, config| async move {
            let option = match server {
                ServerSpec::Local => CoreOption::local(),
                ServerSpec::Remote { addr, ams_net_id } => CoreOption::remote(addr, ams_net_id),
            }
            .with_route(route);
            let (client, checker) = link_runtime()
                .spawn(async move { Client::open_with_checker(&geometry, option, config).await })
                .await
                .map_err(join_err)??;
            let backend: Box<dyn ClientBackend> = Box::new(TwinCATBackend {
                client: Arc::new(client),
                checker: Arc::new(Mutex::new(checker)),
            });
            Ok(backend)
        });
        link_into_capsule(py, opener)
    }
}

#[pymodule]
fn autd3_link_twincat(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<TwinCATRoute>()?;
    m.add_class::<TwinCATLinkOption>()?;
    Ok(())
}
