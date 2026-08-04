use std::ffi::{CStr, c_char};
use std::sync::Arc;
use std::time::Duration;

use autd3_ffi_abi::{
    BoxFuture, CheckerBackend, ClientBackend, ClientOpener, LegacyClientOpener, LinkStatusData,
    ResponseTokenData, client_opener, into_handle, join_err, legacy_client_opener, link_runtime,
};
use autd3_rs::Error;
use autd3_rs::{Client, Frames};
use autd3_rs_core::Interface;
use autd3_rs_link_echocat::{EchocatLinkOption as CoreOption, SleepStrategy, StateChecker};
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

#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
unsafe fn make_option(
    iface: *const c_char,
    has_sync0_period: bool,
    sync0_period_ns: u64,
    has_pdu_timeout: bool,
    pdu_timeout_ns: u64,
    has_state_transition_timeout: bool,
    state_transition_timeout_ns: u64,
    has_dc_static_sync_iterations: bool,
    dc_static_sync_iterations: u32,
    has_dc_start_delay: bool,
    dc_start_delay_ns: u64,
    has_sync_tolerance: bool,
    sync_tolerance_ns: u64,
    has_sync_timeout: bool,
    sync_timeout_ns: u64,
    has_process_data_watchdog: bool,
    process_data_watchdog_ns: u64,
    has_spin_margin: bool,
    spin_margin_ns: u64,
) -> CoreOption {
    let iface = if iface.is_null() {
        None
    } else {
        Some(
            unsafe { CStr::from_ptr(iface) }
                .to_string_lossy()
                .into_owned(),
        )
    };
    let mut option = CoreOption {
        iface: Interface::from(iface),
        ..CoreOption::default()
    };
    if has_sync0_period {
        option.sync0_period = Duration::from_nanos(sync0_period_ns);
    }
    if has_pdu_timeout {
        option.pdu_timeout = Duration::from_nanos(pdu_timeout_ns);
    }
    if has_state_transition_timeout {
        option.state_transition_timeout = Duration::from_nanos(state_transition_timeout_ns);
    }
    if has_dc_static_sync_iterations {
        option.dc_static_sync_iterations = dc_static_sync_iterations;
    }
    if has_dc_start_delay {
        option.dc_start_delay = Duration::from_nanos(dc_start_delay_ns);
    }
    if has_sync_tolerance {
        option.sync_tolerance = Duration::from_nanos(sync_tolerance_ns);
    }
    if has_sync_timeout {
        option.sync_timeout = Duration::from_nanos(sync_timeout_ns);
    }
    if has_process_data_watchdog {
        option.process_data_watchdog = Duration::from_nanos(process_data_watchdog_ns);
    }
    if has_spin_margin {
        option.sleep_strategy = SleepStrategy::Spin {
            margin: Duration::from_nanos(spin_margin_ns),
        };
    }
    option
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub unsafe extern "C" fn autd3_link_echocat(
    iface: *const c_char,
    has_sync0_period: bool,
    sync0_period_ns: u64,
    has_pdu_timeout: bool,
    pdu_timeout_ns: u64,
    has_state_transition_timeout: bool,
    state_transition_timeout_ns: u64,
    has_dc_static_sync_iterations: bool,
    dc_static_sync_iterations: u32,
    has_dc_start_delay: bool,
    dc_start_delay_ns: u64,
    has_sync_tolerance: bool,
    sync_tolerance_ns: u64,
    has_sync_timeout: bool,
    sync_timeout_ns: u64,
    has_process_data_watchdog: bool,
    process_data_watchdog_ns: u64,
    has_spin_margin: bool,
    spin_margin_ns: u64,
) -> *mut ClientOpener {
    let option = unsafe {
        make_option(
            iface,
            has_sync0_period,
            sync0_period_ns,
            has_pdu_timeout,
            pdu_timeout_ns,
            has_state_transition_timeout,
            state_transition_timeout_ns,
            has_dc_static_sync_iterations,
            dc_static_sync_iterations,
            has_dc_start_delay,
            dc_start_delay_ns,
            has_sync_tolerance,
            sync_tolerance_ns,
            has_sync_timeout,
            sync_timeout_ns,
            has_process_data_watchdog,
            process_data_watchdog_ns,
            has_spin_margin,
            spin_margin_ns,
        )
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
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub unsafe extern "C" fn autd3_link_echocat_legacy(
    iface: *const c_char,
    has_sync0_period: bool,
    sync0_period_ns: u64,
    has_pdu_timeout: bool,
    pdu_timeout_ns: u64,
    has_state_transition_timeout: bool,
    state_transition_timeout_ns: u64,
    has_dc_static_sync_iterations: bool,
    dc_static_sync_iterations: u32,
    has_dc_start_delay: bool,
    dc_start_delay_ns: u64,
    has_sync_tolerance: bool,
    sync_tolerance_ns: u64,
    has_sync_timeout: bool,
    sync_timeout_ns: u64,
    has_process_data_watchdog: bool,
    process_data_watchdog_ns: u64,
    has_spin_margin: bool,
    spin_margin_ns: u64,
) -> *mut LegacyClientOpener {
    let option = unsafe {
        make_option(
            iface,
            has_sync0_period,
            sync0_period_ns,
            has_pdu_timeout,
            pdu_timeout_ns,
            has_state_transition_timeout,
            state_transition_timeout_ns,
            has_dc_static_sync_iterations,
            dc_static_sync_iterations,
            has_dc_start_delay,
            dc_start_delay_ns,
            has_sync_tolerance,
            sync_tolerance_ns,
            has_sync_timeout,
            sync_timeout_ns,
            has_process_data_watchdog,
            process_data_watchdog_ns,
            has_spin_margin,
            spin_margin_ns,
        )
    };
    into_handle(legacy_client_opener(move |_| Ok(option)))
}
