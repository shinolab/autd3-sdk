use std::ffi::c_char;
use std::net::IpAddr;
use std::sync::Arc;

use autd3_ffi_abi::{
    BoxFuture, CheckerBackend, ClientBackend, ClientOpener, LegacyClientOpener, LinkStatusData,
    ResponseTokenData, client_opener, cstr_to_string, into_handle, join_err, legacy_client_opener,
    link_runtime, take_handle,
};
use autd3_rs::Error;
use autd3_rs::{Client, Frames};
use autd3_rs_core::StateCheck;
use autd3_rs_link_twincat::{AmsNetId, TwinCATLinkOption, TwinCATStateChecker};
use tokio::sync::Mutex;

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
        Box::new(TwinCATChecker(Arc::clone(&self.checker)))
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

struct TwinCATChecker(Arc<Mutex<TwinCATStateChecker>>);

impl CheckerBackend for TwinCATChecker {
    fn check(&self) -> BoxFuture<LinkStatusData> {
        let checker = Arc::clone(&self.0);
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
                        devices: status.devices().to_vec(),
                        recoveries: status.recoveries(),
                    })
                })
                .await
                .map_err(join_err)?
        })
    }
}

pub struct TwinCATLinkOptionHandle(TwinCATLinkOption);

#[unsafe(no_mangle)]
pub extern "C" fn autd3_link_twincat_option_local() -> *mut TwinCATLinkOptionHandle {
    into_handle(TwinCATLinkOptionHandle(TwinCATLinkOption::local()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_twincat_option_remote(
    addr: *const c_char,
    ams_net_id: *const c_char,
) -> *mut TwinCATLinkOptionHandle {
    let (Some(addr), Some(ams_net_id)) = (unsafe { cstr_to_string(addr) }, unsafe {
        cstr_to_string(ams_net_id)
    }) else {
        return std::ptr::null_mut();
    };
    let (Ok(addr), Ok(ams_net_id)) = (addr.parse::<IpAddr>(), ams_net_id.parse::<AmsNetId>())
    else {
        return std::ptr::null_mut();
    };
    into_handle(TwinCATLinkOptionHandle(TwinCATLinkOption::remote(
        addr, ams_net_id,
    )))
}

autd3_ffi_abi::option_handle_opt_duration_field!(
    TwinCATLinkOptionHandle,
    [timeouts.connect],
    autd3_link_twincat_option_set_connect_timeout,
    autd3_link_twincat_option_get_connect_timeout
);
autd3_ffi_abi::option_handle_opt_duration_field!(
    TwinCATLinkOptionHandle,
    [timeouts.read],
    autd3_link_twincat_option_set_read_timeout,
    autd3_link_twincat_option_get_read_timeout
);
autd3_ffi_abi::option_handle_opt_duration_field!(
    TwinCATLinkOptionHandle,
    [timeouts.write],
    autd3_link_twincat_option_set_write_timeout,
    autd3_link_twincat_option_get_write_timeout
);
autd3_ffi_abi::option_handle_lifecycle!(TwinCATLinkOptionHandle, autd3_link_twincat_option_free);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_twincat_open(
    option: *mut TwinCATLinkOptionHandle,
) -> *mut ClientOpener {
    let Some(TwinCATLinkOptionHandle(option)) = (unsafe { take_handle(option) }) else {
        return std::ptr::null_mut();
    };
    let opener = client_opener(move |geometry, config| async move {
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
    into_handle(opener)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_twincat_open_legacy(
    option: *mut TwinCATLinkOptionHandle,
) -> *mut LegacyClientOpener {
    let Some(TwinCATLinkOptionHandle(option)) = (unsafe { take_handle(option) }) else {
        return std::ptr::null_mut();
    };
    into_handle(legacy_client_opener(move |_| Ok(option)))
}

autd3_ffi_abi::export_abi_version!();
