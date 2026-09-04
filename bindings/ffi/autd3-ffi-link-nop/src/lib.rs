use std::sync::Arc;

use autd3_ffi_abi::{
    BoxFuture, CheckerBackend, ClientBackend, ClientOpener, LegacyClientOpener, LinkStatusData,
    ResponseTokenData, client_opener, into_handle, legacy_client_opener, link_err,
};
use autd3_rs::Error;
use autd3_rs::legacy::emulator::LegacyAudit;
use autd3_rs::{Client, Frames};
use autd3_rs_core::{ConstStateChecker, StateCheck};
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

    fn send(&self, datagrams: Arc<Frames>, frame: Option<usize>) -> BoxFuture<ResponseTokenData> {
        let client = Arc::clone(&self.client);
        Box::pin(async move {
            let mut futures = Vec::new();
            match frame {
                Some(index) => {
                    let frame = datagrams
                        .frame(index)
                        .ok_or_else(|| link_err(format!("frame {index} out of range")))?;
                    futures.push(client.send(frame).await?);
                }
                None => {
                    for frame in datagrams.iter() {
                        futures.push(client.send(frame).await?);
                    }
                }
            }
            Ok::<ResponseTokenData, Error>(ResponseTokenData::from_futures(futures))
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

    fn checker(&self) -> Box<dyn CheckerBackend> {
        Box::new(NopChecker(Arc::clone(&self.checker)))
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

struct NopChecker(Arc<Mutex<ConstStateChecker>>);

impl CheckerBackend for NopChecker {
    fn check(&self) -> Result<LinkStatusData, Error> {
        let status = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .check()
            .map_err(|e| link_err(e.to_string()))?;
        Ok(LinkStatusData {
            devices: status.devices().to_vec(),
            recoveries: status.recoveries(),
        })
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_link_nop() -> *mut ClientOpener {
    let opener = client_opener(move |geometry, config| async move {
        let (client, checker) =
            Client::open_with_checker(&geometry, autd3_rs_link_nop::Nop::new(), config).await?;
        let backend: Box<dyn ClientBackend> = Box::new(NopBackend {
            client: Arc::new(client),
            checker: Arc::new(Mutex::new(checker)),
        });
        Ok(backend)
    });
    into_handle(opener)
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_link_nop_legacy() -> *mut LegacyClientOpener {
    into_handle(legacy_client_opener(|geometry| {
        Ok(LegacyAudit::new(
            geometry
                .iter()
                .map(autd3_rs_core::geometry::Device::num_transducers),
        ))
    }))
}

autd3_ffi_abi::export_abi_version!();
