use std::ffi::c_char;
use std::sync::Arc;

use autd3_ffi_abi::{
    BoxFuture, CheckerBackend, ClientBackend, ClientOpener, LegacyClientOpener, LinkStatusData,
    OPTION_HANDLE_CONSUMED, ResponseTokenData, client_opener, into_handle, join_err,
    legacy_client_opener, link_runtime, take_handle, write_cstr,
};
use autd3_rs::Error;
use autd3_rs::{Client, Frames};
use autd3_rs_link_soem::{SoemLinkOption as CoreOption, StateChecker};
use tokio::sync::Mutex;

struct SoemBackend {
    client: Arc<Client>,
    checker: Arc<Mutex<StateChecker>>,
}

impl ClientBackend for SoemBackend {
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

    fn send(&self, datagrams: Arc<Frames>, frame: Option<usize>) -> BoxFuture<ResponseTokenData> {
        let client = Arc::clone(&self.client);
        Box::pin(async move {
            link_runtime()
                .spawn(async move {
                    let mut futures = Vec::new();
                    match frame {
                        Some(index) => {
                            let frame = datagrams.frame(index).ok_or_else(|| {
                                Error::Link(format!("frame {index} out of range"))
                            })?;
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
                .await
                .map_err(join_err)?
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

    fn checker(&self) -> Box<dyn CheckerBackend> {
        Box::new(SoemChecker(Arc::clone(&self.checker)))
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

struct SoemChecker(Arc<Mutex<StateChecker>>);

impl CheckerBackend for SoemChecker {
    fn check(&self) -> BoxFuture<LinkStatusData> {
        let checker = Arc::clone(&self.0);
        Box::pin(async move {
            link_runtime()
                .spawn(async move {
                    let status = checker
                        .lock()
                        .await
                        .check()
                        .map_err(|e| Error::Link(e.to_string()))?;
                    Ok::<LinkStatusData, Error>(LinkStatusData {
                        devices: status.devices().to_vec(),
                        recoveries: status.recoveries(),
                    })
                })
                .await
                .map_err(join_err)?
        })
    }
}

pub struct SoemLinkOptionHandle(CoreOption);

#[unsafe(no_mangle)]
pub extern "C" fn autd3_link_soem_option_new() -> *mut SoemLinkOptionHandle {
    into_handle(SoemLinkOptionHandle(CoreOption::default()))
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_link_soem_option_safe_default() -> *mut SoemLinkOptionHandle {
    into_handle(SoemLinkOptionHandle(CoreOption::safe_default()))
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_link_soem_option_performance_default() -> *mut SoemLinkOptionHandle {
    into_handle(SoemLinkOptionHandle(CoreOption::performance_default()))
}

autd3_ffi_abi::option_handle_iface!(
    SoemLinkOptionHandle,
    [iface],
    autd3_link_soem_option_set_iface
);
autd3_ffi_abi::option_handle_field!(
    SoemLinkOptionHandle,
    [sync0_period],
    duration,
    autd3_link_soem_option_set_sync0_period,
    autd3_link_soem_option_get_sync0_period
);
autd3_ffi_abi::option_handle_field!(
    SoemLinkOptionHandle,
    [sync0_shift],
    duration,
    autd3_link_soem_option_set_sync0_shift,
    autd3_link_soem_option_get_sync0_shift
);
autd3_ffi_abi::option_handle_field!(
    SoemLinkOptionHandle,
    [sync_tolerance],
    duration,
    autd3_link_soem_option_set_sync_tolerance,
    autd3_link_soem_option_get_sync_tolerance
);
autd3_ffi_abi::option_handle_field!(
    SoemLinkOptionHandle,
    [sync_timeout],
    duration,
    autd3_link_soem_option_set_sync_timeout,
    autd3_link_soem_option_get_sync_timeout
);
autd3_ffi_abi::option_handle_lifecycle!(SoemLinkOptionHandle, autd3_link_soem_option_free);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_soem_open(
    option: *mut SoemLinkOptionHandle,
    out_err: *mut c_char,
    out_err_len: usize,
) -> *mut ClientOpener {
    let Some(SoemLinkOptionHandle(option)) = (unsafe { take_handle(option) }) else {
        unsafe { write_cstr(out_err, out_err_len, OPTION_HANDLE_CONSUMED) };
        return std::ptr::null_mut();
    };
    let opener = client_opener(move |geometry, config| async move {
        let (client, checker) = link_runtime()
            .spawn(async move { Client::open_with_checker(&geometry, option, config).await })
            .await
            .map_err(join_err)??;
        let backend: Box<dyn ClientBackend> = Box::new(SoemBackend {
            client: Arc::new(client),
            checker: Arc::new(Mutex::new(checker)),
        });
        Ok(backend)
    });
    into_handle(opener)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_soem_open_legacy(
    option: *mut SoemLinkOptionHandle,
    out_err: *mut c_char,
    out_err_len: usize,
) -> *mut LegacyClientOpener {
    let Some(SoemLinkOptionHandle(option)) = (unsafe { take_handle(option) }) else {
        unsafe { write_cstr(out_err, out_err_len, OPTION_HANDLE_CONSUMED) };
        return std::ptr::null_mut();
    };
    into_handle(legacy_client_opener(move |_| Ok(option)))
}

autd3_ffi_abi::export_abi_version!();
