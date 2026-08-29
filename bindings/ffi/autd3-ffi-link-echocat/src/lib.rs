use std::ffi::c_char;
use std::sync::Arc;
use std::time::Duration;

use autd3_ffi_abi::{
    AUTD3_ERR_INVALID_ARGUMENT, AUTD3_OK, BoxFuture, CheckerBackend, ClientBackend, ClientOpener,
    LegacyClientOpener, LinkStatusData, OPTION_HANDLE_CONSUMED, ResponseTokenData, client_opener,
    handle_mut, handle_ref, into_handle, join_err, legacy_client_opener, link_runtime, take_handle,
    to_ns, write_cstr, write_out,
};
use autd3_rs::Error;
use autd3_rs::{Client, Frames};
use autd3_rs_link_echocat::{
    EchocatLinkOption as CoreOption, FramePhase, SleepStrategy, StateChecker,
};
use tokio::sync::Mutex;

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
        Box::new(EchocatChecker(Arc::clone(&self.checker)))
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

struct EchocatChecker(Arc<Mutex<StateChecker>>);

impl CheckerBackend for EchocatChecker {
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

pub struct EchocatLinkOptionHandle(CoreOption);

#[unsafe(no_mangle)]
pub extern "C" fn autd3_link_echocat_option_new() -> *mut EchocatLinkOptionHandle {
    into_handle(EchocatLinkOptionHandle(CoreOption::default()))
}

autd3_ffi_abi::option_handle_iface!(
    EchocatLinkOptionHandle,
    [iface],
    autd3_link_echocat_option_set_iface
);
autd3_ffi_abi::option_handle_field!(
    EchocatLinkOptionHandle,
    [sync0_period],
    duration,
    autd3_link_echocat_option_set_sync0_period,
    autd3_link_echocat_option_get_sync0_period
);
autd3_ffi_abi::option_handle_field!(
    EchocatLinkOptionHandle,
    [pdu_timeout],
    duration,
    autd3_link_echocat_option_set_pdu_timeout,
    autd3_link_echocat_option_get_pdu_timeout
);
autd3_ffi_abi::option_handle_field!(
    EchocatLinkOptionHandle,
    [state_transition_timeout],
    duration,
    autd3_link_echocat_option_set_state_transition_timeout,
    autd3_link_echocat_option_get_state_transition_timeout
);
autd3_ffi_abi::option_handle_field!(
    EchocatLinkOptionHandle,
    [dc_static_sync_iterations],
    u32,
    autd3_link_echocat_option_set_dc_static_sync_iterations,
    autd3_link_echocat_option_get_dc_static_sync_iterations
);
autd3_ffi_abi::option_handle_field!(
    EchocatLinkOptionHandle,
    [dc_start_delay],
    duration,
    autd3_link_echocat_option_set_dc_start_delay,
    autd3_link_echocat_option_get_dc_start_delay
);
autd3_ffi_abi::option_handle_field!(
    EchocatLinkOptionHandle,
    [sync_tolerance],
    duration,
    autd3_link_echocat_option_set_sync_tolerance,
    autd3_link_echocat_option_get_sync_tolerance
);
autd3_ffi_abi::option_handle_field!(
    EchocatLinkOptionHandle,
    [sync_timeout],
    duration,
    autd3_link_echocat_option_set_sync_timeout,
    autd3_link_echocat_option_get_sync_timeout
);
autd3_ffi_abi::option_handle_field!(
    EchocatLinkOptionHandle,
    [process_data_watchdog],
    duration,
    autd3_link_echocat_option_set_process_data_watchdog,
    autd3_link_echocat_option_get_process_data_watchdog
);
autd3_ffi_abi::option_handle_lifecycle!(EchocatLinkOptionHandle, autd3_link_echocat_option_free);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_echocat_option_set_frame_phase(
    handle: *mut EchocatLinkOptionHandle,
    has_frame_phase: bool,
    phase_ns: u64,
) -> i32 {
    let Some(option) = (unsafe { handle_mut(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    option.0.frame_phase = if has_frame_phase {
        FramePhase::At(Duration::from_nanos(phase_ns))
    } else {
        FramePhase::Auto
    };
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_echocat_option_get_frame_phase(
    handle: *const EchocatLinkOptionHandle,
    out_has_frame_phase: *mut bool,
    out_phase_ns: *mut u64,
) -> i32 {
    let Some(option) = (unsafe { handle_ref(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    let phase = match option.0.frame_phase {
        FramePhase::At(at) => Some(at),
        _ => None,
    };
    let code = unsafe { write_out(out_has_frame_phase, phase.is_some()) };
    if code != AUTD3_OK {
        return code;
    }
    unsafe { write_out(out_phase_ns, to_ns(phase.unwrap_or_default())) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_echocat_option_set_spin_margin(
    handle: *mut EchocatLinkOptionHandle,
    has_spin_margin: bool,
    margin_ns: u64,
) -> i32 {
    let Some(option) = (unsafe { handle_mut(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    option.0.sleep_strategy = if has_spin_margin {
        SleepStrategy::Spin {
            margin: Duration::from_nanos(margin_ns),
        }
    } else {
        SleepStrategy::Sleep
    };
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_echocat_option_get_spin_margin(
    handle: *const EchocatLinkOptionHandle,
    out_has_spin_margin: *mut bool,
    out_margin_ns: *mut u64,
) -> i32 {
    let Some(option) = (unsafe { handle_ref(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    let margin = match option.0.sleep_strategy {
        SleepStrategy::Spin { margin } => Some(margin),
        _ => None,
    };
    let code = unsafe { write_out(out_has_spin_margin, margin.is_some()) };
    if code != AUTD3_OK {
        return code;
    }
    unsafe { write_out(out_margin_ns, to_ns(margin.unwrap_or_default())) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_echocat_open(
    option: *mut EchocatLinkOptionHandle,
    out_err: *mut c_char,
    out_err_len: usize,
) -> *mut ClientOpener {
    let Some(EchocatLinkOptionHandle(option)) = (unsafe { take_handle(option) }) else {
        unsafe { write_cstr(out_err, out_err_len, OPTION_HANDLE_CONSUMED) };
        return std::ptr::null_mut();
    };
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
    into_handle(opener)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_echocat_open_legacy(
    option: *mut EchocatLinkOptionHandle,
    out_err: *mut c_char,
    out_err_len: usize,
) -> *mut LegacyClientOpener {
    let Some(EchocatLinkOptionHandle(option)) = (unsafe { take_handle(option) }) else {
        unsafe { write_cstr(out_err, out_err_len, OPTION_HANDLE_CONSUMED) };
        return std::ptr::null_mut();
    };
    into_handle(legacy_client_opener(move |_| Ok(option)))
}

autd3_ffi_abi::export_abi_version!();

#[cfg(test)]
mod tests {
    use super::*;

    fn frame_phase(handle: *const EchocatLinkOptionHandle) -> Option<Duration> {
        let mut present = false;
        let mut ns = 0u64;
        assert_eq!(
            unsafe {
                autd3_link_echocat_option_get_frame_phase(handle, &raw mut present, &raw mut ns)
            },
            AUTD3_OK,
        );
        present.then(|| Duration::from_nanos(ns))
    }

    #[test]
    fn the_landing_phase_survives_the_c_boundary_in_both_shapes() {
        let handle = autd3_link_echocat_option_new();
        assert_eq!(
            frame_phase(handle),
            None,
            "the default has to reach C as `auto`"
        );

        assert_eq!(
            unsafe { autd3_link_echocat_option_set_frame_phase(handle, true, 500_000) },
            AUTD3_OK,
        );
        assert_eq!(frame_phase(handle), Some(Duration::from_micros(500)));

        assert_eq!(
            unsafe { autd3_link_echocat_option_set_frame_phase(handle, false, 500_000) },
            AUTD3_OK,
        );
        assert_eq!(
            frame_phase(handle),
            None,
            "clearing it has to go back to `auto`, not to a zero phase on the SYNC0 edge",
        );

        unsafe { autd3_link_echocat_option_free(handle) };
    }

    #[test]
    fn a_null_handle_is_an_argument_error_not_a_crash() {
        let mut present = false;
        let mut ns = 0u64;
        assert_eq!(
            unsafe { autd3_link_echocat_option_set_frame_phase(std::ptr::null_mut(), true, 1) },
            AUTD3_ERR_INVALID_ARGUMENT,
        );
        assert_eq!(
            unsafe {
                autd3_link_echocat_option_get_frame_phase(
                    std::ptr::null(),
                    &raw mut present,
                    &raw mut ns,
                )
            },
            AUTD3_ERR_INVALID_ARGUMENT,
        );
    }
}
