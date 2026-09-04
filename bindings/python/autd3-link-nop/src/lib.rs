use std::sync::Arc;

use autd3_python_capsule::{
    BoxFuture, ClientBackend, LinkStatusData, ResponseToken, client_opener, legacy_client_opener,
    legacy_link_into_capsule, link_err, link_into_capsule,
};
use autd3_rs::Error;
use autd3_rs::{Client, ConstStateChecker, Frames, StateCheck};
use autd3_rs_link_nop::Nop as CoreNop;
use pyo3::prelude::*;
use pyo3::types::PyCapsule;
use std::sync::Mutex;

struct NopBackend {
    client: Arc<Client>,
    checker: Arc<Mutex<ConstStateChecker>>,
}

impl ClientBackend for NopBackend {
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

#[pyclass(name = "Nop", module = "autd3_link_nop")]
pub struct Nop;

#[pymethods]
impl Nop {
    #[new]
    fn new() -> Self {
        Self
    }

    #[allow(clippy::unused_self)]
    fn _capsule<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyCapsule>> {
        let opener = client_opener(move |geometry, config| async move {
            let (client, checker) =
                Client::open_with_checker(&geometry, CoreNop::new(), config).await?;
            let backend: Box<dyn ClientBackend> = Box::new(NopBackend {
                client: Arc::new(client),
                checker: Arc::new(Mutex::new(checker)),
            });
            Ok(backend)
        });
        link_into_capsule(py, opener)
    }

    #[allow(clippy::unused_self)]
    fn _legacy_capsule<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyCapsule>> {
        legacy_link_into_capsule(
            py,
            legacy_client_opener(|geometry| {
                Ok(autd3_rs::legacy::emulator::LegacyAudit::new(
                    geometry
                        .iter()
                        .map(autd3_rs::geometry::Device::num_transducers),
                ))
            }),
        )
    }
}

#[pymodule]
fn autd3_link_nop(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Nop>()?;
    Ok(())
}
