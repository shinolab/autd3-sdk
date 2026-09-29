use std::ffi::c_char;
use std::net::SocketAddrV6;
use std::sync::Arc;

use autd3_ffi_abi::{
    AUTD3_ERR_INVALID_ARGUMENT, AUTD3_OK, BoxFuture, CheckerBackend, ClientBackend,
    DeviceStatusData, ResponseTokenData, alloc_cstring, cstr_to_string, free_cstring, handle_mut,
    handle_ref, into_handle, network_err, write_cstr, write_out,
};
use autd3_rs::udp::emulator::UdpEmulator;
use autd3_rs::udp::{StateChecker, TransportOption as CoreOption};
use autd3_rs::{Client, ClientConfig, Error, Frames, Geometry};
use std::sync::Mutex;

pub(crate) struct UdpBackend {
    client: Arc<Client>,
    checker: Arc<Mutex<StateChecker>>,
}

impl ClientBackend for UdpBackend {
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
                        .ok_or_else(|| network_err(format!("frame {index} out of range")))?;
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
                        .ok_or_else(|| network_err(format!("frame {index} out of range")))?;
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
        Box::new(UdpChecker(Arc::clone(&self.checker)))
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

struct UdpChecker(Arc<Mutex<StateChecker>>);

impl CheckerBackend for UdpChecker {
    fn check(&self) -> Result<DeviceStatusData, Error> {
        let status = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .check()
            .map_err(Error::from)?;
        Ok(DeviceStatusData {
            devices: status.devices().to_vec(),
            recoveries: status.recoveries(),
        })
    }
}

pub struct TransportOptionHandle(pub(crate) CoreOption);

#[unsafe(no_mangle)]
pub extern "C" fn autd3_transport_option_new() -> *mut TransportOptionHandle {
    into_handle(TransportOptionHandle(CoreOption::default()))
}

autd3_ffi_abi::option_handle_iface!(
    TransportOptionHandle,
    [iface],
    autd3_transport_option_set_iface
);
autd3_ffi_abi::option_handle_field!(
    TransportOptionHandle,
    [cycle],
    duration,
    autd3_transport_option_set_cycle,
    autd3_transport_option_get_cycle
);
autd3_ffi_abi::option_handle_field!(
    TransportOptionHandle,
    [reply_timeout],
    duration,
    autd3_transport_option_set_reply_timeout,
    autd3_transport_option_get_reply_timeout
);
autd3_ffi_abi::option_handle_field!(
    TransportOptionHandle,
    [response_timeout],
    duration,
    autd3_transport_option_set_response_timeout,
    autd3_transport_option_get_response_timeout
);
autd3_ffi_abi::option_handle_field!(
    TransportOptionHandle,
    [enumeration_timeout],
    duration,
    autd3_transport_option_set_enumeration_timeout,
    autd3_transport_option_get_enumeration_timeout
);
autd3_ffi_abi::option_handle_field!(
    TransportOptionHandle,
    [sync_timeout],
    duration,
    autd3_transport_option_set_sync_timeout,
    autd3_transport_option_get_sync_timeout
);
autd3_ffi_abi::option_handle_lifecycle!(TransportOptionHandle, autd3_transport_option_free);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_transport_option_set_group(
    handle: *mut TransportOptionHandle,
    group: *const c_char,
) -> i32 {
    let Some(option) = (unsafe { handle_mut(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    let group = match unsafe { cstr_to_string(group) } {
        None => None,
        Some(addr) => match addr.parse::<SocketAddrV6>() {
            Ok(addr) => Some(addr),
            Err(_) => return AUTD3_ERR_INVALID_ARGUMENT,
        },
    };
    option.0.group = group;
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_transport_option_get_group(
    handle: *const TransportOptionHandle,
    out: *mut *mut c_char,
) -> i32 {
    let Some(option) = (unsafe { handle_ref(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    let value = option.0.group.map_or(std::ptr::null_mut(), |addr| {
        alloc_cstring(&addr.to_string())
    });
    let code = unsafe { write_out(out, value) };
    if code != AUTD3_OK {
        unsafe { free_cstring(value) };
    }
    code
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_udp_free_string(ptr: *mut c_char) {
    unsafe { free_cstring(ptr) };
}

pub(crate) async fn open(
    geometry: Geometry,
    option: CoreOption,
    config: ClientConfig,
) -> Result<Box<dyn ClientBackend>, Error> {
    let (client, checker) = Client::open_with_checker(&geometry, option, config).await?;
    Ok(Box::new(UdpBackend {
        client: Arc::new(client),
        checker: Arc::new(Mutex::new(checker)),
    }))
}

pub struct UdpEmulatorHandle(UdpEmulator);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_udp_emulator_spawn(
    num_devices: usize,
    out_err: *mut c_char,
    out_err_len: usize,
) -> *mut UdpEmulatorHandle {
    match UdpEmulator::spawn(num_devices) {
        Ok(emulator) => into_handle(UdpEmulatorHandle(emulator)),
        Err(e) => {
            unsafe { write_cstr(out_err, out_err_len, &e.to_string()) };
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_udp_emulator_option(
    handle: *const UdpEmulatorHandle,
) -> *mut TransportOptionHandle {
    let Some(emulator) = (unsafe { handle_ref(handle) }) else {
        return std::ptr::null_mut();
    };
    into_handle(TransportOptionHandle(emulator.0.option()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_udp_emulator_reboot(
    handle: *const UdpEmulatorHandle,
    index: usize,
) -> i32 {
    let Some(emulator) = (unsafe { handle_ref(handle) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    if index >= emulator.0.num_devices() {
        return AUTD3_ERR_INVALID_ARGUMENT;
    }
    emulator.0.reboot(index);
    AUTD3_OK
}

autd3_ffi_abi::option_handle_lifecycle!(UdpEmulatorHandle, autd3_udp_emulator_free);

#[cfg(test)]
mod tests {
    use std::ffi::CString;
    use std::time::Duration;

    use autd3_rs::DeviceState;
    use autd3_rs::geometry::Autd3;

    use super::*;

    fn cycle(handle: *const TransportOptionHandle) -> Duration {
        let mut ns = 0u64;
        assert_eq!(
            unsafe { autd3_transport_option_get_cycle(handle, &raw mut ns) },
            AUTD3_OK
        );
        Duration::from_nanos(ns)
    }

    fn group(handle: *const TransportOptionHandle) -> Option<String> {
        let mut out: *mut c_char = std::ptr::null_mut();
        assert_eq!(
            unsafe { autd3_transport_option_get_group(handle, &raw mut out) },
            AUTD3_OK
        );
        if out.is_null() {
            return None;
        }
        let s = unsafe { std::ffi::CStr::from_ptr(out) }
            .to_string_lossy()
            .into_owned();
        unsafe { autd3_udp_free_string(out) };
        Some(s)
    }

    #[test]
    fn the_defaults_reach_c_unchanged() {
        let handle = autd3_transport_option_new();
        let option = CoreOption::default();
        assert_eq!(cycle(handle), option.cycle);
        let mut ns = 0u64;
        assert_eq!(
            unsafe { autd3_transport_option_get_sync_timeout(handle, &raw mut ns) },
            AUTD3_OK
        );
        assert_eq!(Duration::from_nanos(ns), option.sync_timeout);
        assert_eq!(group(handle), None);
        unsafe { autd3_transport_option_free(handle) };
    }

    #[test]
    fn the_group_round_trips_and_rejects_garbage() {
        let handle = autd3_transport_option_new();
        let addr = CString::new("[::1]:44336").unwrap();
        assert_eq!(
            unsafe { autd3_transport_option_set_group(handle, addr.as_ptr()) },
            AUTD3_OK
        );
        assert_eq!(group(handle).as_deref(), Some("[::1]:44336"));

        let garbage = CString::new("127.0.0.1:1").unwrap();
        assert_eq!(
            unsafe { autd3_transport_option_set_group(handle, garbage.as_ptr()) },
            AUTD3_ERR_INVALID_ARGUMENT
        );
        assert_eq!(group(handle).as_deref(), Some("[::1]:44336"));

        assert_eq!(
            unsafe { autd3_transport_option_set_group(handle, std::ptr::null()) },
            AUTD3_OK
        );
        assert_eq!(group(handle), None);
        unsafe { autd3_transport_option_free(handle) };
    }

    #[test]
    fn a_null_handle_is_an_argument_error_not_a_crash() {
        let mut ns = 0u64;
        assert_eq!(
            unsafe { autd3_transport_option_get_cycle(std::ptr::null(), &raw mut ns) },
            AUTD3_ERR_INVALID_ARGUMENT
        );
        assert!(unsafe { autd3_udp_emulator_option(std::ptr::null()) }.is_null());
    }

    #[test]
    fn the_emulator_opens_a_client() {
        let mut err = [0 as c_char; 256];
        let emulator = unsafe { autd3_udp_emulator_spawn(2, err.as_mut_ptr(), err.len()) };
        assert!(!emulator.is_null());
        let option = unsafe { autd3_udp_emulator_option(emulator) };
        let TransportOptionHandle(option) = *unsafe { Box::from_raw(option) };

        let geometry = Geometry::new(vec![Autd3::default(), Autd3::default()]);
        let backend =
            autd3_rs::rt::block_on(open(geometry, option, ClientConfig::default())).unwrap();
        assert_eq!(backend.num_devices(), 2);
        let status = backend.checker().check().unwrap();
        assert_eq!(status.devices, [DeviceState::Op; 2]);
        autd3_rs::rt::block_on(backend.close()).unwrap();

        assert_eq!(
            unsafe { autd3_udp_emulator_reboot(emulator, 2) },
            AUTD3_ERR_INVALID_ARGUMENT
        );
        unsafe { autd3_udp_emulator_free(emulator) };
    }
}
