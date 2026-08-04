use std::ffi::{CStr, c_char};
use std::net::SocketAddr;
use std::sync::Arc;

use autd3_ffi_abi::{
    BoxFuture, CheckerBackend, ClientBackend, ClientOpener, LegacyClientOpener, LinkStatusData,
    ResponseTokenData, alloc_cstring, client_opener, free_cstring, into_handle, join_err,
    legacy_client_opener, link_runtime,
};
use autd3_rs::Error;
use autd3_rs::{Client, Frames};
use autd3_rs_link_remote::{RemoteLinkOption, RemoteStateChecker};
use tokio::sync::Mutex;

struct RemoteBackend {
    client: Arc<Client>,
    checker: Arc<Mutex<RemoteStateChecker>>,
}

impl ClientBackend for RemoteBackend {
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
        Box::new(RemoteChecker(Arc::clone(&self.checker)))
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

struct RemoteChecker(Arc<Mutex<RemoteStateChecker>>);

impl CheckerBackend for RemoteChecker {
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_remote(
    addr: *const c_char,
    timeout_ns: u64,
) -> *mut ClientOpener {
    if addr.is_null() {
        return std::ptr::null_mut();
    }
    let addr = unsafe { CStr::from_ptr(addr) }
        .to_string_lossy()
        .into_owned();
    let Ok(addr) = addr.parse::<SocketAddr>() else {
        return std::ptr::null_mut();
    };
    let opener = client_opener(move |geometry, config| async move {
        let (client, checker) = link_runtime()
            .spawn(async move {
                let option = RemoteLinkOption {
                    addr,
                    timeout: (timeout_ns != 0).then(|| std::time::Duration::from_nanos(timeout_ns)),
                };
                Client::open_with_checker(&geometry, option, config).await
            })
            .await
            .map_err(join_err)??;
        let backend: Box<dyn ClientBackend> = Box::new(RemoteBackend {
            client: Arc::new(client),
            checker: Arc::new(Mutex::new(checker)),
        });
        Ok(backend)
    });
    into_handle(opener)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_remote_legacy(
    addr: *const c_char,
    timeout_ns: u64,
) -> *mut LegacyClientOpener {
    if addr.is_null() {
        return std::ptr::null_mut();
    }
    let addr = unsafe { CStr::from_ptr(addr) }
        .to_string_lossy()
        .into_owned();
    let Ok(addr) = addr.parse::<SocketAddr>() else {
        return std::ptr::null_mut();
    };
    into_handle(legacy_client_opener(move |_| {
        Ok(RemoteLinkOption {
            addr,
            timeout: (timeout_ns != 0).then(|| std::time::Duration::from_nanos(timeout_ns)),
        })
    }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_remote_discover(
    timeout_ns: u64,
    instance: *const c_char,
    link_timeout_ns: *mut u64,
    err: *mut *mut c_char,
) -> *mut c_char {
    if !err.is_null() {
        unsafe { *err = std::ptr::null_mut() };
    }
    if !link_timeout_ns.is_null() {
        unsafe { *link_timeout_ns = 0 };
    }
    let instance = (!instance.is_null()).then(|| {
        unsafe { CStr::from_ptr(instance) }
            .to_string_lossy()
            .into_owned()
    });
    let option = autd3_rs_link_remote::DiscoveryOption {
        timeout: if timeout_ns == 0 {
            autd3_rs_link_remote::DiscoveryOption::default().timeout
        } else {
            std::time::Duration::from_nanos(timeout_ns)
        },
        instance,
    };
    match RemoteLinkOption::discover_with(&option) {
        Ok(option) => {
            if !link_timeout_ns.is_null() {
                unsafe {
                    *link_timeout_ns = option.timeout.map_or(0, |timeout| {
                        u64::try_from(timeout.as_nanos()).unwrap_or(u64::MAX)
                    });
                }
            }
            alloc_cstring(&option.addr.to_string())
        }
        Err(e) => {
            if !err.is_null() {
                unsafe { *err = alloc_cstring(&e.to_string()) };
            }
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_remote_free_string(ptr: *mut c_char) {
    unsafe { free_cstring(ptr) };
}
