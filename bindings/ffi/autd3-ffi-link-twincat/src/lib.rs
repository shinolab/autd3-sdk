use std::ffi::{CStr, c_char};
use std::net::IpAddr;
use std::sync::{Arc, OnceLock};

use autd3_ffi_abi::{
    BoxFuture, ClientBackend, ClientOpener, LinkStatusData, client_opener, into_handle,
};
use autd3_rs::{Client, Datagrams};
use autd3_rs_core::{Error, StateCheck};
use autd3_rs_link_twincat::{AmsNetId, TwinCATLinkOption, TwinCATRoute, TwinCATStateChecker};
use tokio::sync::Mutex;

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

fn to_route(route: u8) -> TwinCATRoute {
    match route {
        1 => TwinCATRoute::Notify,
        2 => TwinCATRoute::Ads,
        _ => TwinCATRoute::Auto,
    }
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

    fn send_checked(&self, datagrams: Arc<Datagrams>, frame: Option<usize>) -> BoxFuture<()> {
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

    fn check_status(&self) -> BoxFuture<LinkStatusData> {
        let checker = Arc::clone(&self.checker);
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
                        device_states: status.devices.iter().map(ToString::to_string).collect(),
                        all_op: status.all_op(),
                        any_lost: status.any_lost(),
                        recoveries: status.recoveries,
                    })
                })
                .await
                .map_err(join_err)?
        })
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
pub extern "C" fn autd3_link_twincat_local(route: u8) -> *mut ClientOpener {
    into_opener(TwinCATLinkOption::local().with_route(to_route(route)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_link_twincat_remote(
    addr: *const c_char,
    ams_net_id: *const c_char,
    route: u8,
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
    into_opener(TwinCATLinkOption::remote(addr, ams_net_id).with_route(to_route(route)))
}
