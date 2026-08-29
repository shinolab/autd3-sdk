use std::sync::Arc;
use std::time::Duration;

use autd3_python_capsule::{
    BoxFuture, ClientBackend, LinkStatusData, ResponseToken, client_opener, join_err,
    legacy_client_opener, legacy_link_into_capsule, link_into_capsule, link_runtime,
};
use autd3_rs::Error;
use autd3_rs::{Client, Frames};
use autd3_rs_core::Interface;
use autd3_rs_link_echocat::{
    EchocatLinkOption as CoreOption, FramePhase as CoreFramePhase, SleepStrategy, StateChecker,
};
use pyo3::prelude::*;
use pyo3::types::PyCapsule;
use tokio::sync::Mutex;

fn duration(obj: &Bound<'_, PyAny>) -> PyResult<Duration> {
    let ns: u128 = obj.call_method0("as_nanos")?.extract()?;
    Ok(Duration::from_nanos(u64::try_from(ns).unwrap_or(u64::MAX)))
}

fn opt_duration(obj: Option<&Bound<'_, PyAny>>) -> PyResult<Option<Duration>> {
    obj.map(duration).transpose()
}

#[pyclass(name = "FramePhase", module = "autd3_link_echocat", from_py_object)]
#[derive(Clone, Copy)]
pub struct FramePhase(CoreFramePhase);

#[pymethods]
impl FramePhase {
    #[classattr]
    #[pyo3(name = "Auto")]
    fn auto() -> Self {
        Self(CoreFramePhase::Auto)
    }

    #[staticmethod]
    #[pyo3(name = "At")]
    fn at(phase: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self(CoreFramePhase::At(duration(phase)?)))
    }

    fn __repr__(&self) -> String {
        format!("FramePhase.{:?}", self.0)
    }
}

struct EchocatBackend {
    client: Arc<Client>,
    checker: Arc<Mutex<StateChecker>>,
}

impl ClientBackend for EchocatBackend {
    fn num_devices(&self) -> usize {
        self.client.num_devices()
    }

    fn dc_offset_ns(&self) -> i64 {
        self.client.dc_offset_ns()
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

    fn read_telemetry(&self, counter: autd3_rs::Telemetry) -> BoxFuture<Vec<u8>> {
        let client = Arc::clone(&self.client);
        Box::pin(async move {
            link_runtime()
                .spawn(async move { client.read_telemetry(counter).await })
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
                        device_states: status.devices().iter().map(ToString::to_string).collect(),
                        all_op: status.all_op(),
                        any_lost: status.any_lost(),
                        recoveries: status.recoveries(),
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

#[pyclass(name = "EchocatLinkOption", module = "autd3_link_echocat")]
pub struct EchocatLinkOption {
    inner: CoreOption,
}

#[pymethods]
impl EchocatLinkOption {
    #[new]
    #[pyo3(signature = (
        iface = None,
        sync0_period = None,
        frame_phase = None,
        pdu_timeout = None,
        state_transition_timeout = None,
        dc_static_sync_iterations = None,
        dc_start_delay = None,
        sync_tolerance = None,
        sync_timeout = None,
        process_data_watchdog = None,
        spin_margin = None,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        iface: Option<String>,
        sync0_period: Option<&Bound<'_, PyAny>>,
        frame_phase: Option<FramePhase>,
        pdu_timeout: Option<&Bound<'_, PyAny>>,
        state_transition_timeout: Option<&Bound<'_, PyAny>>,
        dc_static_sync_iterations: Option<u32>,
        dc_start_delay: Option<&Bound<'_, PyAny>>,
        sync_tolerance: Option<&Bound<'_, PyAny>>,
        sync_timeout: Option<&Bound<'_, PyAny>>,
        process_data_watchdog: Option<&Bound<'_, PyAny>>,
        spin_margin: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let mut inner = CoreOption {
            iface: Interface::from(iface),
            ..CoreOption::default()
        };
        if let Some(v) = opt_duration(sync0_period)? {
            inner.sync0_period = v;
        }
        if let Some(v) = frame_phase {
            inner.frame_phase = v.0;
        }
        if let Some(v) = opt_duration(pdu_timeout)? {
            inner.pdu_timeout = v;
        }
        if let Some(v) = opt_duration(state_transition_timeout)? {
            inner.state_transition_timeout = v;
        }
        if let Some(v) = dc_static_sync_iterations {
            inner.dc_static_sync_iterations = v;
        }
        if let Some(v) = opt_duration(dc_start_delay)? {
            inner.dc_start_delay = v;
        }
        if let Some(v) = opt_duration(sync_tolerance)? {
            inner.sync_tolerance = v;
        }
        if let Some(v) = opt_duration(sync_timeout)? {
            inner.sync_timeout = v;
        }
        if let Some(v) = opt_duration(process_data_watchdog)? {
            inner.process_data_watchdog = v;
        }
        if let Some(v) = opt_duration(spin_margin)? {
            inner.sleep_strategy = SleepStrategy::Spin { margin: v };
        }
        Ok(Self { inner })
    }

    fn _capsule<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyCapsule>> {
        let option = self.inner.clone();
        let opener = client_opener(move |geometry, config| async move {
            let (client, checker) = link_runtime()
                .spawn(async move { Client::open_with_checker(&geometry, option, config).await })
                .await
                .map_err(join_err)??;
            let backend: Box<dyn ClientBackend> = Box::new(EchocatBackend {
                client: Arc::new(client),
                checker: Arc::new(Mutex::new(checker)),
            });
            Ok(backend)
        });
        link_into_capsule(py, opener)
    }

    fn _legacy_capsule<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyCapsule>> {
        let option = self.inner.clone();
        legacy_link_into_capsule(py, legacy_client_opener(move |_| Ok(option)))
    }
}

#[pymodule]
fn autd3_link_echocat(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<EchocatLinkOption>()?;
    m.add_class::<FramePhase>()?;
    Ok(())
}
