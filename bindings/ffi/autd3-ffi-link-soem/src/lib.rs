use std::ffi::{CStr, c_char};
use std::sync::Arc;
use std::time::Duration;

use autd3_ffi_abi::{
    BoxFuture, CheckerBackend, ClientBackend, ClientOpener, LegacyClientOpener, LinkStatusData,
    ResponseTokenData, client_opener, into_handle, join_err, legacy_client_opener, link_runtime,
    to_ns,
};
use autd3_rs::Error;
use autd3_rs::{Client, Frames};
use autd3_rs_core::Interface;
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

#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
unsafe fn make_option(
    iface: *const c_char,
    has_sync0_period: bool,
    sync0_period_ns: u64,
    has_sync0_shift: bool,
    sync0_shift_ns: u64,
    has_sync_tolerance: bool,
    sync_tolerance_ns: u64,
    has_sync_timeout: bool,
    sync_timeout_ns: u64,
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
    if has_sync0_shift {
        option.sync0_shift = Duration::from_nanos(sync0_shift_ns);
    }
    if has_sync_tolerance {
        option.sync_tolerance = Duration::from_nanos(sync_tolerance_ns);
    }
    if has_sync_timeout {
        option.sync_timeout = Duration::from_nanos(sync_timeout_ns);
    }
    option
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub unsafe extern "C" fn autd3_link_soem(
    iface: *const c_char,
    has_sync0_period: bool,
    sync0_period_ns: u64,
    has_sync0_shift: bool,
    sync0_shift_ns: u64,
    has_sync_tolerance: bool,
    sync_tolerance_ns: u64,
    has_sync_timeout: bool,
    sync_timeout_ns: u64,
) -> *mut ClientOpener {
    let option = unsafe {
        make_option(
            iface,
            has_sync0_period,
            sync0_period_ns,
            has_sync0_shift,
            sync0_shift_ns,
            has_sync_tolerance,
            sync_tolerance_ns,
            has_sync_timeout,
            sync_timeout_ns,
        )
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
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub unsafe extern "C" fn autd3_link_soem_legacy(
    iface: *const c_char,
    has_sync0_period: bool,
    sync0_period_ns: u64,
    has_sync0_shift: bool,
    sync0_shift_ns: u64,
    has_sync_tolerance: bool,
    sync_tolerance_ns: u64,
    has_sync_timeout: bool,
    sync_timeout_ns: u64,
) -> *mut LegacyClientOpener {
    let option = unsafe {
        make_option(
            iface,
            has_sync0_period,
            sync0_period_ns,
            has_sync0_shift,
            sync0_shift_ns,
            has_sync_tolerance,
            sync_tolerance_ns,
            has_sync_timeout,
            sync_timeout_ns,
        )
    };
    into_handle(legacy_client_opener(move |_| Ok(option)))
}

#[repr(C)]
pub struct Autd3SoemLinkOptionValues {
    pub sync0_period_ns: u64,
    pub sync0_shift_ns: u64,
    pub sync_tolerance_ns: u64,
    pub sync_timeout_ns: u64,
}

unsafe fn write_option(option: &CoreOption, out: *mut Autd3SoemLinkOptionValues) -> i32 {
    if out.is_null() {
        return -1;
    }

    unsafe {
        *out = Autd3SoemLinkOptionValues {
            sync0_period_ns: to_ns(option.sync0_period),
            sync0_shift_ns: to_ns(option.sync0_shift),
            sync_tolerance_ns: to_ns(option.sync_tolerance),
            sync_timeout_ns: to_ns(option.sync_timeout),
        };
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_soem_option_safe_default(
    out: *mut Autd3SoemLinkOptionValues,
) -> i32 {
    unsafe { write_option(&CoreOption::safe_default(), out) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_soem_option_performance_default(
    out: *mut Autd3SoemLinkOptionValues,
) -> i32 {
    unsafe { write_option(&CoreOption::performance_default(), out) }
}
