use std::ffi::c_char;
use std::sync::Arc;

use autd3_ffi_abi::{
    AUTD3_ERR_INVALID_ARGUMENT, AUTD3_OK, BoxFuture, CheckerBackend, ClientBackend, ClientOpener,
    LegacyClientOpener, LinkStatusData, OPTION_HANDLE_CONSUMED, ResponseTokenData, client_opener,
    from_rt_policy, handle_mut, handle_ref, into_handle, join_err, legacy_client_opener,
    link_runtime, take_handle, to_rt_policy, to_rt_priority, write_cstr, write_out,
};
use autd3_rs::Error;
use autd3_rs::{Client, Frames};
use autd3_rs_core::CoreId;
use autd3_rs_link_ethercrab::{EtherCrabLinkOptionFull as CoreOption, StateChecker};
use tokio::sync::Mutex;

struct EtherCrabBackend {
    client: Arc<Client>,
    checker: Arc<Mutex<StateChecker>>,
}

impl ClientBackend for EtherCrabBackend {
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
        Box::new(EtherCrabChecker(Arc::clone(&self.checker)))
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

struct EtherCrabChecker(Arc<Mutex<StateChecker>>);

impl CheckerBackend for EtherCrabChecker {
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

pub struct EtherCrabLinkOptionHandle(CoreOption);

#[unsafe(no_mangle)]
pub extern "C" fn autd3_link_ethercrab_option_new() -> *mut EtherCrabLinkOptionHandle {
    into_handle(EtherCrabLinkOptionHandle(CoreOption::default()))
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_link_ethercrab_option_safe_default() -> *mut EtherCrabLinkOptionHandle {
    into_handle(EtherCrabLinkOptionHandle(CoreOption::safe_default()))
}

#[unsafe(no_mangle)]
pub extern "C" fn autd3_link_ethercrab_option_performance_default() -> *mut EtherCrabLinkOptionHandle
{
    into_handle(EtherCrabLinkOptionHandle(CoreOption::performance_default()))
}

autd3_ffi_abi::option_handle_iface!(
    EtherCrabLinkOptionHandle,
    [iface],
    autd3_link_ethercrab_option_set_iface
);
autd3_ffi_abi::option_handle_field!(
    EtherCrabLinkOptionHandle,
    [dc_configuration.sync0_period],
    duration,
    autd3_link_ethercrab_option_set_sync0_period,
    autd3_link_ethercrab_option_get_sync0_period
);
autd3_ffi_abi::option_handle_field!(
    EtherCrabLinkOptionHandle,
    [dc_configuration.sync0_shift],
    duration,
    autd3_link_ethercrab_option_set_sync0_shift,
    autd3_link_ethercrab_option_get_sync0_shift
);
autd3_ffi_abi::option_handle_field!(
    EtherCrabLinkOptionHandle,
    [sync_tolerance],
    duration,
    autd3_link_ethercrab_option_set_sync_tolerance,
    autd3_link_ethercrab_option_get_sync_tolerance
);
autd3_ffi_abi::option_handle_field!(
    EtherCrabLinkOptionHandle,
    [sync_timeout],
    duration,
    autd3_link_ethercrab_option_set_sync_timeout,
    autd3_link_ethercrab_option_get_sync_timeout
);
autd3_ffi_abi::option_handle_field!(
    EtherCrabLinkOptionHandle,
    [timeouts.pdu],
    duration,
    autd3_link_ethercrab_option_set_pdu_timeout,
    autd3_link_ethercrab_option_get_pdu_timeout
);
autd3_ffi_abi::option_handle_field!(
    EtherCrabLinkOptionHandle,
    [timeouts.state_transition],
    duration,
    autd3_link_ethercrab_option_set_state_transition_timeout,
    autd3_link_ethercrab_option_get_state_transition_timeout
);
autd3_ffi_abi::option_handle_lifecycle!(
    EtherCrabLinkOptionHandle,
    autd3_link_ethercrab_option_free
);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_ethercrab_option_set_tx_rx_priority(
    handle: *mut EtherCrabLinkOptionHandle,
    mode: u8,
    value: u8,
) -> i32 {
    let Some(option) = (unsafe { handle_mut(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    let Some(priority) = to_rt_priority(mode, value) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    option.0.tx_rx_priority = priority;
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_ethercrab_option_set_tx_rx_policy(
    handle: *mut EtherCrabLinkOptionHandle,
    value: u8,
) -> i32 {
    let Some(option) = (unsafe { handle_mut(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    let Some(policy) = to_rt_policy(value) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    option.0.tx_rx_policy = policy;
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_ethercrab_option_get_tx_rx_policy(
    handle: *const EtherCrabLinkOptionHandle,
    out: *mut u8,
) -> i32 {
    let Some(option) = (unsafe { handle_ref(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    unsafe { write_out(out, from_rt_policy(option.0.tx_rx_policy)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_ethercrab_option_set_tx_rx_affinity(
    handle: *mut EtherCrabLinkOptionHandle,
    has_affinity: bool,
    core_id: usize,
) -> i32 {
    let Some(option) = (unsafe { handle_mut(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    option.0.tx_rx_affinity = has_affinity.then_some(CoreId { id: core_id });
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_ethercrab_open(
    option: *mut EtherCrabLinkOptionHandle,
    out_err: *mut c_char,
    out_err_len: usize,
) -> *mut ClientOpener {
    let Some(EtherCrabLinkOptionHandle(option)) = (unsafe { take_handle(option) }) else {
        unsafe { write_cstr(out_err, out_err_len, OPTION_HANDLE_CONSUMED) };
        return std::ptr::null_mut();
    };
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
    into_handle(opener)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_ethercrab_open_legacy(
    option: *mut EtherCrabLinkOptionHandle,
    out_err: *mut c_char,
    out_err_len: usize,
) -> *mut LegacyClientOpener {
    let Some(EtherCrabLinkOptionHandle(option)) = (unsafe { take_handle(option) }) else {
        unsafe { write_cstr(out_err, out_err_len, OPTION_HANDLE_CONSUMED) };
        return std::ptr::null_mut();
    };
    into_handle(legacy_client_opener(move |_| Ok(option)))
}

autd3_ffi_abi::export_abi_version!();
