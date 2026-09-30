use std::ffi::c_char;
use std::net::SocketAddrV6;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, TryLockError};
use std::time::{Duration, Instant};

use autd3_ffi_abi::{
    AUTD3_ERR, AUTD3_ERR_INVALID_ARGUMENT, AUTD3_OK, BoxFuture, CheckerBackend, ClientBackend,
    DeviceStatusData, ErrorCategory, ResponseTokenData, alloc_cstring, cstr_to_string,
    free_cstring, handle_mut, handle_ref, into_handle, network_err, take_handle, to_ns, write_cstr,
    write_out,
};
use autd3_rs::driver::Poll;
use autd3_rs::udp::emulator::UdpEmulator;
use autd3_rs::udp::{StateChecker, TransportOption as CoreOption};
use autd3_rs::{Client, ClientConfig, Connector, Driver, Error, Frames, Geometry};

use crate::CheckerHandle;

pub const AUTD3_DRIVER_NEXT: i32 = 0;
pub const AUTD3_DRIVER_CLOSED: i32 = 1;

pub(crate) struct UdpBackend {
    client: Arc<Client>,
}

impl ClientBackend for UdpBackend {
    fn num_devices(&self) -> usize {
        self.client.num_devices()
    }

    fn clock_offset_ns(&self) -> i64 {
        self.client.clock_offset_ns()
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

    fn read_telemetry(&self) -> BoxFuture<Vec<autd3_rs::TelemetryCounters>> {
        let client = Arc::clone(&self.client);
        Box::pin(async move { client.read_telemetry().await })
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
            .unwrap_or_else(PoisonError::into_inner)
            .check()
            .map_err(Error::from)?;
        Ok(DeviceStatusData {
            devices: status.devices().to_vec(),
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
    [heartbeat],
    duration,
    autd3_transport_option_set_heartbeat,
    autd3_transport_option_get_heartbeat
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
    [lost_timeout],
    duration,
    autd3_transport_option_set_lost_timeout,
    autd3_transport_option_get_lost_timeout
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
    connector: Connector,
    config: ClientConfig,
) -> Result<Box<dyn ClientBackend>, Error> {
    let client = Client::open(&geometry, connector, config).await?;
    Ok(Box::new(UdpBackend {
        client: Arc::new(client),
    }))
}

pub struct DriverHandle {
    driver: Mutex<Driver>,
    checker: Arc<Mutex<StateChecker>>,
}

pub struct ConnectorHandle(pub(crate) Connector);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_driver_open(
    option: *mut TransportOptionHandle,
    num_devices: usize,
    out_driver: *mut *mut DriverHandle,
    out_connector: *mut *mut ConnectorHandle,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    if option.is_null() || out_driver.is_null() || out_connector.is_null() {
        unsafe { write_cstr(out_err, out_err_len, "null argument") };
        return AUTD3_ERR_INVALID_ARGUMENT;
    }
    if num_devices == 0 || num_devices > autd3_rs::MAX_DEVICES {
        unsafe {
            write_cstr(
                out_err,
                out_err_len,
                &format!(
                    "the device count {num_devices} is outside 1..={}",
                    autd3_rs::MAX_DEVICES
                ),
            );
        };
        return AUTD3_ERR_INVALID_ARGUMENT;
    }
    let Some(TransportOptionHandle(option)) = (unsafe { take_handle(option) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    match Driver::open(&option, num_devices) {
        Ok((driver, connector)) => {
            let checker = Arc::new(Mutex::new(driver.state_checker()));
            let driver = into_handle(DriverHandle {
                driver: Mutex::new(driver),
                checker,
            });
            let connector = into_handle(ConnectorHandle(connector));
            unsafe {
                write_out(out_driver, driver);
                write_out(out_connector, connector);
            }
            AUTD3_OK
        }
        Err(e) => {
            unsafe { write_cstr(out_err, out_err_len, &e.to_string()) };
            e.error_code()
        }
    }
}

impl DriverHandle {
    fn lock(&self) -> Option<MutexGuard<'_, Driver>> {
        match self.driver.try_lock() {
            Ok(guard) => Some(guard),
            Err(TryLockError::Poisoned(guard)) => Some(guard.into_inner()),
            Err(TryLockError::WouldBlock) => None,
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_driver_poll(
    driver: *const DriverHandle,
    out_wait_ns: *mut u64,
) -> i32 {
    let Some(handle) = (unsafe { handle_ref(driver) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    if out_wait_ns.is_null() {
        return AUTD3_ERR_INVALID_ARGUMENT;
    }
    let Some(mut driver) = handle.lock() else {
        return AUTD3_ERR;
    };
    match driver.poll() {
        Poll::Next(deadline) => unsafe {
            write_out(
                out_wait_ns,
                to_ns(deadline.saturating_duration_since(Instant::now())),
            );
            AUTD3_DRIVER_NEXT
        },
        Poll::Closed => AUTD3_DRIVER_CLOSED,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_driver_wait(driver: *const DriverHandle, wait_ns: u64) -> i32 {
    let Some(handle) = (unsafe { handle_ref(driver) }) else {
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    let Some(mut driver) = handle.lock() else {
        return AUTD3_ERR;
    };
    let now = Instant::now();
    let deadline = now
        .checked_add(Duration::from_nanos(wait_ns))
        .unwrap_or(now + Duration::from_secs(3600));
    driver.wait(deadline);
    AUTD3_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_driver_run(
    driver: *const DriverHandle,
    out_err: *mut c_char,
    out_err_len: usize,
) -> i32 {
    let Some(handle) = (unsafe { handle_ref(driver) }) else {
        unsafe { write_cstr(out_err, out_err_len, "null driver") };
        return AUTD3_ERR_INVALID_ARGUMENT;
    };
    let Some(mut driver) = handle.lock() else {
        unsafe { write_cstr(out_err, out_err_len, "the driver is already being driven") };
        return AUTD3_ERR;
    };
    match driver.run() {
        Ok(()) => AUTD3_OK,
        Err(e) => {
            unsafe { write_cstr(out_err, out_err_len, &e.to_string()) };
            e.error_code()
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn autd3_driver_state_checker(
    driver: *const DriverHandle,
) -> *mut CheckerHandle {
    let Some(handle) = (unsafe { handle_ref(driver) }) else {
        return std::ptr::null_mut();
    };
    into_handle(CheckerHandle(Box::new(UdpChecker(Arc::clone(
        &handle.checker,
    )))))
}

autd3_ffi_abi::option_handle_lifecycle!(DriverHandle, autd3_driver_free);
autd3_ffi_abi::option_handle_lifecycle!(ConnectorHandle, autd3_connector_free);

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

    fn heartbeat(handle: *const TransportOptionHandle) -> Duration {
        let mut ns = 0u64;
        assert_eq!(
            unsafe { autd3_transport_option_get_heartbeat(handle, &raw mut ns) },
            AUTD3_OK
        );
        Duration::from_nanos(ns)
    }

    fn lost_timeout(handle: *const TransportOptionHandle) -> Duration {
        let mut ns = 0u64;
        assert_eq!(
            unsafe { autd3_transport_option_get_lost_timeout(handle, &raw mut ns) },
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
        assert_eq!(heartbeat(handle), option.heartbeat);
        assert_eq!(lost_timeout(handle), option.lost_timeout);
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
            unsafe { autd3_transport_option_get_heartbeat(std::ptr::null(), &raw mut ns) },
            AUTD3_ERR_INVALID_ARGUMENT
        );
        assert!(unsafe { autd3_udp_emulator_option(std::ptr::null()) }.is_null());
    }

    fn open_driver(emulator: *const UdpEmulatorHandle, n: usize) -> (usize, *mut ConnectorHandle) {
        let option = unsafe { autd3_udp_emulator_option(emulator) };
        let mut driver = std::ptr::null_mut();
        let mut connector = std::ptr::null_mut();
        let mut err = [0 as c_char; 256];
        assert_eq!(AUTD3_OK, unsafe {
            autd3_driver_open(
                option,
                n,
                &raw mut driver,
                &raw mut connector,
                err.as_mut_ptr(),
                err.len(),
            )
        });
        (driver as usize, connector)
    }

    fn spawn_emulator(n: usize) -> *mut UdpEmulatorHandle {
        let mut err = [0 as c_char; 256];
        let emulator = unsafe { autd3_udp_emulator_spawn(n, err.as_mut_ptr(), err.len()) };
        assert!(!emulator.is_null());
        emulator
    }

    #[test]
    fn the_emulator_opens_a_client() {
        let emulator = spawn_emulator(2);
        let (driver, connector) = open_driver(emulator, 2);
        let checker = unsafe { autd3_driver_state_checker(driver as *const DriverHandle) };
        assert!(!checker.is_null());
        let runner = std::thread::spawn(move || unsafe {
            autd3_driver_run(driver as *const DriverHandle, std::ptr::null_mut(), 0)
        });

        let ConnectorHandle(connector) = *unsafe { Box::from_raw(connector) };
        let geometry = Geometry::new(vec![Autd3::default(), Autd3::default()]);
        let backend =
            autd3_rs::rt::block_on(open(geometry, connector, ClientConfig::default())).unwrap();
        assert_eq!(backend.num_devices(), 2);
        let status = unsafe { &*checker }.0.check().unwrap();
        assert_eq!(status.devices, [DeviceState::Ready; 2]);
        let telemetry = autd3_rs::rt::block_on(backend.read_telemetry()).unwrap();
        assert_eq!(telemetry.len(), 2);
        autd3_rs::rt::block_on(backend.close()).unwrap();
        assert_eq!(runner.join().unwrap(), AUTD3_OK);
        assert!(unsafe { &*checker }.0.check().is_err());

        let mut wait_ns = 0;
        assert_eq!(AUTD3_DRIVER_CLOSED, unsafe {
            autd3_driver_poll(driver as *const DriverHandle, &raw mut wait_ns)
        });
        unsafe { crate::autd3_checker_free(checker) };
        unsafe { autd3_driver_free(driver as *mut DriverHandle) };

        assert_eq!(
            unsafe { autd3_udp_emulator_reboot(emulator, 2) },
            AUTD3_ERR_INVALID_ARGUMENT
        );
        unsafe { autd3_udp_emulator_free(emulator) };
    }

    #[test]
    fn a_driver_can_be_polled_by_the_caller() {
        let emulator = spawn_emulator(1);
        let (driver, connector) = open_driver(emulator, 1);
        let runner = std::thread::spawn(move || {
            let driver = driver as *const DriverHandle;
            let mut wait_ns = 0u64;
            while unsafe { autd3_driver_poll(driver, &raw mut wait_ns) } == AUTD3_DRIVER_NEXT {
                assert_eq!(AUTD3_OK, unsafe { autd3_driver_wait(driver, wait_ns) });
            }
        });

        let ConnectorHandle(connector) = *unsafe { Box::from_raw(connector) };
        let backend = autd3_rs::rt::block_on(open(
            Geometry::new(vec![Autd3::default()]),
            connector,
            ClientConfig::default(),
        ))
        .unwrap();
        autd3_rs::rt::block_on(backend.read_firmware_version()).unwrap();
        autd3_rs::rt::block_on(backend.close()).unwrap();
        runner.join().unwrap();
        unsafe { autd3_driver_free(driver as *mut DriverHandle) };
        unsafe { autd3_udp_emulator_free(emulator) };
    }

    #[test]
    fn a_rejected_driver_open_leaves_the_option_with_the_caller() {
        let option = autd3_transport_option_new();
        let mut connector = std::ptr::null_mut();
        assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
            autd3_driver_open(
                option,
                1,
                std::ptr::null_mut(),
                &raw mut connector,
                std::ptr::null_mut(),
                0,
            )
        });
        assert!(connector.is_null());
        unsafe { autd3_transport_option_free(option) };
    }

    #[test]
    fn an_out_of_range_device_count_leaves_the_option_with_the_caller() {
        let option = autd3_transport_option_new();
        let mut driver = std::ptr::null_mut();
        let mut connector = std::ptr::null_mut();
        for num_devices in [0, autd3_rs::MAX_DEVICES + 1] {
            assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, unsafe {
                autd3_driver_open(
                    option,
                    num_devices,
                    &raw mut driver,
                    &raw mut connector,
                    std::ptr::null_mut(),
                    0,
                )
            });
            assert!(driver.is_null());
            assert!(connector.is_null());
        }
        let mut ns = 0u64;
        assert_eq!(
            unsafe { autd3_transport_option_get_heartbeat(option, &raw mut ns) },
            AUTD3_OK
        );
        unsafe { autd3_transport_option_free(option) };
    }

    #[test]
    fn a_rejected_client_open_leaves_the_connector_with_the_caller() {
        extern "C" fn never_reports_success(
            code: i32,
            _value: *mut std::ffi::c_void,
            _msg: *const c_char,
            _user_data: *mut std::ffi::c_void,
        ) {
            assert_eq!(AUTD3_ERR_INVALID_ARGUMENT, code);
        }

        let emulator = spawn_emulator(1);
        let (driver, connector) = open_driver(emulator, 1);
        let config = crate::autd3_client_config_new();
        unsafe {
            crate::autd3_client_open(
                std::ptr::null(),
                connector,
                config,
                Some(never_reports_success),
                std::ptr::null_mut(),
            );
        }
        unsafe { autd3_connector_free(connector) };
        assert_eq!(AUTD3_OK, unsafe {
            autd3_driver_run(driver as *const DriverHandle, std::ptr::null_mut(), 0)
        });
        unsafe { crate::autd3_client_config_free(config) };
        unsafe { autd3_driver_free(driver as *mut DriverHandle) };
        unsafe { autd3_udp_emulator_free(emulator) };
    }
}
