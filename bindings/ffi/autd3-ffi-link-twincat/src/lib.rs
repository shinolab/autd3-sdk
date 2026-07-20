use std::ffi::{CStr, c_char};
use std::net::IpAddr;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use autd3_ffi_abi::{
    BoxFuture, CheckerBackend, ClientBackend, ClientOpener, LinkStatusData, ResponseTokenData,
    client_opener, into_handle,
};
use autd3_rs::{Client, Frames};
use autd3_rs_core::{Error, StateCheck};
use autd3_rs_link_twincat::{AmsNetId, Timeouts, TwinCATLinkOption, TwinCATStateChecker};
use tokio::sync::Mutex;

fn build_timeouts(
    has_connect: bool,
    connect_ns: u64,
    has_read: bool,
    read_ns: u64,
    has_write: bool,
    write_ns: u64,
) -> Timeouts {
    Timeouts {
        connect: has_connect.then(|| Duration::from_nanos(connect_ns)),
        read: has_read.then(|| Duration::from_nanos(read_ns)),
        write: has_write.then(|| Duration::from_nanos(write_ns)),
    }
}

fn link_runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to build twincat tokio runtime")
    })
}

#[allow(clippy::needless_pass_by_value)]
fn join_err(e: tokio::task::JoinError) -> Error {
    Error::Link(e.to_string())
}

struct TwinCATBackend {
    client: Arc<Client>,
    checker: Arc<Mutex<TwinCATStateChecker>>,
}

impl ClientBackend for TwinCATBackend {
    fn num_devices(&self) -> usize {
        self.client.num_devices()
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
                        devices: status.devices,
                        recoveries: status.recoveries,
                    })
                })
                .await
                .map_err(join_err)?
        })
    }
}

fn into_opener(option: TwinCATLinkOption) -> *mut ClientOpener {
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
#[allow(clippy::fn_params_excessive_bools)]
pub extern "C" fn autd3_link_twincat_local(
    has_connect: bool,
    connect_ns: u64,
    has_read: bool,
    read_ns: u64,
    has_write: bool,
    write_ns: u64,
) -> *mut ClientOpener {
    into_opener(TwinCATLinkOption::local_with_timeouts(build_timeouts(
        has_connect,
        connect_ns,
        has_read,
        read_ns,
        has_write,
        write_ns,
    )))
}

#[unsafe(no_mangle)]
#[allow(clippy::fn_params_excessive_bools, clippy::too_many_arguments)]
pub unsafe extern "C" fn autd3_link_twincat_remote(
    addr: *const c_char,
    ams_net_id: *const c_char,
    has_connect: bool,
    connect_ns: u64,
    has_read: bool,
    read_ns: u64,
    has_write: bool,
    write_ns: u64,
) -> *mut ClientOpener {
    if addr.is_null() || ams_net_id.is_null() {
        return std::ptr::null_mut();
    }
    let addr = unsafe { CStr::from_ptr(addr) }
        .to_string_lossy()
        .into_owned();
    let ams_net_id = unsafe { CStr::from_ptr(ams_net_id) }
        .to_string_lossy()
        .into_owned();
    let (Ok(addr), Ok(ams_net_id)) = (addr.parse::<IpAddr>(), ams_net_id.parse::<AmsNetId>())
    else {
        return std::ptr::null_mut();
    };
    into_opener(TwinCATLinkOption::remote_with_timeouts(
        addr,
        ams_net_id,
        build_timeouts(
            has_connect,
            connect_ns,
            has_read,
            read_ns,
            has_write,
            write_ns,
        ),
    ))
}
